//! Unmute Realtime Engine - Minimal Low-Latency Implementation
//!
//! This is a bare-minimum implementation focused purely on latency.
//! Key differences from standard unmute.rs:
//! - Persistent WebSocket connection (connect once, reuse forever)
//! - Real-time streaming (no buffering entire utterances)
//! - Pre-configured session (no per-request config overhead)
//! - VAD-driven transcription (server-side voice activity detection)
//!
//! Usage:
//! ```rust
//! let rt = UnmuteRealtime::connect("ws://localhost:8000").await?;
//! 
//! // Stream audio chunks as they arrive (don't wait for complete utterance!)
//! for chunk in audio_chunks {
//!     rt.send_audio(&chunk).await?;
//! }
//! 
//! // Receive transcriptions via callback
//! rt.on_transcription(|text| { /* handle text */ });
//! ```

#[cfg(feature = "unmute")]
use futures_util::{SinkExt, StreamExt, stream::{SplitSink, SplitStream}};
#[cfg(feature = "unmute")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "unmute")]
use std::sync::Arc;
#[cfg(feature = "unmute")]
use tokio::sync::{mpsc, Mutex, RwLock};
#[cfg(feature = "unmute")]
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream, MaybeTlsStream};
#[cfg(feature = "unmute")]
use tracing::{debug, error, info, warn};
#[cfg(feature = "unmute")]
use zoey_core::Result;

#[cfg(feature = "unmute")]
use crate::types::*;

// ============================================================================
// Minimal Protocol Types
// ============================================================================

/// Minimal client message - only what's absolutely needed
#[cfg(feature = "unmute")]
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum RealtimeMessage {
    /// Send audio chunk (base64 encoded)
    #[serde(rename = "input_audio_buffer.append")]
    AudioAppend { audio: String },
    
    /// Commit audio for transcription (call after silence detected)
    #[serde(rename = "input_audio_buffer.commit")]
    AudioCommit,
    
    /// Clear buffer (call when starting new utterance)
    #[serde(rename = "input_audio_buffer.clear")]
    AudioClear,
}

/// Server events we care about
#[cfg(feature = "unmute")]
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    /// Transcription completed
    #[serde(rename = "conversation.item.input_audio_transcription.completed")]
    Transcription { transcript: String },
    
    /// Speech started (VAD)
    #[serde(rename = "input_audio_buffer.speech_started")]
    SpeechStarted { 
        #[serde(default)]
        audio_start_ms: Option<u64> 
    },
    
    /// Speech stopped (VAD)
    #[serde(rename = "input_audio_buffer.speech_stopped")]
    SpeechStopped { 
        #[serde(default)]
        audio_end_ms: Option<u64> 
    },
    
    /// Audio committed
    #[serde(rename = "input_audio_buffer.committed")]
    Committed,
    
    /// Error
    #[serde(rename = "error")]
    Error { error: ErrorDetail },
    
    /// All other messages - ignored
    #[serde(other)]
    Other,
}

#[cfg(feature = "unmute")]
#[derive(Debug, Deserialize)]
pub struct ErrorDetail {
    #[serde(default)]
    pub message: String,
}

// ============================================================================
// Realtime Connection
// ============================================================================

#[cfg(feature = "unmute")]
type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
#[cfg(feature = "unmute")]
type WsSink = SplitSink<WsStream, Message>;
#[cfg(feature = "unmute")]
type WsSource = SplitStream<WsStream>;

/// Persistent realtime connection to Unmute
/// 
/// This maintains a single WebSocket connection and provides
/// methods to stream audio and receive transcriptions with
/// minimal latency.
#[cfg(feature = "unmute")]
pub struct UnmuteRealtime {
    /// WebSocket sender (wrapped for thread safety)
    sender: Arc<Mutex<WsSink>>,
    /// Channel to receive transcriptions
    transcription_rx: mpsc::Receiver<String>,
    /// VAD state - is user currently speaking?
    is_speaking: Arc<RwLock<bool>>,
    /// Connection alive flag
    connected: Arc<RwLock<bool>>,
}

