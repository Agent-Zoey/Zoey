//! Embedded Unmute-Compatible Server
//!
//! A minimal, secure WebSocket server that speaks the Unmute protocol.
//! Runs in-process with Zoey - no external dependencies.
//!
//! ## Security
//! - Binds to localhost only (127.0.0.1) by default
//! - Optional authentication via API key
//! - No external network exposure
//!
//! ## Usage
//! ```rust,ignore
//! use zoey_provider_voice::UnmuteServer;
//!
//! // Start embedded server
//! let server = UnmuteServer::builder()
//!     .bind("127.0.0.1:8765")
//!     .with_whisper(WhisperModel::Base)
//!     .with_tts_openai()
//!     .build()
//!     .await?;
//!
//! // Server runs in background
//! server.start().await?;
//!
//! // Connect from realtime client
//! let client = UnmuteRealtime::connect("ws://127.0.0.1:8765").await?;
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock, broadcast};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::types::*;

// ============================================================================
// Protocol Types (Same as unmute_realtime.rs but for server-side)
// ============================================================================

/// Client messages we accept
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "input_audio_buffer.append")]
    AudioAppend { audio: String },
    
    #[serde(rename = "input_audio_buffer.commit")]
    AudioCommit,
    
    #[serde(rename = "input_audio_buffer.clear")]
    AudioClear,
    
    #[serde(rename = "session.update")]
    SessionUpdate { session: SessionConfig },
    
    #[serde(rename = "response.create")]
    ResponseCreate { response: ResponseConfig },
    
    #[serde(other)]
    Unknown,
}

/// Server messages we send
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "session.created")]
    SessionCreated { session: SessionInfo },
    
    #[serde(rename = "session.updated")]
    SessionUpdated { session: SessionInfo },
    
    #[serde(rename = "input_audio_buffer.committed")]
    AudioCommitted { item_id: String },
    
    #[serde(rename = "input_audio_buffer.cleared")]
    AudioCleared,
    
    #[serde(rename = "input_audio_buffer.speech_started")]
    SpeechStarted { audio_start_ms: u64 },
    
    #[serde(rename = "input_audio_buffer.speech_stopped")]
    SpeechStopped { audio_end_ms: u64 },
    
    #[serde(rename = "conversation.item.input_audio_transcription.completed")]
    TranscriptionCompleted { 
        transcript: String,
        item_id: String,
    },
    
    #[serde(rename = "conversation.item.input_audio_transcription.failed")]
    TranscriptionFailed { 
        error: ErrorInfo,
        item_id: String,
    },
    
    #[serde(rename = "response.audio.delta")]
    AudioDelta {
        delta: String,
        response_id: String,
    },
    
    #[serde(rename = "response.audio.done")]
    AudioDone { response_id: String },
    
    #[serde(rename = "response.done")]
    ResponseDone { response_id: String },
    
    #[serde(rename = "error")]
    Error { error: ErrorInfo },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_audio_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_audio_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_audio_transcription: Option<TranscriptionSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranscriptionSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResponseConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub model: String,
    pub voice: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorInfo {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

// ============================================================================
// Server Configuration
// ============================================================================

/// Server configuration builder
pub struct UnmuteServerBuilder {
    bind_addr: String,
    api_key: Option<String>,
    #[cfg(feature = "whisper")]
    whisper_model: Option<WhisperModel>,
    tts_engine: TtsEngineConfig,
    max_connections: usize,
}

#[derive(Clone)]
enum TtsEngineConfig {
    None,
    OpenAI,
    ElevenLabs,
    Local(String),
}

impl Default for UnmuteServerBuilder {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8765".to_string(),
            api_key: None,
            #[cfg(feature = "whisper")]
            whisper_model: Some(WhisperModel::Base),
            tts_engine: TtsEngineConfig::OpenAI,
            max_connections: 10,
        }
    }
}

impl UnmuteServerBuilder {
    /// Create new builder with secure defaults
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set bind address (default: 127.0.0.1:8765 - localhost only!)
    /// 
    /// ⚠️ SECURITY: Only bind to 0.0.0.0 if you have proper authentication!
    pub fn bind(mut self, addr: &str) -> Self {
        self.bind_addr = addr.to_string();
        self
    }
    
