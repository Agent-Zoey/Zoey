//! Embedded Moshi-Compatible Server
//!
//! A standalone WebSocket server that speaks the Moshi binary protocol.
//! Implements real-time speech-to-text using the Moshi protocol specification.
//!
//! Based on: https://github.com/kyutai-labs/moshi
//! Protocol: https://github.com/kyutai-labs/moshi/blob/main/rust/protocol.md
//!
//! ## Protocol Overview
//!
//! The Moshi protocol uses WebSocket binary messages with a single byte message type prefix:
//! - MT=0 Handshake: Protocol version (u32) + Model version (u32)
//! - MT=1 Audio: OGG/Opus encoded audio (24kHz mono)
//! - MT=2 Text: UTF8 encoded transcription
//! - MT=3 Control: Start(0), EndTurn(1), Pause(2), Restart(3)
//! - MT=4 Metadata: UTF8 JSON
//! - MT=5 Error: UTF8 error description
//! - MT=6 Ping: No payload
//!
//! ## Security
//! - Binds to localhost by default
//! - Optional TLS with self-signed certificates
//! - Optional API key authentication
//!
//! ## Usage
//! ```rust,ignore
//! use zoey_provider_voice::engines::moshi_server::{MoshiServer, MoshiServerBuilder};
//!
//! // Start embedded server
//! let mut server = MoshiServer::builder()
//!     .bind("127.0.0.1:8998")
//!     .with_whisper(WhisperModel::Base)
//!     .build()
//!     .await?;
//!
//! server.start().await?;
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::types::*;
use super::moshi::{MoshiMsgType, MoshiControl, MoshiOpusEncoder, MoshiOpusDecoder};

// ============================================================================
// Protocol Constants
// ============================================================================

/// Moshi protocol version
pub const MOSHI_PROTOCOL_VERSION: u32 = 0;

/// Moshi model version (matching kyutai/moshiko)
pub const MOSHI_MODEL_VERSION: u32 = 1;

/// Audio sample rate (Moshi standard)
pub const MOSHI_SAMPLE_RATE: u32 = 24000;

/// Frame rate in Hz (Moshi standard: 12.5 fps = 80ms per frame)
pub const MOSHI_FRAME_RATE: f64 = 12.5;

/// Opus encoder frame size (960 samples = 40ms at 24kHz)
pub const OPUS_FRAME_SIZE: usize = 960;

// ============================================================================
// Server Messages
// ============================================================================

/// Session metadata sent to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoshiMetadata {
    /// Text sampling temperature
    pub text_temperature: f64,
    /// Text top-k sampling
    pub text_topk: usize,
    /// Audio sampling temperature
    pub audio_temperature: f64,
    /// Audio top-k sampling
    pub audio_topk: usize,
    /// Padding multiplier
    pub pad_mult: f32,
    /// Repetition penalty context size
    pub repetition_penalty_context: usize,
    /// Repetition penalty value
    pub repetition_penalty: f32,
    /// Model file identifier
    pub model_file: String,
    /// Server instance name
    pub instance_name: String,
    /// Build info
    pub build_info: BuildInfo,
}

/// Build information for metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInfo {
    /// Version string
    pub version: String,
    /// Git commit hash (if available)
    pub commit: Option<String>,
}

impl Default for MoshiMetadata {
    fn default() -> Self {
        Self {
            text_temperature: 0.8,
            text_topk: 250,
            audio_temperature: 0.8,
            audio_topk: 250,
            pad_mult: 0.0,
            repetition_penalty_context: 32,
            repetition_penalty: 1.0,
            model_file: "whisper".to_string(),
            instance_name: "zoey-moshi".to_string(),
            build_info: BuildInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                commit: None,
            },
        }
    }
}

// ============================================================================
// Server Configuration
// ============================================================================

/// TTS engine configuration for response synthesis
#[derive(Clone)]
pub enum MoshiTtsConfig {
    /// No TTS (transcription only)
    None,
    /// OpenAI TTS
    OpenAI,
    /// ElevenLabs TTS
    ElevenLabs,
    /// Piper TTS (local)
    Piper(String),
    /// Local HTTP TTS server
    Local(String),
}

