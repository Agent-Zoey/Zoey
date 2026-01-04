//! Local Voice Server - Full Unmute-compatible implementation
//!
//! Voice server with accurate STT and natural TTS:
//! - WebSocket API (Unmute protocol)
//! - Whisper STT (accurate, ~1-2s) or Vosk STT (fast, ~100-200ms)
//! - Piper TTS (~50-100ms latency)
//! - Real-time VAD (Voice Activity Detection)
//! - Streaming audio support
//!
//! Usage:
//! ```bash
//! # Use Whisper (more accurate):
//! voice-server --stt whisper --whisper-model small
//! 
//! # Use Vosk (faster):
//! voice-server --stt vosk --vosk-model ~/.cache/vosk/vosk-model-small-en-us-0.15
//! ```

use axum::{
    extract::State,
    response::IntoResponse,
    routing::get,
    Router,
};
use axum::extract::ws::{WebSocketUpgrade, Message, WebSocket};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser, Debug)]
#[command(name = "voice-server")]
#[command(about = "Local Unmute-compatible voice server with Whisper/Vosk STT + Piper TTS")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "8765", env = "VOICE_SERVER_PORT")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1", env = "VOICE_SERVER_HOST")]
    host: String,

    /// STT engine to use: whisper (accurate) or vosk (fast)
    #[arg(long, default_value = "whisper", env = "STT_ENGINE")]
    stt: String,
    
    /// Whisper model size: tiny, base, small, medium (default: small)
    #[arg(long, default_value = "small", env = "WHISPER_MODEL")]
    whisper_model: String,

    /// Path to Piper binary
    #[arg(long, env = "PIPER_PATH")]
    piper: Option<PathBuf>,

    /// Path to Piper voice model (.onnx)
    #[arg(long, env = "PIPER_MODEL")]
    model: Option<PathBuf>,

    /// Path to Vosk model directory (for vosk STT)
    #[arg(long, env = "VOSK_MODEL")]
    vosk_model: Option<PathBuf>,
    
    /// VAD silence threshold in ms (captures complete sentences)
    #[arg(long, default_value = "800")]
    silence_ms: u64,
    
    /// VAD energy threshold
    #[arg(long, default_value = "400")]
    energy_threshold: f64,
    
    /// Minimum audio length in ms before transcribing
    #[arg(long, default_value = "400")]
    min_audio_ms: u64,
}