#[cfg(feature = "unmute")]
impl UnmuteRealtime {
    /// Connect to Unmute with minimal configuration
    /// 
    /// This establishes a persistent WebSocket connection that stays
    /// open for the lifetime of this struct. No per-request overhead.
    pub async fn connect(endpoint: &str) -> Result<Self> {
        let ws_url = format!("{}/ws", endpoint.trim_end_matches('/'));
        
        info!(url = %ws_url, "Connecting to Unmute realtime...");
        
        let (ws_stream, _) = connect_async(&ws_url)
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("Failed to connect: {}", e)))?;
        
        let (sender, receiver) = ws_stream.split();
        
        // Create transcription channel
        let (tx, rx) = mpsc::channel::<String>(64);
        let is_speaking = Arc::new(RwLock::new(false));
        let connected = Arc::new(RwLock::new(true));
        
        // Spawn receiver task
        let is_speaking_clone = is_speaking.clone();
        let connected_clone = connected.clone();
        tokio::spawn(async move {
            Self::receive_loop(receiver, tx, is_speaking_clone, connected_clone).await;
        });
        
        info!("Unmute realtime connected");
        
        Ok(Self {
            sender: Arc::new(Mutex::new(sender)),
            transcription_rx: rx,
            is_speaking,
            connected,
        })
    }
    
    /// Receive loop - processes server messages
    async fn receive_loop(
        mut receiver: WsSource,
        tx: mpsc::Sender<String>,
        is_speaking: Arc<RwLock<bool>>,
        connected: Arc<RwLock<bool>>,
    ) {
        while let Some(msg_result) = receiver.next().await {
            match msg_result {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<ServerEvent>(&text) {
                        Ok(ServerEvent::Transcription { transcript }) => {
                            debug!(text = %transcript, "Received transcription");
                            let _ = tx.send(transcript).await;
                        }
                        Ok(ServerEvent::SpeechStarted { .. }) => {
                            debug!("VAD: speech started");
                            *is_speaking.write().await = true;
                        }
                        Ok(ServerEvent::SpeechStopped { .. }) => {
                            debug!("VAD: speech stopped");
                            *is_speaking.write().await = false;
                        }
                        Ok(ServerEvent::Error { error }) => {
                            warn!(error = %error.message, "Unmute error");
                        }
                        Ok(_) => {
                            // Ignore other messages for minimal overhead
                        }
                        Err(e) => {
                            debug!(error = %e, "Failed to parse message");
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Unmute connection closed");
                    break;
                }
                Err(e) => {
                    error!(error = %e, "WebSocket error");
                    break;
                }
                _ => {}
            }
        }
        
        *connected.write().await = false;
    }
    
    /// Send raw audio chunk (16-bit PCM, 16kHz mono)
    /// 
    /// Call this for every audio chunk as it arrives - don't buffer!
    /// The smaller and more frequent the chunks, the lower the latency.
    /// 
    /// Recommended: 20ms chunks (640 bytes at 16kHz mono 16-bit)
    #[inline]
    pub async fn send_audio(&self, pcm_16khz_mono: &[i16]) -> Result<()> {
        // Convert to bytes
        let bytes: Vec<u8> = pcm_16khz_mono
            .iter()
            .flat_map(|&s| s.to_le_bytes())
            .collect();
        
        self.send_audio_bytes(&bytes).await
    }
    
    /// Send raw audio bytes directly (even faster - no conversion)
    #[inline]
    pub async fn send_audio_bytes(&self, pcm_bytes: &[u8]) -> Result<()> {
        let audio = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            pcm_bytes,
        );
        
        let msg = RealtimeMessage::AudioAppend { audio };
        let json = serde_json::to_string(&msg).unwrap();
        
        let mut sender = self.sender.lock().await;
        sender.send(Message::Text(json))
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("Send failed: {}", e)))?;
        
        Ok(())
    }
    
    /// Commit audio buffer - triggers transcription
    /// 
    /// Call this when silence is detected (user stopped speaking).
    /// The server will transcribe everything since last clear/commit.
    pub async fn commit(&self) -> Result<()> {
        let msg = RealtimeMessage::AudioCommit;
        let json = serde_json::to_string(&msg).unwrap();
        
        let mut sender = self.sender.lock().await;
        sender.send(Message::Text(json))
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("Commit failed: {}", e)))?;
        
        Ok(())
    }
    
    /// Clear audio buffer - start fresh
    /// 
    /// Call this before starting a new utterance to discard any
    /// leftover audio from previous interactions.
    pub async fn clear(&self) -> Result<()> {
        let msg = RealtimeMessage::AudioClear;
        let json = serde_json::to_string(&msg).unwrap();
        
        let mut sender = self.sender.lock().await;
        sender.send(Message::Text(json))
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("Clear failed: {}", e)))?;
        
        Ok(())
    }
    
    /// Check if user is currently speaking (VAD state from server)
    pub async fn is_speaking(&self) -> bool {
        *self.is_speaking.read().await
    }
    
    /// Check if connection is still alive
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }
    
    /// Get next transcription (non-blocking)
    /// 
    /// Returns None if no transcription is available.
    pub fn try_recv(&mut self) -> Option<String> {
        self.transcription_rx.try_recv().ok()
    }
    
    /// Wait for next transcription (blocking)
    pub async fn recv(&mut self) -> Option<String> {
        self.transcription_rx.recv().await
    }
    
    /// Get the transcription receiver for external handling
    /// 
    /// Use this to integrate with your own async loop.
    pub fn take_receiver(self) -> mpsc::Receiver<String> {
        self.transcription_rx
    }
}