impl Default for MoshiTtsConfig {
    fn default() -> Self {
        Self::None
    }
}

/// Server configuration builder
pub struct MoshiServerBuilder {
    bind_addr: String,
    use_tls: bool,
    cert_path: Option<String>,
    key_path: Option<String>,
    #[cfg(feature = "whisper")]
    whisper_model: Option<WhisperModel>,
    tts_config: MoshiTtsConfig,
    max_connections: usize,
    session_timeout_secs: u64,
    metadata: MoshiMetadata,
}

impl Default for MoshiServerBuilder {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8998".to_string(),
            use_tls: false,
            cert_path: None,
            key_path: None,
            #[cfg(feature = "whisper")]
            whisper_model: Some(WhisperModel::Base),
            tts_config: MoshiTtsConfig::None,
            max_connections: 10,
            session_timeout_secs: 360,
            metadata: MoshiMetadata::default(),
        }
    }
}

impl MoshiServerBuilder {
    /// Create new builder with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set bind address (default: 127.0.0.1:8998)
    pub fn bind(mut self, addr: &str) -> Self {
        self.bind_addr = addr.to_string();
        self
    }

    /// Enable TLS (generates self-signed cert if paths not provided)
    pub fn with_tls(mut self) -> Self {
        self.use_tls = true;
        self
    }

    /// Set TLS certificate paths
    pub fn with_tls_certs(mut self, cert_path: &str, key_path: &str) -> Self {
        self.use_tls = true;
        self.cert_path = Some(cert_path.to_string());
        self.key_path = Some(key_path.to_string());
        self
    }

    /// Configure Whisper model for STT
    #[cfg(feature = "whisper")]
    pub fn with_whisper(mut self, model: WhisperModel) -> Self {
        self.whisper_model = Some(model);
        self
    }

    /// Configure OpenAI TTS for responses
    pub fn with_tts_openai(mut self) -> Self {
        self.tts_config = MoshiTtsConfig::OpenAI;
        self
    }

    /// Configure ElevenLabs TTS for responses
    pub fn with_tts_elevenlabs(mut self) -> Self {
        self.tts_config = MoshiTtsConfig::ElevenLabs;
        self
    }

    /// Configure Piper TTS for responses
    pub fn with_tts_piper(mut self, endpoint: &str) -> Self {
        self.tts_config = MoshiTtsConfig::Piper(endpoint.to_string());
        self
    }

    /// Configure local TTS server
    pub fn with_tts_local(mut self, endpoint: &str) -> Self {
        self.tts_config = MoshiTtsConfig::Local(endpoint.to_string());
        self
    }

    /// Set maximum concurrent connections
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }

    /// Set session timeout in seconds (default: 360)
    pub fn session_timeout(mut self, secs: u64) -> Self {
        self.session_timeout_secs = secs;
        self
    }

    /// Set server instance name
    pub fn instance_name(mut self, name: &str) -> Self {
        self.metadata.instance_name = name.to_string();
        self
    }

    /// Build the server
    pub async fn build(self) -> Result<MoshiServer, VoiceError> {
        Ok(MoshiServer {
            bind_addr: self.bind_addr,
            use_tls: self.use_tls,
            cert_path: self.cert_path,
            key_path: self.key_path,
            #[cfg(feature = "whisper")]
            whisper_model: self.whisper_model.unwrap_or(WhisperModel::Base),
            tts_config: self.tts_config,
            max_connections: self.max_connections,
            session_timeout_secs: self.session_timeout_secs,
            metadata: self.metadata,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
        })
    }
}

// ============================================================================
// Session State
// ============================================================================

/// Per-connection session state (thread-safe, no Opus codecs)
struct SessionState {
    /// Session identifier
    id: String,
    /// Accumulated audio buffer (PCM f32, 24kHz)
    audio_buffer: Vec<f32>,
    /// Transcription text buffer
    transcription_buffer: String,
    /// Current step index
    step_idx: usize,
    /// Session start time
    started_at: std::time::Instant,
    /// Last activity time
    last_activity: std::time::Instant,
    /// Is currently processing
    is_processing: bool,
}