// ============================================================================
// Unmute Protocol Types
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    /// Append audio to input buffer
    #[serde(rename = "input_audio_buffer.append")]
    AudioAppend { audio: String },
    
    /// Commit buffer for transcription
    #[serde(rename = "input_audio_buffer.commit")]
    AudioCommit,
    
    /// Clear audio buffer
    #[serde(rename = "input_audio_buffer.clear")]
    AudioClear,
    
    /// Request TTS response
    #[serde(rename = "response.create")]
    ResponseCreate { 
        #[serde(default)]
        response: Option<ResponseConfig> 
    },
    
    /// Update session config
    #[serde(rename = "session.update")]
    SessionUpdate {
        #[serde(default)]
        session: Option<SessionConfig>,
    },
    
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Default)]
struct ResponseConfig {
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    voice: Option<String>,
    #[serde(default)]
    modalities: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct SessionConfig {
    #[serde(default)]
    voice: Option<String>,
    #[serde(default)]
    turn_detection: Option<TurnDetectionConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct TurnDetectionConfig {
    #[serde(rename = "type", default)]
    detection_type: Option<String>,
    #[serde(default)]
    threshold: Option<f64>,
    #[serde(default)]
    silence_duration_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ServerMessage {
    /// Session created
    #[serde(rename = "session.created")]
    SessionCreated { session: SessionInfo },
    
    /// Session updated
    #[serde(rename = "session.updated")]
    SessionUpdated { session: SessionInfo },
    
    /// Audio buffer committed
    #[serde(rename = "input_audio_buffer.committed")]
    AudioCommitted { item_id: String },
    
    /// VAD: speech started
    #[serde(rename = "input_audio_buffer.speech_started")]
    SpeechStarted { audio_start_ms: u64 },
    
    /// VAD: speech stopped
    #[serde(rename = "input_audio_buffer.speech_stopped")]
    SpeechStopped { audio_end_ms: u64 },
    
    /// Transcription completed
    #[serde(rename = "conversation.item.input_audio_transcription.completed")]
    TranscriptionCompleted { 
        transcript: String,
        item_id: String,
    },
    
    /// TTS audio chunk
    #[serde(rename = "response.audio.delta")]
    AudioDelta { 
        delta: String,  // base64 audio
        item_id: String,
    },
    
    /// TTS audio done
    #[serde(rename = "response.audio.done")]
    AudioDone { item_id: String },
    
    /// Response done
    #[serde(rename = "response.done")]
    ResponseDone { 
        response: ResponseInfo,
    },
    
    /// Error
    #[serde(rename = "error")]
    Error { error: ErrorInfo },
}

#[derive(Debug, Serialize)]
struct SessionInfo {
    id: String,
    model: String,
    voice: String,
    turn_detection: TurnDetectionInfo,
}

#[derive(Debug, Serialize)]
struct TurnDetectionInfo {
    #[serde(rename = "type")]
    detection_type: String,
    threshold: f64,
    silence_duration_ms: u64,
}

#[derive(Debug, Serialize)]
struct ResponseInfo {
    id: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct ErrorInfo {
    message: String,
    code: String,
}

// ============================================================================
// STT Engine Enum
// ============================================================================

enum SttEngine {
    Whisper { model_size: String },
    Vosk { model: Arc<vosk::Model> },
}

impl SttEngine {
    fn name(&self) -> &str {
        match self {
            SttEngine::Whisper { .. } => "whisper",
            SttEngine::Vosk { .. } => "vosk",
        }
    }
}

// ============================================================================
// Server State
// ============================================================================

struct ServerState {
    piper_path: PathBuf,
    model_path: PathBuf,
    stt_engine: SttEngine,
    silence_ms: u64,
    energy_threshold: f64,
    min_audio_ms: u64,
}

struct SessionState {
    id: String,
    audio_buffer: Vec<i16>,
    is_speaking: bool,
    speech_start_ms: u64,
    last_speech_time: std::time::Instant,
    session_start: std::time::Instant,
    item_counter: u64,
    voice: String,
    silence_duration_ms: u64,
    energy_threshold: f64,
}

impl SessionState {
    fn new(silence_ms: u64, energy_threshold: f64) -> Self {
        Self {
            id: format!("session_{}", uuid::Uuid::new_v4()),
            audio_buffer: Vec::with_capacity(16000 * 30), // Pre-allocate for 30s
            is_speaking: false,
            speech_start_ms: 0,
            last_speech_time: std::time::Instant::now(),
            session_start: std::time::Instant::now(),
            item_counter: 0,
            voice: "cori".to_string(),
            silence_duration_ms: silence_ms,
            energy_threshold,
        }
    }

    fn next_item_id(&mut self) -> String {
        self.item_counter += 1;
        format!("item_{:04}", self.item_counter)
    }
    
    fn elapsed_ms(&self) -> u64 {
        self.session_start.elapsed().as_millis() as u64
    }
    
    fn silence_duration(&self) -> u64 {
        self.last_speech_time.elapsed().as_millis() as u64
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    // Find Piper binary
    let piper_path = args.piper.unwrap_or_else(|| {
        let paths = [
            PathBuf::from("voices/piper/piper"),
            PathBuf::from("crates/providers/zoey-provider-voice/voices/piper/piper"),
            PathBuf::from("/root/zoey-rust/crates/providers/zoey-provider-voice/voices/piper/piper"),
        ];
        paths.into_iter().find(|p| p.exists()).unwrap_or(PathBuf::from("piper"))
    });
    
    // Find Piper model
    let model_path = args.model.unwrap_or_else(|| {
        let paths = [
            PathBuf::from("voices/models/en_GB-cori-high.onnx"),
            PathBuf::from("crates/providers/zoey-provider-voice/voices/models/en_GB-cori-high.onnx"),
            PathBuf::from("/root/zoey-rust/crates/providers/zoey-provider-voice/voices/models/en_GB-cori-high.onnx"),
        ];
        paths.into_iter().find(|p| p.exists()).unwrap_or(PathBuf::from("model.onnx"))
    });
    
    // Initialize STT engine based on --stt flag
    info!(stt = %args.stt, "Initializing STT engine...");
    
    let stt_engine = if args.stt == "vosk" {
        // Load Vosk model
        let vosk_model_path = args.vosk_model.unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            let paths = [
                PathBuf::from(format!("{}/.cache/vosk/vosk-model-small-en-us-0.15", home)),
                PathBuf::from(format!("{}/.cache/vosk/vosk-model-en-us-0.22", home)),
            ];
            paths.into_iter().find(|p| p.exists()).unwrap_or(PathBuf::from("vosk-model"))
        });
        
        info!(path = %vosk_model_path.display(), "Loading Vosk model...");
        let vosk_model = vosk::Model::new(vosk_model_path.to_str().unwrap())
            .ok_or_else(|| anyhow::anyhow!("Failed to load Vosk model from {:?}", vosk_model_path))?;
        info!("✓ Vosk model loaded");
        
        SttEngine::Vosk { model: Arc::new(vosk_model) }
    } else {
        // Use Whisper (default)
        info!(model = %args.whisper_model, "Using Whisper STT (model will download on first use)");
        SttEngine::Whisper { model_size: args.whisper_model.clone() }
    };
    
    info!(
        piper = %piper_path.display(),
        piper_model = %model_path.display(),
        stt = %stt_engine.name(),
        "Voice server configuration"
    );
    
    let state = Arc::new(ServerState {
        piper_path,
        model_path,
        stt_engine,
        silence_ms: args.silence_ms,
        energy_threshold: args.energy_threshold,
        min_audio_ms: args.min_audio_ms,
    });
    
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .with_state(state.clone());
    
    let addr = format!("{}:{}", args.host, args.port);
    info!("✓ Voice server ready on ws://{}/ws", addr);
    info!("  STT: {} ({})", state.stt_engine.name(), 
        if args.stt == "whisper" { format!("{} model", args.whisper_model) } else { "cached".to_string() });
    info!("  TTS: Piper (~50-100ms latency)");
    info!("  VAD: silence={}ms, min_audio={}ms, threshold={}", args.silence_ms, args.min_audio_ms, args.energy_threshold);
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}

async fn health_handler(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "status": "ok",
        "stt": state.stt_engine.name(),
        "tts": "piper",
        "features": ["vad", "streaming"]
    }))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// ============================================================================
// WebSocket Handler
// ============================================================================

async fn handle_socket(socket: WebSocket, state: Arc<ServerState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut session = SessionState::new(state.silence_ms, state.energy_threshold);
    
    // Send session.created
    let session_msg = ServerMessage::SessionCreated {
        session: SessionInfo {
            id: session.id.clone(),
            model: "local-vosk-piper".to_string(),
            voice: session.voice.clone(),
            turn_detection: TurnDetectionInfo {
                detection_type: "server_vad".to_string(),
                threshold: session.energy_threshold,
                silence_duration_ms: session.silence_duration_ms,
            },
        },
    };
    send_message(&mut sender, &session_msg).await;
    
    info!(session_id = %session.id, "New WebSocket session started");
    
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(client_msg) => {
                        handle_client_message(client_msg, &mut session, &mut sender, &state).await;
                    }
                    Err(e) => {
                        warn!(session_id = %session.id, error = %e, message = %text, "Failed to parse client message");
                        // Send error response
                        send_message(&mut sender, &ServerMessage::Error {
                            error: ErrorInfo {
                                message: format!("Invalid message format: {}", e),
                                code: "parse_error".to_string(),
                            },
                        }).await;
                    }
                }
            }
            Ok(Message::Binary(data)) => {
                // Handle raw binary PCM audio (16-bit LE, 16kHz mono)
                let samples: Vec<i16> = data
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect();
                
                process_audio_samples(&samples, &mut session, &mut sender, &state).await;
            }
            Ok(Message::Close(_)) => {
                info!(session_id = %session.id, "WebSocket closed");
                break;
            }
            Err(e) => {
                error!(session_id = %session.id, error = %e, "WebSocket error");
                break;
            }
            _ => {}
        }
    }
}