// ============================================================================
// High-Level Voice Conversation Handler
// ============================================================================

/// Callback for receiving transcriptions
#[cfg(feature = "unmute")]
pub type TranscriptionHandler = Box<dyn Fn(String) + Send + Sync>;

/// Callback for generating responses (async)
#[cfg(feature = "unmute")]
pub type ResponseGenerator = Box<dyn Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>> + Send + Sync>;

/// High-level voice conversation manager
/// 
/// Handles the full voice conversation loop:
/// 1. Receives audio from Discord/other source
/// 2. Streams to Unmute for transcription
/// 3. Calls your response generator
/// 4. (TTS response is handled separately)
#[cfg(feature = "unmute")]
pub struct VoiceConversation {
    /// Realtime connection
    realtime: UnmuteRealtime,
    /// Silence detection threshold in ms
    silence_ms: u64,
    /// Last audio timestamp
    last_audio: std::time::Instant,
    /// Whether we're in an utterance
    in_utterance: bool,
}

#[cfg(feature = "unmute")]
impl VoiceConversation {
    /// Create new voice conversation
    pub async fn connect(endpoint: &str) -> Result<Self> {
        let realtime = UnmuteRealtime::connect(endpoint).await?;
        
        Ok(Self {
            realtime,
            silence_ms: 400, // 400ms silence = end of utterance
            last_audio: std::time::Instant::now(),
            in_utterance: false,
        })
    }
    
    /// Set silence detection threshold (default 400ms)
    pub fn set_silence_threshold(&mut self, ms: u64) {
        self.silence_ms = ms;
    }
    
    /// Process incoming audio chunk
    /// 
    /// Call this for every audio chunk from Discord/etc.
    /// Handles buffering, VAD, and triggering transcription.
    /// 
    /// Returns Some(transcription) when an utterance completes.
    pub async fn process_audio(&mut self, pcm_16khz_mono: &[i16]) -> Result<Option<String>> {
        // Check for energy (simple VAD)
        let rms = Self::calculate_rms(pcm_16khz_mono);
        let has_speech = rms > 300.0; // Adjust threshold as needed
        
        if has_speech {
            // Send audio
            self.realtime.send_audio(pcm_16khz_mono).await?;
            self.last_audio = std::time::Instant::now();
            self.in_utterance = true;
        } else if self.in_utterance {
            // Check for silence timeout
            let silence_elapsed = self.last_audio.elapsed().as_millis() as u64;
            
            if silence_elapsed >= self.silence_ms {
                // Utterance complete - commit and get transcription
                self.realtime.commit().await?;
                self.in_utterance = false;
                
                // Try to get transcription immediately
                if let Some(text) = self.realtime.try_recv() {
                    return Ok(Some(text));
                }
            }
        }
        
        // Check for any pending transcriptions
        Ok(self.realtime.try_recv())
    }
    
    /// Wait for next transcription
    pub async fn next_transcription(&mut self) -> Option<String> {
        self.realtime.recv().await
    }
    
    /// Calculate RMS for VAD
    fn calculate_rms(samples: &[i16]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum / samples.len() as f64).sqrt()
    }
}

// ============================================================================
// Persistent TTS Connection (Low-Latency)
// ============================================================================