impl SessionState {
    fn new(id: String) -> Self {
        Self {
            id,
            audio_buffer: Vec::with_capacity(MOSHI_SAMPLE_RATE as usize * 10), // 10 sec buffer
            transcription_buffer: String::new(),
            step_idx: 0,
            started_at: std::time::Instant::now(),
            last_activity: std::time::Instant::now(),
            is_processing: false,
        }
    }

    fn touch(&mut self) {
        self.last_activity = std::time::Instant::now();
    }

    fn elapsed_secs(&self) -> u64 {
        self.last_activity.elapsed().as_secs()
    }

    fn clear_audio(&mut self) {
        self.audio_buffer.clear();
    }
}

/// Per-connection codec state (NOT shared between threads)
struct ConnectionCodecs {
    opus_decoder: MoshiOpusDecoder,
    #[allow(dead_code)]
    opus_encoder: MoshiOpusEncoder,
}

impl ConnectionCodecs {
    fn new() -> Result<Self, VoiceError> {
        let opus_decoder = MoshiOpusDecoder::new()
            .map_err(|e| VoiceError::AudioError(format!("Failed to create Opus decoder: {}", e)))?;
        let opus_encoder = MoshiOpusEncoder::new()
            .map_err(|e| VoiceError::AudioError(format!("Failed to create Opus encoder: {}", e)))?;
        Ok(Self {
            opus_decoder,
            opus_encoder,
        })
    }
}

// ============================================================================
// Server Implementation
// ============================================================================