async fn handle_client_message(
    msg: ClientMessage,
    session: &mut SessionState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    state: &Arc<ServerState>,
) {
    match msg {
        ClientMessage::AudioAppend { audio } => {
            // Decode base64 audio
            if let Ok(decoded) = base64::decode(&audio) {
                let samples: Vec<i16> = decoded
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect();
                
                process_audio_samples(&samples, session, sender, state).await;
            }
        }
        
        ClientMessage::AudioCommit => {
            if !session.audio_buffer.is_empty() {
                // End speech if active
                if session.is_speaking {
                    session.is_speaking = false;
                    let msg = ServerMessage::SpeechStopped { 
                        audio_end_ms: session.elapsed_ms() 
                    };
                    send_message(sender, &msg).await;
                }
                
                // Transcribe
                let item_id = session.next_item_id();
                send_message(sender, &ServerMessage::AudioCommitted { item_id: item_id.clone() }).await;
                
                let audio_samples = std::mem::take(&mut session.audio_buffer);
                transcribe_and_send(&audio_samples, &item_id, sender, state).await;
            }
        }
        
        ClientMessage::AudioClear => {
            session.audio_buffer.clear();
            if session.is_speaking {
                session.is_speaking = false;
                send_message(sender, &ServerMessage::SpeechStopped { 
                    audio_end_ms: session.elapsed_ms() 
                }).await;
            }
        }
        
        ClientMessage::ResponseCreate { response } => {
            if let Some(config) = response {
                if let Some(text) = config.instructions {
                    let item_id = session.next_item_id();
                    synthesize_and_stream(&text, &item_id, sender, state).await;
                }
            }
        }
        
        ClientMessage::SessionUpdate { session: config } => {
            if let Some(cfg) = config {
                if let Some(voice) = cfg.voice {
                    session.voice = voice;
                }
                if let Some(turn) = cfg.turn_detection {
                    if let Some(threshold) = turn.threshold {
                        session.energy_threshold = threshold;
                    }
                    if let Some(silence) = turn.silence_duration_ms {
                        session.silence_duration_ms = silence;
                    }
                }
            }
            
            send_message(sender, &ServerMessage::SessionUpdated {
                session: SessionInfo {
                    id: session.id.clone(),
                    model: "local-vosk-piper".to_string(),
                    voice: session.voice.clone(),
                    turn_detection: TurnDetectionInfo {
                        detection_type: "server_vad".to_string(),
                        threshold: session.energy_threshold,
                        silence_duration_ms: session.silence_duration_ms,
                    },
                },
            }).await;
        }
        
        ClientMessage::Unknown => {}
    }
}