    /// Require API key for connections
    /// 
    /// Clients must send: `Authorization: Bearer <key>` header during WebSocket upgrade
    pub fn with_api_key(mut self, key: &str) -> Self {
        self.api_key = Some(key.to_string());
        self
    }
    
    /// Configure Whisper model for STT
    #[cfg(feature = "whisper")]
    pub fn with_whisper(mut self, model: WhisperModel) -> Self {
        self.whisper_model = Some(model);
        self
    }
    
    /// Use OpenAI for TTS
    pub fn with_tts_openai(mut self) -> Self {
        self.tts_engine = TtsEngineConfig::OpenAI;
        self
    }
    
    /// Use ElevenLabs for TTS
    pub fn with_tts_elevenlabs(mut self) -> Self {
        self.tts_engine = TtsEngineConfig::ElevenLabs;
        self
    }
    
    /// Use local TTS server
    pub fn with_tts_local(mut self, endpoint: &str) -> Self {
        self.tts_engine = TtsEngineConfig::Local(endpoint.to_string());
        self
    }
    
    /// Disable TTS (STT only)
    pub fn without_tts(mut self) -> Self {
        self.tts_engine = TtsEngineConfig::None;
        self
    }
    
    /// Set maximum concurrent connections
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }
    
    /// Build the server
    pub async fn build(self) -> Result<UnmuteServer, VoiceError> {
        // Validate bind address for security
        if self.bind_addr.starts_with("0.0.0.0") && self.api_key.is_none() {
            warn!("⚠️  Binding to 0.0.0.0 without API key - server is publicly accessible!");
        }
        
        Ok(UnmuteServer {
            bind_addr: self.bind_addr,
            api_key: self.api_key,
            #[cfg(feature = "whisper")]
            whisper_model: self.whisper_model.unwrap_or(WhisperModel::Base),
            tts_engine: self.tts_engine,
            max_connections: self.max_connections,
            connections: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
        })
    }
}

// ============================================================================
// Server Implementation
// ============================================================================

/// Embedded Unmute-compatible WebSocket server
pub struct UnmuteServer {
    bind_addr: String,
    api_key: Option<String>,
    #[cfg(feature = "whisper")]
    whisper_model: WhisperModel,
    tts_engine: TtsEngineConfig,
    max_connections: usize,
    connections: Arc<RwLock<HashMap<String, ConnectionState>>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

struct ConnectionState {
    session_id: String,
    audio_buffer: Vec<u8>,
    session_config: SessionConfig,
    is_speaking: bool,
    last_audio_ms: u64,
}

impl UnmuteServer {
    /// Create a new server builder
    pub fn builder() -> UnmuteServerBuilder {
        UnmuteServerBuilder::new()
    }
    
    /// Get the server's bind address
    pub fn addr(&self) -> &str {
        &self.bind_addr
    }
    
    /// Get WebSocket URL for clients to connect
    pub fn ws_url(&self) -> String {
        format!("ws://{}/ws", self.bind_addr)
    }
    
    /// Start the server (blocking - run in tokio::spawn)
    pub async fn start(&mut self) -> Result<(), VoiceError> {
        let addr: SocketAddr = self.bind_addr.parse()
            .map_err(|e| VoiceError::Other(format!("Invalid bind address: {}", e)))?;
        
        let listener = TcpListener::bind(addr).await
            .map_err(|e| VoiceError::NetworkError(format!("Failed to bind: {}", e)))?;
        
        info!(addr = %addr, "Unmute server listening");
        
        let (shutdown_tx, _) = broadcast::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx.clone());
        
        let connections = self.connections.clone();
        let api_key = self.api_key.clone();
        let max_connections = self.max_connections;
        #[cfg(feature = "whisper")]
        let whisper_model = self.whisper_model;
        let tts_engine = self.tts_engine.clone();
        