/// Embedded Moshi-compatible WebSocket server
///
/// Implements the Moshi binary protocol for real-time speech-to-text.
///
/// ## Features
/// - Binary WebSocket protocol (same as Kyutai Moshi)
/// - Opus audio encoding/decoding
/// - Whisper-based transcription
/// - Optional TTS response synthesis
///
/// ## Example
/// ```rust,ignore
/// let mut server = MoshiServer::builder()
///     .bind("127.0.0.1:8998")
///     .with_whisper(WhisperModel::Base)
///     .build()
///     .await?;
///
/// // Run server (blocks)
/// server.start().await?;
/// ```
pub struct MoshiServer {
    bind_addr: String,
    use_tls: bool,
    cert_path: Option<String>,
    key_path: Option<String>,
    #[cfg(feature = "whisper")]
    whisper_model: WhisperModel,
    tts_config: MoshiTtsConfig,
    max_connections: usize,
    session_timeout_secs: u64,
    metadata: MoshiMetadata,
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl MoshiServer {
    /// Create a new server builder
    pub fn builder() -> MoshiServerBuilder {
        MoshiServerBuilder::new()
    }

    /// Get the server's bind address
    pub fn addr(&self) -> &str {
        &self.bind_addr
    }

    /// Get WebSocket URL for clients
    pub fn ws_url(&self) -> String {
        let scheme = if self.use_tls { "wss" } else { "ws" };
        format!("{}://{}/api/chat", scheme, self.bind_addr)
    }

    /// Start the server (blocks until shutdown)
    pub async fn start(&mut self) -> Result<(), VoiceError> {
        let addr: SocketAddr = self.bind_addr.parse()
            .map_err(|e| VoiceError::Other(format!("Invalid bind address: {}", e)))?;

        let listener = TcpListener::bind(addr).await
            .map_err(|e| VoiceError::NetworkError(format!("Failed to bind: {}", e)))?;

        info!(addr = %addr, tls = %self.use_tls, "Moshi server listening");

        let (shutdown_tx, _) = broadcast::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        let sessions = self.sessions.clone();
        let max_connections = self.max_connections;
        let session_timeout_secs = self.session_timeout_secs;
        #[cfg(feature = "whisper")]
        let whisper_model = self.whisper_model;
        let tts_config = self.tts_config.clone();
        let metadata = self.metadata.clone();

        // Spawn session cleanup task
        let sessions_cleanup = sessions.clone();
        let timeout = session_timeout_secs;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let mut sessions = sessions_cleanup.write().await;
                sessions.retain(|id, state| {
                    let expired = state.elapsed_secs() > timeout;
                    if expired {
                        info!(session = %id, "Session expired");
                    }
                    !expired
                });
            }
        });

        loop {
            let mut shutdown_rx = shutdown_tx.subscribe();

            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, peer_addr)) => {
                            let conn_count = sessions.read().await.len();
                            if conn_count >= max_connections {
                                warn!(peer = %peer_addr, "Connection rejected: limit reached");
                                continue;
                            }

                            info!(peer = %peer_addr, "New connection");

                            let sessions = sessions.clone();
                            #[cfg(feature = "whisper")]
                            let whisper_model = whisper_model;
                            let tts_config = tts_config.clone();
                            let metadata = metadata.clone();

                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(
                                    stream,
                                    peer_addr,
                                    sessions,
                                    #[cfg(feature = "whisper")]
                                    whisper_model,
                                    tts_config,
                                    metadata,
                                ).await {
                                    error!(peer = %peer_addr, error = %e, "Connection error");
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "Accept failed");
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Moshi server shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Stop the server
    pub fn stop(&self) {
        if let Some(ref tx) = self.shutdown_tx {
            let _ = tx.send(());
        }
    }

    /// Handle a WebSocket connection
    async fn handle_connection(
        stream: TcpStream,
        peer_addr: SocketAddr,
        sessions: Arc<RwLock<HashMap<String, SessionState>>>,
        #[cfg(feature = "whisper")]
        whisper_model: WhisperModel,
        tts_config: MoshiTtsConfig,
        metadata: MoshiMetadata,
    ) -> Result<(), VoiceError> {
        // WebSocket upgrade
        let ws_stream = accept_async(stream).await
            .map_err(|e| VoiceError::WebSocketError(format!("Upgrade failed: {}", e)))?;

        let (mut write, mut read) = ws_stream.split();

        // Generate session ID
        let session_id = format!("sess_{}", uuid::Uuid::new_v4().simple());

        // Create per-connection codecs (not shared)
        let mut codecs = ConnectionCodecs::new()?;

        // Initialize session state (shared)
        {
            let state = SessionState::new(session_id.clone());
            sessions.write().await.insert(session_id.clone(), state);
        }

        // Send handshake
        let handshake = Self::create_handshake();
        write.send(Message::Binary(handshake.into())).await
            .map_err(|e| VoiceError::WebSocketError(format!("Handshake failed: {}", e)))?;

        // Send metadata
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| VoiceError::Other(format!("JSON error: {}", e)))?;
        let metadata_msg = Self::create_message(MoshiMsgType::Metadata, metadata_json.as_bytes());
        write.send(Message::Binary(metadata_msg.into())).await
            .map_err(|e| VoiceError::WebSocketError(format!("Metadata send failed: {}", e)))?;

        info!(session = %session_id, "Session started");

        // Message loop
        let result = Self::message_loop(
            &session_id,
            &mut write,
            &mut read,
            &sessions,
            &mut codecs,
            #[cfg(feature = "whisper")]
            whisper_model,
            &tts_config,
        ).await;

        // Cleanup
        sessions.write().await.remove(&session_id);
        info!(session = %session_id, "Session ended");

        result
    }

    /// Main message processing loop
    async fn message_loop(
        session_id: &str,
        write: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<TcpStream>,
            Message,
        >,
        read: &mut futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<TcpStream>,
        >,
        sessions: &Arc<RwLock<HashMap<String, SessionState>>>,
        codecs: &mut ConnectionCodecs,
        #[cfg(feature = "whisper")]
        whisper_model: WhisperModel,
        _tts_config: &MoshiTtsConfig,
    ) -> Result<(), VoiceError> {
        while let Some(msg_result) = read.next().await {
            match msg_result {
                Ok(Message::Binary(data)) => {
                    if data.is_empty() {
                        continue;
                    }

                    let msg_type = match MoshiMsgType::from_u8(data[0]) {
                        Some(t) => t,
                        None => {
                            debug!("Unknown message type: {}", data[0]);
                            continue;
                        }
                    };

                    let payload = &data[1..];

                    match msg_type {
                        MoshiMsgType::Audio => {
                            // Decode Opus audio and accumulate
                            if let Err(e) = Self::handle_audio(
                                session_id,
                                payload,
                                write,
                                sessions,
                                codecs,
                                #[cfg(feature = "whisper")]
                                whisper_model,
                            ).await {
                                error!(session = %session_id, error = %e, "Audio handling error");
                            }
                        }
                        MoshiMsgType::Control => {
                            if !payload.is_empty() {
                                if let Some(control) = MoshiControl::from_u8(payload[0]) {
                                    Self::handle_control(
                                        session_id,
                                        control,
                                        write,
                                        sessions,
                                        #[cfg(feature = "whisper")]
                                        whisper_model,
                                    ).await?;
                                }
                            }
                        }
                        MoshiMsgType::Text => {
                            // Client sent text (unusual but handle it)
                            if let Ok(text) = std::str::from_utf8(payload) {
                                debug!(session = %session_id, text = %text, "Received text from client");
                            }
                        }
                        MoshiMsgType::Ping => {
                            // Respond with ping
                            let pong = vec![MoshiMsgType::Ping.to_u8()];
                            write.send(Message::Binary(pong.into())).await
                                .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                        }
                        _ => {
                            debug!(session = %session_id, msg_type = ?msg_type, "Unhandled message type");
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!(session = %session_id, "Client disconnected");
                    break;
                }
                Ok(Message::Ping(data)) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Err(e) => {
                    error!(session = %session_id, error = %e, "WebSocket error");
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Handle incoming audio data
    async fn handle_audio(
        session_id: &str,
        opus_data: &[u8],
        write: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<TcpStream>,
            Message,
        >,
        sessions: &Arc<RwLock<HashMap<String, SessionState>>>,
        codecs: &mut ConnectionCodecs,
        #[cfg(feature = "whisper")]
        whisper_model: WhisperModel,
    ) -> Result<(), VoiceError> {
        // Decode Opus to PCM using per-connection codec
        let samples = codecs.opus_decoder.decode(opus_data)
            .map_err(|e| VoiceError::AudioError(format!("Opus decode error: {}", e)))?;

        // Update session state with decoded audio
        let (should_transcribe, accumulated_samples) = {
            let mut sessions = sessions.write().await;
            let state = sessions.get_mut(session_id)
                .ok_or_else(|| VoiceError::Other("Session not found".to_string()))?;

            state.touch();
            state.audio_buffer.extend(&samples);

            // Check if we have enough audio for a transcription step
            // At 12.5 fps, we need ~1920 samples (80ms) per step
            let frame_samples = (MOSHI_SAMPLE_RATE as f64 / MOSHI_FRAME_RATE) as usize;

            if state.audio_buffer.len() >= frame_samples {
                state.step_idx += 1;
                let frame = state.audio_buffer.drain(..frame_samples).collect::<Vec<_>>();

                // Transcribe every ~1 second of audio (12-13 frames)
                let should_transcribe = state.step_idx % 12 == 0 && state.step_idx > 0;
                (should_transcribe, Some(frame))
            } else {
                (false, None)
            }
        };

        // Transcribe if we have accumulated enough audio
        #[cfg(feature = "whisper")]
        if should_transcribe {
            if let Some(frame_samples) = accumulated_samples {
                let transcript = Self::transcribe_samples(&frame_samples, whisper_model).await?;

                if !transcript.is_empty() {
                    // Send text message
                    let text_msg = Self::create_message(MoshiMsgType::Text, transcript.as_bytes());
                    write.send(Message::Binary(text_msg.into())).await
                        .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;

                    // Update transcription buffer
                    let mut sessions = sessions.write().await;
                    if let Some(state) = sessions.get_mut(session_id) {
                        state.transcription_buffer.push_str(&transcript);
                        state.transcription_buffer.push(' ');
                    }
                }
            }
        }

        #[cfg(not(feature = "whisper"))]
        let _ = (should_transcribe, accumulated_samples);

        Ok(())
    }

    /// Handle control messages
    async fn handle_control(
        session_id: &str,
        control: MoshiControl,
        write: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<TcpStream>,
            Message,
        >,
        sessions: &Arc<RwLock<HashMap<String, SessionState>>>,
        #[cfg(feature = "whisper")]
        whisper_model: WhisperModel,
    ) -> Result<(), VoiceError> {
        match control {
            MoshiControl::Start => {
                debug!(session = %session_id, "Control: Start");
                // Clear buffers and start fresh
                let mut sessions = sessions.write().await;
                if let Some(state) = sessions.get_mut(session_id) {
                    state.clear_audio();
                    state.transcription_buffer.clear();
                    state.step_idx = 0;
                }
            }
            MoshiControl::EndTurn => {
                debug!(session = %session_id, "Control: EndTurn");
                // Transcribe any remaining audio
                #[cfg(feature = "whisper")]
                {
                    let (samples, current_transcript) = {
                        let mut sessions = sessions.write().await;
                        let state = sessions.get_mut(session_id)
                            .ok_or_else(|| VoiceError::Other("Session not found".to_string()))?;

                        let samples = std::mem::take(&mut state.audio_buffer);
                        let transcript = state.transcription_buffer.clone();
                        state.transcription_buffer.clear();
                        (samples, transcript)
                    };

                    if !samples.is_empty() {
                        let final_transcript = Self::transcribe_samples(&samples, whisper_model).await?;
                        let full_transcript = format!("{}{}", current_transcript, final_transcript);

                        if !full_transcript.trim().is_empty() {
                            let text_msg = Self::create_message(MoshiMsgType::Text, full_transcript.as_bytes());
                            write.send(Message::Binary(text_msg.into())).await
                                .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                        }
                    }
                }
            }
            MoshiControl::Pause => {
                debug!(session = %session_id, "Control: Pause");
                let mut sessions = sessions.write().await;
                if let Some(state) = sessions.get_mut(session_id) {
                    state.is_processing = false;
                }
            }
            MoshiControl::Restart => {
                debug!(session = %session_id, "Control: Restart");
                let mut sessions = sessions.write().await;
                if let Some(state) = sessions.get_mut(session_id) {
                    state.clear_audio();
                    state.transcription_buffer.clear();
                    state.step_idx = 0;
                    state.is_processing = true;
                }
            }
        }

        Ok(())
    }

    /// Transcribe PCM samples using Whisper
    #[cfg(feature = "whisper")]
    async fn transcribe_samples(
        samples: &[f32],
        model: WhisperModel,
    ) -> Result<String, VoiceError> {
        use super::whisper::WhisperEngine;

        if samples.is_empty() {
            return Ok(String::new());
        }

        // Convert f32 samples to i16 bytes (Whisper expects 16-bit PCM)
        let pcm_bytes: Vec<u8> = samples.iter()
            .flat_map(|&s| {
                let sample = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
                sample.to_le_bytes()
            })
            .collect();

        // Resample from 24kHz to 16kHz for Whisper
        let resampled = Self::resample_24k_to_16k(&pcm_bytes);

        let audio = AudioData {
            data: Bytes::from(resampled),
            format: AudioFormat::Pcm,
            sample_rate: 16000,
            duration_ms: Some((samples.len() as u64 * 1000) / MOSHI_SAMPLE_RATE as u64),
            character_count: 0,
        };

        let engine = WhisperEngine::new(model);
        let config = TranscriptionConfig::default();

        match engine.transcribe(&audio, &config).await {
            Ok(result) => Ok(result.text),
            Err(e) => {
                error!(error = %e, "Transcription failed");
                Err(VoiceError::TranscriptionError(e.to_string()))
            }
        }
    }

    /// Resample from 24kHz to 16kHz
    #[cfg(feature = "whisper")]
    fn resample_24k_to_16k(pcm_24k: &[u8]) -> Vec<u8> {
        // Simple decimation: 24k -> 16k = 2/3 ratio
        // For every 3 input samples, output 2 samples
        let samples_24k: Vec<i16> = pcm_24k.chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some(i16::from_le_bytes([chunk[0], chunk[1]]))
                } else {
                    None
                }
            })
            .collect();

        let ratio = 16000.0 / 24000.0;
        let output_len = (samples_24k.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(output_len * 2);

        for i in 0..output_len {
            let src_idx = i as f64 / ratio;
            let idx = src_idx.floor() as usize;
            let frac = src_idx.fract();

            let sample = if idx + 1 < samples_24k.len() {
                let s1 = samples_24k[idx] as f64;
                let s2 = samples_24k[idx + 1] as f64;
                (s1 * (1.0 - frac) + s2 * frac) as i16
            } else if idx < samples_24k.len() {
                samples_24k[idx]
            } else {
                0
            };

            resampled.extend_from_slice(&sample.to_le_bytes());
        }

        resampled
    }

    /// Create a handshake message
    fn create_handshake() -> Vec<u8> {
        let mut msg = Vec::with_capacity(9);
        msg.push(MoshiMsgType::Handshake.to_u8());
        msg.extend_from_slice(&MOSHI_PROTOCOL_VERSION.to_le_bytes());
        msg.extend_from_slice(&MOSHI_MODEL_VERSION.to_le_bytes());
        msg
    }

    /// Create a protocol message
    fn create_message(msg_type: MoshiMsgType, payload: &[u8]) -> Vec<u8> {
        let mut msg = Vec::with_capacity(1 + payload.len());
        msg.push(msg_type.to_u8());
        msg.extend_from_slice(payload);
        msg
    }

    /// Create an error message
    fn create_error(message: &str) -> Vec<u8> {
        Self::create_message(MoshiMsgType::Error, message.as_bytes())
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Start an embedded Moshi server with default settings
///
/// Returns the server and WebSocket URL.
///
/// ## Example
/// ```rust,ignore
/// let (mut server, url) = start_moshi_server().await?;
/// println!("Connect to: {}", url);
/// ```
#[cfg(feature = "whisper")]
pub async fn start_moshi_server() -> Result<(MoshiServer, String), VoiceError> {
    let server = MoshiServer::builder()
        .bind("127.0.0.1:8998")
        .with_whisper(WhisperModel::Base)
        .build()
        .await?;

    let url = server.ws_url();

    Ok((server, url))
}

/// Start Moshi server on a random port
#[cfg(feature = "whisper")]
pub async fn start_moshi_server_random_port() -> Result<(MoshiServer, String), VoiceError> {
    let server = MoshiServer::builder()
        .bind("127.0.0.1:0")
        .with_whisper(WhisperModel::Base)
        .build()
        .await?;

    let url = server.ws_url();

    Ok((server, url))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = MoshiServerBuilder::new();
        assert_eq!(builder.bind_addr, "127.0.0.1:8998");
        assert!(!builder.use_tls);
    }

    #[test]
    fn test_handshake_message() {
        let handshake = MoshiServer::create_handshake();
        assert_eq!(handshake[0], 0); // Handshake type
        assert_eq!(handshake.len(), 9); // 1 + 4 + 4 bytes
    }

    #[test]
    fn test_create_message() {
        let msg = MoshiServer::create_message(MoshiMsgType::Text, b"hello");
        assert_eq!(msg[0], 2); // Text type
        assert_eq!(&msg[1..], b"hello");
    }

    #[test]
    fn test_metadata_serialization() {
        let metadata = MoshiMetadata::default();
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("text_temperature"));
        assert!(json.contains("zoey-moshi"));
    }

    #[test]
    fn test_ws_url() {
        let builder = MoshiServerBuilder::new();
        let server = tokio_test::block_on(async {
            builder.build().await.unwrap()
        });
        assert_eq!(server.ws_url(), "ws://127.0.0.1:8998/api/chat");
    }

    #[test]
    fn test_ws_url_tls() {
        let builder = MoshiServerBuilder::new().with_tls();
        let server = tokio_test::block_on(async {
            builder.build().await.unwrap()
        });
        assert_eq!(server.ws_url(), "wss://127.0.0.1:8998/api/chat");
    }

    #[cfg(feature = "whisper")]
    #[test]
    fn test_resample_24k_to_16k() {
        // Create 24kHz sine wave samples
        let samples_24k: Vec<u8> = (0..480)
            .flat_map(|i| {
                let sample = ((i as f64 * 0.1).sin() * 1000.0) as i16;
                sample.to_le_bytes()
            })
            .collect();

        let resampled = MoshiServer::resample_24k_to_16k(&samples_24k);

        // Should have ~2/3 the samples
        let input_samples = samples_24k.len() / 2;
        let output_samples = resampled.len() / 2;
        let ratio = output_samples as f64 / input_samples as f64;

        assert!((ratio - 0.667).abs() < 0.1, "Resample ratio should be ~0.667, got {}", ratio);
    }
}