// ============================================================================
// Audio Processing with VAD
// ============================================================================

async fn process_audio_samples(
    samples: &[i16],
    session: &mut SessionState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    _state: &Arc<ServerState>,
) {
    if samples.is_empty() {
        return;
    }
    
    // Calculate RMS energy for VAD
    let rms = calculate_rms(samples);
    let is_speech = rms > session.energy_threshold;
    
    if is_speech {
        // Speech detected
        if !session.is_speaking {
            // Speech just started
            session.is_speaking = true;
            session.speech_start_ms = session.elapsed_ms();
            send_message(sender, &ServerMessage::SpeechStarted { 
                audio_start_ms: session.speech_start_ms 
            }).await;
            debug!(rms = rms, "Speech started");
        }
        
        session.audio_buffer.extend_from_slice(samples);
        session.last_speech_time = std::time::Instant::now();
    } else if session.is_speaking {
        // Silence during speech - check for end of utterance
        session.audio_buffer.extend_from_slice(samples); // Include trailing silence
        
        if session.silence_duration() > session.silence_duration_ms {
            // End of utterance detected by VAD
            session.is_speaking = false;
            let audio_end_ms = session.elapsed_ms();
            
            send_message(sender, &ServerMessage::SpeechStopped { audio_end_ms }).await;
            debug!(silence_ms = session.silence_duration(), "Speech ended (VAD)");
            
            // Auto-commit on silence (like real Unmute)
            // Note: Don't auto-transcribe here - wait for explicit commit
            // This allows client to decide when to process
        }
    }
}

// ============================================================================
// Transcription - Whisper or Vosk
// ============================================================================

async fn transcribe_and_send(
    samples: &[i16],
    item_id: &str,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    state: &Arc<ServerState>,
) {
    let start = std::time::Instant::now();
    let samples = samples.to_vec();
    
    let result = match &state.stt_engine {
        SttEngine::Whisper { model_size } => {
            transcribe_whisper(&samples, model_size).await
        }
        SttEngine::Vosk { model } => {
            transcribe_vosk(&samples, model.clone()).await
        }
    };
    
    let latency_ms = start.elapsed().as_millis();
    
    match result {
        Ok(text) if !text.trim().is_empty() => {
            info!(latency_ms = latency_ms, text = %text, stt = %state.stt_engine.name(), "Transcription complete");
            send_message(sender, &ServerMessage::TranscriptionCompleted {
                transcript: text,
                item_id: item_id.to_string(),
            }).await;
        }
        Ok(_) => {
            debug!(latency_ms = latency_ms, "Empty transcription (silence)");
        }
        Err(e) => {
            warn!(error = %e, "Transcription failed");
            send_message(sender, &ServerMessage::Error {
                error: ErrorInfo {
                    message: e.to_string(),
                    code: "transcription_failed".to_string(),
                },
            }).await;
        }
    }
}