/// TTS client message
#[cfg(feature = "unmute")]
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum TTSMessage {
    /// Configure session
    #[serde(rename = "session.update")]
    SessionUpdate { session: TTSSessionConfig },
    
    /// Request TTS generation
    #[serde(rename = "response.create")]
    ResponseCreate { response: TTSResponseConfig },
}

#[cfg(feature = "unmute")]
#[derive(Debug, Serialize)]
pub struct TTSSessionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_audio_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
}

#[cfg(feature = "unmute")]
#[derive(Debug, Serialize)]
pub struct TTSResponseConfig {
    pub instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
}

/// TTS server events
#[cfg(feature = "unmute")]
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum TTSServerEvent {
    /// Audio chunk
    #[serde(rename = "response.audio.delta")]
    AudioDelta { delta: String },
    
    /// Audio complete
    #[serde(rename = "response.audio.done")]
    AudioDone,
    
    /// Response complete
    #[serde(rename = "response.done")]
    ResponseDone,
    
    /// Error
    #[serde(rename = "error")]
    Error { error: ErrorDetail },
    
    /// Other messages
    #[serde(other)]
    Other,
}

/// Persistent TTS connection for low-latency speech synthesis
/// 
/// Maintains a single WebSocket connection for all TTS requests,
/// eliminating connection overhead (~50-200ms) per request.
#[cfg(feature = "unmute")]
pub struct UnmuteTTS {
    /// WebSocket sender
    sender: Arc<Mutex<WsSink>>,
    /// Audio chunk receiver
    audio_rx: Arc<Mutex<mpsc::Receiver<Result<Vec<u8>>>>>,
    /// Audio chunk sender (for receiver task)
    audio_tx: mpsc::Sender<Result<Vec<u8>>>,
    /// Connection alive flag
    connected: Arc<RwLock<bool>>,
    /// Configured voice
    voice: Arc<RwLock<String>>,
}