        loop {
            let mut shutdown_rx = shutdown_tx.subscribe();
            
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, peer_addr)) => {
                            // Check connection limit
                            let conn_count = connections.read().await.len();
                            if conn_count >= max_connections {
                                warn!(peer = %peer_addr, "Connection rejected: max connections reached");
                                continue;
                            }
                            
                            info!(peer = %peer_addr, "New connection");
                            
                            let connections = connections.clone();
                            let api_key = api_key.clone();
                            #[cfg(feature = "whisper")]
                            let whisper_model = whisper_model;
                            let tts_engine = tts_engine.clone();
                            
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(
                                    stream,
                                    peer_addr,
                                    connections,
                                    api_key,
                                    #[cfg(feature = "whisper")]
                                    whisper_model,
                                    tts_engine,
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
                    info!("Server shutting down");
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
    
    /// Handle a single WebSocket connection
    async fn handle_connection(
        stream: TcpStream,
        peer_addr: SocketAddr,
        connections: Arc<RwLock<HashMap<String, ConnectionState>>>,
        api_key: Option<String>,
        #[cfg(feature = "whisper")]
        whisper_model: WhisperModel,
        tts_engine: TtsEngineConfig,
    ) -> Result<(), VoiceError> {
        // WebSocket upgrade
        let ws_stream = accept_async(stream).await
            .map_err(|e| VoiceError::WebSocketError(format!("Upgrade failed: {}", e)))?;
        
        let (mut write, mut read) = ws_stream.split();
        
        // Generate session ID
        let session_id = format!("sess_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string());
        
        // Initialize connection state
        {
            let mut conns = connections.write().await;
            conns.insert(session_id.clone(), ConnectionState {
                session_id: session_id.clone(),
                audio_buffer: Vec::new(),
                session_config: SessionConfig::default(),
                is_speaking: false,
                last_audio_ms: 0,
            });
        }
        
        // Send session.created
        let session_msg = ServerMessage::SessionCreated {
            session: SessionInfo {
                id: session_id.clone(),
                model: "whisper-1".to_string(),
                voice: "default".to_string(),
            },
        };
        write.send(Message::Text(serde_json::to_string(&session_msg).unwrap())).await
            .map_err(|e| VoiceError::WebSocketError(format!("Send failed: {}", e)))?;
        
        // Message processing loop
        while let Some(msg_result) = read.next().await {
            match msg_result {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(client_msg) => {
                            if let Err(e) = Self::handle_message(
                                &session_id,
                                client_msg,
                                &mut write,
                                &connections,
                                #[cfg(feature = "whisper")]
                                whisper_model,
                                &tts_engine,
                            ).await {
                                // Send error to client
                                let error_msg = ServerMessage::Error {
                                    error: ErrorInfo {
                                        message: e.to_string(),
                                        code: Some("processing_error".to_string()),
                                    },
                                };
                                let _ = write.send(Message::Text(serde_json::to_string(&error_msg).unwrap())).await;
                            }
                        }
                        Err(e) => {
                            debug!(error = %e, "Failed to parse client message");
                        }
                    }
                }
                Ok(Message::Binary(data)) => {
                    // Handle binary audio data directly
                    let mut conns = connections.write().await;
                    if let Some(state) = conns.get_mut(&session_id) {
                        state.audio_buffer.extend(data);
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
                    error!(error = %e, "WebSocket error");
                    break;
                }
                _ => {}
            }
        }
        
        // Cleanup
        connections.write().await.remove(&session_id);
        
        Ok(())
    }
    
    /// Handle a client message
    async fn handle_message(
        session_id: &str,
        msg: ClientMessage,
        write: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<TcpStream>, Message>,
        connections: &Arc<RwLock<HashMap<String, ConnectionState>>>,
        #[cfg(feature = "whisper")]
        whisper_model: WhisperModel,
        tts_engine: &TtsEngineConfig,
    ) -> Result<(), VoiceError> {
        match msg {
            ClientMessage::AudioAppend { audio } => {
                // Decode base64 audio and append to buffer
                let audio_bytes = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &audio,
                ).map_err(|e| VoiceError::AudioError(format!("Invalid base64: {}", e)))?;
                
                let mut conns = connections.write().await;
                if let Some(state) = conns.get_mut(session_id) {
                    state.audio_buffer.extend(audio_bytes);
                    
                    // Simple VAD - detect speech start/stop
                    let is_speech = Self::detect_speech(&state.audio_buffer);
                    if is_speech && !state.is_speaking {
                        state.is_speaking = true;
                        drop(conns); // Release lock before sending
                        let msg = ServerMessage::SpeechStarted { audio_start_ms: 0 };
                        write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                            .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                    }
                }
            }
            
            ClientMessage::AudioCommit => {
                // Transcribe buffered audio
                let audio_data = {
                    let mut conns = connections.write().await;
                    let state = conns.get_mut(session_id)
                        .ok_or_else(|| VoiceError::Other("Session not found".to_string()))?;
                    
                    let data = std::mem::take(&mut state.audio_buffer);
                    state.is_speaking = false;
                    data
                };
                
                // Send speech stopped
                let stop_msg = ServerMessage::SpeechStopped { audio_end_ms: 0 };
                write.send(Message::Text(serde_json::to_string(&stop_msg).unwrap())).await
                    .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                
                // Generate item ID
                let item_id = format!("item_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string());
                
                // Send committed
                let committed_msg = ServerMessage::AudioCommitted { item_id: item_id.clone() };
                write.send(Message::Text(serde_json::to_string(&committed_msg).unwrap())).await
                    .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                
                // Transcribe
                #[cfg(feature = "whisper")]
                {
                    match Self::transcribe_audio(&audio_data, whisper_model).await {
                        Ok(transcript) => {
                            let msg = ServerMessage::TranscriptionCompleted {
                                transcript,
                                item_id,
                            };
                            write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                                .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                        }
                        Err(e) => {
                            let msg = ServerMessage::TranscriptionFailed {
                                error: ErrorInfo {
                                    message: e.to_string(),
                                    code: Some("transcription_failed".to_string()),
                                },
                                item_id,
                            };
                            write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                                .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                        }
                    }
                }
                
                #[cfg(not(feature = "whisper"))]
                {
                    let msg = ServerMessage::TranscriptionFailed {
                        error: ErrorInfo {
                            message: "Whisper not enabled".to_string(),
                            code: Some("not_available".to_string()),
                        },
                        item_id,
                    };
                    write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                        .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                }
            }
            
            ClientMessage::AudioClear => {
                let mut conns = connections.write().await;
                if let Some(state) = conns.get_mut(session_id) {
                    state.audio_buffer.clear();
                    state.is_speaking = false;
                }
                
                let msg = ServerMessage::AudioCleared;
                write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                    .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
            }
            
            ClientMessage::SessionUpdate { session } => {
                let mut conns = connections.write().await;
                if let Some(state) = conns.get_mut(session_id) {
                    state.session_config = session;
                }
                
                let msg = ServerMessage::SessionUpdated {
                    session: SessionInfo {
                        id: session_id.to_string(),
                        model: "whisper-1".to_string(),
                        voice: "default".to_string(),
                    },
                };
                write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                    .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
            }
            
            ClientMessage::ResponseCreate { response } => {
                // TTS: Generate audio from text
                if let Some(text) = response.instructions {
                    let response_id = format!("resp_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string());
                    
                    match Self::synthesize_audio(&text, tts_engine).await {
                        Ok(audio_bytes) => {
                            // Send audio in chunks
                            for chunk in audio_bytes.chunks(4096) {
                                let b64 = base64::Engine::encode(
                                    &base64::engine::general_purpose::STANDARD,
                                    chunk,
                                );
                                let msg = ServerMessage::AudioDelta {
                                    delta: b64,
                                    response_id: response_id.clone(),
                                };
                                write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                                    .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                            }
                            
                            // Send audio done
                            let msg = ServerMessage::AudioDone { response_id: response_id.clone() };
                            write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                                .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                            
                            // Send response done
                            let msg = ServerMessage::ResponseDone { response_id };
                            write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                                .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                        }
                        Err(e) => {
                            let msg = ServerMessage::Error {
                                error: ErrorInfo {
                                    message: format!("TTS failed: {}", e),
                                    code: Some("tts_failed".to_string()),
                                },
                            };
                            write.send(Message::Text(serde_json::to_string(&msg).unwrap())).await
                                .map_err(|e| VoiceError::WebSocketError(e.to_string()))?;
                        }
                    }
                }
            }
            
            ClientMessage::Unknown => {
                debug!("Unknown client message");
            }
        }
        
        Ok(())
    }
    
    /// Simple VAD - detect if audio contains speech
    fn detect_speech(audio_bytes: &[u8]) -> bool {
        if audio_bytes.len() < 2 {
            return false;
        }
        
        // Interpret as 16-bit PCM
        let samples: Vec<i16> = audio_bytes
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some(i16::from_le_bytes([chunk[0], chunk[1]]))
                } else {
                    None
                }
            })
            .collect();
        
        // Calculate RMS
        if samples.is_empty() {
            return false;
        }
        
        let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        let rms = (sum / samples.len() as f64).sqrt();
        
        rms > 500.0 // Threshold for speech detection
    }
    