// Whisper STT using whisper-rs (whisper.cpp)
async fn transcribe_whisper(samples: &[i16], model_size: &str) -> anyhow::Result<String> {
    use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
    
    let model_size = model_size.to_string();
    let samples = samples.to_vec();
    
    tokio::task::spawn_blocking(move || {
        // Get model path (download if needed)
        let model_path = get_whisper_model_path(&model_size)?;
        
        // Create context
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap(),
            WhisperContextParameters::default(),
        ).map_err(|e| anyhow::anyhow!("Failed to create Whisper context: {}", e))?;
        
        // Convert i16 to f32 (Whisper expects float audio)
        let samples_f32: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();
        
        // Configure parameters for speed
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_single_segment(true);
        params.set_no_context(true);
        
        // Run transcription
        let mut whisper_state = ctx.create_state()
            .map_err(|e| anyhow::anyhow!("Failed to create state: {}", e))?;
        
        whisper_state.full(params, &samples_f32)
            .map_err(|e| anyhow::anyhow!("Transcription failed: {}", e))?;
        
        // Collect results
        let num_segments = whisper_state.full_n_segments()
            .map_err(|e| anyhow::anyhow!("Failed to get segments: {}", e))?;
        
        let mut text = String::new();
        for i in 0..num_segments {
            if let Ok(segment) = whisper_state.full_get_segment_text(i) {
                text.push_str(&segment);
                text.push(' ');
            }
        }
        
        Ok(text.trim().to_string())
    }).await?
}

// Download Whisper model if needed
fn get_whisper_model_path(model_size: &str) -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let cache_dir = PathBuf::from(format!("{}/.cache/whisper", home));
    std::fs::create_dir_all(&cache_dir)?;
    
    let model_file = format!("ggml-{}.bin", model_size);
    let model_path = cache_dir.join(&model_file);
    
    if !model_path.exists() {
        info!(model = model_size, "Downloading Whisper model (one-time)...");
        
        let url = format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            model_file
        );
        
        let status = std::process::Command::new("curl")
            .args(["-L", "-o", model_path.to_str().unwrap(), &url])
            .status()?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("Failed to download model"));
        }
        info!(path = %model_path.display(), "Model downloaded");
    }
    
    Ok(model_path)
}

// Vosk STT using cached model
async fn transcribe_vosk(samples: &[i16], model: Arc<vosk::Model>) -> anyhow::Result<String> {
    let samples = samples.to_vec();
    
    tokio::task::spawn_blocking(move || {
        let mut recognizer = vosk::Recognizer::new(&model, 16000.0)
            .ok_or_else(|| anyhow::anyhow!("Failed to create recognizer"))?;
        
        recognizer.set_words(true);
        
        for chunk in samples.chunks(4096) {
            let _ = recognizer.accept_waveform(chunk);
        }
        
        let result = recognizer.final_result();
        let text = result.single()
            .map(|r| r.text.to_string())
            .unwrap_or_default();
        
        Ok(text)
    }).await?
}

// ============================================================================
// TTS Synthesis
// ============================================================================

async fn synthesize_and_stream(
    text: &str,
    item_id: &str,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    state: &Arc<ServerState>,
) {
    let start = std::time::Instant::now();
    
    let result = synthesize_piper(&state.piper_path, &state.model_path, text).await;
    
    match result {
        Ok(audio_bytes) => {
            let latency_ms = start.elapsed().as_millis();
            info!(latency_ms = latency_ms, audio_size = audio_bytes.len(), "TTS synthesis complete");
            
            // Stream audio in chunks (like real Unmute)
            let chunk_size = 4096;
            for chunk in audio_bytes.chunks(chunk_size) {
                send_message(sender, &ServerMessage::AudioDelta {
                    delta: base64::encode(chunk),
                    item_id: item_id.to_string(),
                }).await;
            }
            
            send_message(sender, &ServerMessage::AudioDone { 
                item_id: item_id.to_string() 
            }).await;
            
            send_message(sender, &ServerMessage::ResponseDone {
                response: ResponseInfo {
                    id: format!("resp_{}", item_id),
                    status: "completed".to_string(),
                },
            }).await;
        }
        Err(e) => {
            warn!(error = %e, "TTS synthesis failed");
            send_message(sender, &ServerMessage::Error {
                error: ErrorInfo {
                    message: e.to_string(),
                    code: "tts_failed".to_string(),
                },
            }).await;
        }
    }
}

async fn synthesize_piper(piper_path: &PathBuf, model_path: &PathBuf, text: &str) -> anyhow::Result<Vec<u8>> {
    let mut child = Command::new(piper_path)
        .arg("--model")
        .arg(model_path)
        .arg("--output-raw")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }
    
    let output = child.wait_with_output().await?;
    
    if !output.status.success() {
        return Err(anyhow::anyhow!("Piper process failed"));
    }
    
    Ok(output.stdout)
}

// ============================================================================
// Utilities
// ============================================================================

fn calculate_rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
    (sum / samples.len() as f64).sqrt()
}

async fn send_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMessage,
) {
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = sender.send(Message::Text(json)).await;
    }
}