#[cfg(feature = "unmute")]
impl UnmuteTTS {
    /// Connect to Unmute TTS with persistent connection
    pub async fn connect(endpoint: &str, voice: Option<&str>) -> Result<Self> {
        let ws_url = format!("{}/ws", endpoint.trim_end_matches('/'));
        
        info!(url = %ws_url, "Connecting to Unmute TTS...");
        
        let (ws_stream, _) = connect_async(&ws_url)
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("TTS connect failed: {}", e)))?;
        
        let (sender, receiver) = ws_stream.split();
        let (audio_tx, audio_rx) = mpsc::channel::<Result<Vec<u8>>>(64);
        let connected = Arc::new(RwLock::new(true));
        let voice_name = Arc::new(RwLock::new(voice.unwrap_or("default").to_string()));
        
        // Spawn receiver task
        let connected_clone = connected.clone();
        let tx_clone = audio_tx.clone();
        tokio::spawn(async move {
            Self::receive_loop(receiver, tx_clone, connected_clone).await;
        });
        
        // Configure session
        let sender_arc = Arc::new(Mutex::new(sender));
        let session_config = TTSMessage::SessionUpdate {
            session: TTSSessionConfig {
                output_audio_format: Some("pcm16".to_string()),
                voice: Some(voice.unwrap_or("default").to_string()),
            },
        };
        
        {
            let mut s = sender_arc.lock().await;
            s.send(Message::Text(serde_json::to_string(&session_config).unwrap()))
                .await
                .map_err(|e| VoiceError::WebSocketError(format!("Session config failed: {}", e)))?;
        }
        
        info!("Unmute TTS connected (persistent)");
        
        Ok(Self {
            sender: sender_arc,
            audio_rx: Arc::new(Mutex::new(audio_rx)),
            audio_tx,
            connected,
            voice: voice_name,
        })
    }
    
    /// Receive loop for TTS audio chunks
    async fn receive_loop(
        mut receiver: WsSource,
        tx: mpsc::Sender<Result<Vec<u8>>>,
        connected: Arc<RwLock<bool>>,
    ) {
        while let Some(msg_result) = receiver.next().await {
            match msg_result {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<TTSServerEvent>(&text) {
                        Ok(TTSServerEvent::AudioDelta { delta }) => {
                            // Decode and forward audio chunk
                            if let Ok(data) = base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD,
                                &delta,
                            ) {
                                let _ = tx.send(Ok(data)).await;
                            }
                        }
                        Ok(TTSServerEvent::AudioDone) => {
                            // Send empty to signal end
                            let _ = tx.send(Ok(Vec::new())).await;
                        }
                        Ok(TTSServerEvent::Error { error }) => {
                            warn!(error = %error.message, "TTS error");
                            let _ = tx.send(Err(VoiceError::AudioError(error.message).into())).await;
                        }
                        _ => {}
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("TTS connection closed");
                    break;
                }
                Err(e) => {
                    error!(error = %e, "TTS WebSocket error");
                    break;
                }
                _ => {}
            }
        }
        
        *connected.write().await = false;
    }
    
    /// Synthesize text to speech (streaming)
    /// 
    /// Returns a receiver that yields audio chunks as they're generated.
    /// First chunk typically arrives within 100-300ms.
    pub async fn synthesize(&self, text: &str) -> Result<mpsc::Receiver<Result<Vec<u8>>>> {
        let voice = self.voice.read().await.clone();
        
        let msg = TTSMessage::ResponseCreate {
            response: TTSResponseConfig {
                instructions: text.to_string(),
                voice: Some(voice),
                modalities: Some(vec!["audio".to_string()]),
            },
        };
        
        let start = std::time::Instant::now();
        
        {
            let mut sender = self.sender.lock().await;
            sender.send(Message::Text(serde_json::to_string(&msg).unwrap()))
                .await
                .map_err(|e| VoiceError::WebSocketError(format!("TTS request failed: {}", e)))?;
        }
        
        debug!(latency_ms = %start.elapsed().as_millis(), "TTS request sent");
        
        // Create new channel for this request
        let (tx, rx) = mpsc::channel::<Result<Vec<u8>>>(64);
        let audio_rx = self.audio_rx.clone();
        
        // Forward audio from shared receiver to request-specific receiver
        tokio::spawn(async move {
            let mut receiver = audio_rx.lock().await;
            while let Some(chunk) = receiver.recv().await {
                match &chunk {
                    Ok(data) if data.is_empty() => {
                        // End of audio - forward and stop
                        let _ = tx.send(chunk).await;
                        break;
                    }
                    _ => {
                        if tx.send(chunk).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        
        Ok(rx)
    }
    
    /// Quick synthesize - returns full audio (convenience method)
    pub async fn synthesize_full(&self, text: &str) -> Result<Vec<u8>> {
        let mut rx = self.synthesize(text).await?;
        let mut audio = Vec::new();
        
        while let Some(chunk) = rx.recv().await {
            match chunk {
                Ok(data) if data.is_empty() => break,
                Ok(data) => audio.extend(data),
                Err(e) => return Err(e),
            }
        }
        
        Ok(audio)
    }
    
    /// Check if connection is alive
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }
}

// ============================================================================
// Integration Example
// ============================================================================

/// Example: Minimal Discord integration
/// 
/// ```rust,ignore
/// // In your voice receiver handler:
/// 
/// struct MyVoiceHandler {
///     conversation: Arc<Mutex<VoiceConversation>>,
///     response_tx: mpsc::Sender<String>,
/// }
/// 
/// impl EventHandler for MyVoiceHandler {
///     async fn act(&self, ctx: &EventContext<'_>) {
///         if let EventContext::VoiceTick(tick) = ctx {
///             for (ssrc, data) in tick.speaking.iter() {
///                 if let Some(audio) = &data.decoded_voice {
///                     // Convert to 16kHz mono and process
///                     let mono_16k = convert_48k_stereo_to_16k_mono(audio);
///                     
///                     let mut conv = self.conversation.lock().await;
///                     if let Ok(Some(text)) = conv.process_audio(&mono_16k).await {
///                         // Got transcription! Send to agent
///                         let _ = self.response_tx.send(text).await;
///                     }
///                 }
///             }
///         }
///     }
/// }
/// ```

#[cfg(all(test, feature = "unmute"))]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = RealtimeMessage::AudioAppend { 
            audio: "dGVzdA==".to_string() 
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("input_audio_buffer.append"));
        assert!(json.contains("dGVzdA=="));
    }

    #[test]
    fn test_commit_message() {
        let msg = RealtimeMessage::AudioCommit;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("input_audio_buffer.commit"));
    }
}