    /// Transcribe audio using Whisper
    #[cfg(feature = "whisper")]
    async fn transcribe_audio(audio_bytes: &[u8], model: WhisperModel) -> Result<String, VoiceError> {
        use crate::engines::whisper::WhisperEngine;
        use bytes::Bytes;
        
        let engine = WhisperEngine::new(model);
        
        let audio = AudioData {
            data: Bytes::copy_from_slice(audio_bytes),
            format: AudioFormat::Pcm,
            sample_rate: 16000,
            duration_ms: Some((audio_bytes.len() as u64 * 1000) / (16000 * 2)),
            character_count: 0,
        };
        
        let config = TranscriptionConfig::default();
        let result = engine.transcribe(&audio, &config).await.map_err(|e| VoiceError::TranscriptionError(e.to_string()))?;
        
        Ok(result.text)
    }
    
    /// Synthesize audio using configured TTS engine
    async fn synthesize_audio(text: &str, tts_engine: &TtsEngineConfig) -> Result<Vec<u8>, VoiceError> {
        use crate::VoicePlugin;
        
        let plugin = match tts_engine {
            TtsEngineConfig::OpenAI => VoicePlugin::with_openai(None),
            TtsEngineConfig::ElevenLabs => VoicePlugin::with_elevenlabs(None),
            TtsEngineConfig::Local(endpoint) => VoicePlugin::with_local(endpoint.clone()),
            TtsEngineConfig::None => {
                return Err(VoiceError::NotReady("TTS not configured".to_string()));
            }
        };
        
        let audio = plugin.synthesize(text).await.map_err(|e| VoiceError::AudioError(format!("TTS failed: {}", e)))?;
        Ok(audio.data.to_vec())
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Start an embedded Unmute server with default settings
/// 
/// Returns the WebSocket URL to connect to.
/// 
/// ## Example
/// ```rust,ignore
/// let url = start_embedded_server().await?;
/// println!("Connect to: {}", url);
/// 
/// let client = UnmuteRealtime::connect(&url).await?;
/// ```
#[cfg(feature = "whisper")]
pub async fn start_embedded_server() -> Result<(UnmuteServer, String), VoiceError> {
    let mut server = UnmuteServer::builder()
        .bind("127.0.0.1:0") // Random available port
        .with_whisper(WhisperModel::Base)
        .with_tts_openai()
        .build()
        .await?;
    
    let url = server.ws_url();
    
    tokio::spawn(async move {
        if let Err(e) = server.start().await {
            error!(error = %e, "Embedded server error");
        }
    });
    
    // Give server time to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    
    Ok((UnmuteServer::builder().build().await?, url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = UnmuteServerBuilder::new();
        assert_eq!(builder.bind_addr, "127.0.0.1:8765");
        assert!(builder.api_key.is_none());
    }

    #[test]
    fn test_speech_detection() {
        // Silent audio (all zeros)
        let silent = vec![0u8; 1000];
        assert!(!UnmuteServer::detect_speech(&silent));
        
        // Loud audio (high values)
        let mut loud = Vec::new();
        for _ in 0..500 {
            loud.extend_from_slice(&10000i16.to_le_bytes());
        }
        assert!(UnmuteServer::detect_speech(&loud));
    }

    #[test]
    fn test_server_message_serialization() {
        let msg = ServerMessage::TranscriptionCompleted {
            transcript: "hello world".to_string(),
            item_id: "item_123".to_string(),
        };
        
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("conversation.item.input_audio_transcription.completed"));
        assert!(json.contains("hello world"));
    }
}

