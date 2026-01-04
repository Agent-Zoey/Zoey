// Moshi Speech-Text Engine
//
// Integration with Kyutai's Moshi real-time speech-text foundation model.
// Moshi provides full-duplex spoken dialogue with near real-time latency
// using the Mimi streaming neural audio codec.
//
// Reference: https://github.com/kyutai-labs/moshi
// Paper: https://arxiv.org/abs/2410.00037

use crate::types::*;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info};
use zoey_core::Result;

// ============================================================================
// Moshi Protocol Types (from protocol.md)
// ============================================================================

/// Moshi WebSocket message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MoshiMsgType {
    /// Handshake MT=0: Protocol version (u32) + Model version (u32)
    Handshake = 0,
    /// Audio MT=1: OGG frames with Opus encoded audio (24kHz mono)
    Audio = 1,
    /// Text MT=2: UTF8 encoded string
    Text = 2,
    /// Control MT=3: Control byte (Start=0, EndTurn=1, Pause=2, Restart=3)
    Control = 3,
    /// Metadata MT=4: UTF8 JSON string
    Metadata = 4,
    /// Error MT=5: UTF8 error description
    Error = 5,
    /// Ping MT=6: No payload
    Ping = 6,
}

impl MoshiMsgType {
    /// Parse a message type from a byte value
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Handshake),
            1 => Some(Self::Audio),
            2 => Some(Self::Text),
            3 => Some(Self::Control),
            4 => Some(Self::Metadata),
            5 => Some(Self::Error),
            6 => Some(Self::Ping),
            _ => None,
        }
    }

    /// Convert to byte value
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Control message types for Moshi protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MoshiControl {
    /// Start streaming/recording
    Start = 0,
    /// End current turn (triggers final transcription)
    EndTurn = 1,
    /// Pause streaming
    Pause = 2,
    /// Restart streaming from beginning
    Restart = 3,
}

impl MoshiControl {
    /// Parse a control type from a byte value
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Start),
            1 => Some(Self::EndTurn),
            2 => Some(Self::Pause),
            3 => Some(Self::Restart),
            _ => None,
        }
    }

    /// Convert to byte value
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

// ============================================================================
// Moshi Configuration
// ============================================================================

/// Moshi model options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoshiModel {
    /// Moshi standard model (default)
    Moshi,
    /// Moshika model (different voice/personality)
    Moshika,
    /// Custom model from HuggingFace
    Custom,
}

impl Default for MoshiModel {
    fn default() -> Self {
        Self::Moshi
    }
}

impl MoshiModel {
    /// Get the HuggingFace repository for this model
    pub fn hf_repo(&self) -> &'static str {
        match self {
            Self::Moshi => "kyutai/moshiko-pytorch-bf16",
            Self::Moshika => "kyutai/moshika-pytorch-bf16",
            Self::Custom => "",
        }
    }
}

/// Moshi session configuration (matches Moshi's SessionConfigReq)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoshiSessionConfig {
    /// Temperature for text sampling (0.0-1.0)
    #[serde(default = "default_text_temp")]
    pub text_temperature: f64,
    /// Top-k for text sampling
    #[serde(default = "default_text_topk")]
    pub text_topk: usize,
    /// Temperature for audio sampling
    #[serde(default = "default_audio_temp")]
    pub audio_temperature: f64,
    /// Top-k for audio sampling
    #[serde(default = "default_audio_topk")]
    pub audio_topk: usize,
    /// Maximum generation steps (max ~3 minutes at 12.5fps)
    #[serde(default = "default_max_steps")]
    pub max_steps: usize,
    /// Random seed for audio generation
    pub audio_seed: Option<u64>,
    /// Random seed for text generation
    pub text_seed: Option<u64>,
    /// Padding multiplier for text pad token probability
    pub pad_mult: Option<f32>,
    /// Repetition penalty context size
    pub repetition_penalty_context: Option<usize>,
    /// Repetition penalty value
    pub repetition_penalty: Option<f32>,
}

fn default_text_temp() -> f64 {
    0.8
}
fn default_text_topk() -> usize {
    250
}
fn default_audio_temp() -> f64 {
    0.8
}
fn default_audio_topk() -> usize {
    250
}
fn default_max_steps() -> usize {
    4500
}

impl Default for MoshiSessionConfig {
    fn default() -> Self {
        Self {
            text_temperature: default_text_temp(),
            text_topk: default_text_topk(),
            audio_temperature: default_audio_temp(),
            audio_topk: default_audio_topk(),
            max_steps: default_max_steps(),
            audio_seed: None,
            text_seed: None,
            pad_mult: None,
            repetition_penalty_context: Some(32),
            repetition_penalty: Some(1.0),
        }
    }
}

/// Configuration for the Moshi engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoshiConfig {
    /// WebSocket endpoint for Moshi server
    pub endpoint: String,
    /// Use secure WebSocket (wss://)
    pub use_tls: bool,
    /// Model to use
    pub model: MoshiModel,
    /// Custom HF repo (if model is Custom)
    pub custom_hf_repo: Option<String>,
    /// Session configuration
    pub session: MoshiSessionConfig,
    /// Audio sample rate (Moshi uses 24kHz)
    pub sample_rate: u32,
    /// Frame rate (Moshi uses 12.5 Hz)
    pub frame_rate: f64,
    /// Enable ASR-only mode (no TTS output)
    pub asr_only: bool,
    /// ASR delay in tokens (for streaming transcription)
    pub asr_delay_in_tokens: Option<usize>,
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    /// Enable auto-reconnect on disconnect
    pub auto_reconnect: bool,
}

impl Default for MoshiConfig {
    fn default() -> Self {
        Self {
            endpoint: "localhost:8998".to_string(),
            use_tls: false, // Local Moshi server runs without TLS by default
            model: MoshiModel::default(),
            custom_hf_repo: None,
            session: MoshiSessionConfig::default(),
            sample_rate: 24000, // Moshi standard
            frame_rate: 12.5,   // Moshi standard (80ms per frame)
            asr_only: false,
            asr_delay_in_tokens: Some(12), // ~1 second delay for stable transcription
            connection_timeout_secs: 30,
            auto_reconnect: true,
        }
    }
}

impl MoshiConfig {
    /// Create config for connecting to local Moshi server
    pub fn local() -> Self {
        Self::default()
    }

    /// Create config for connecting to a remote Moshi server
    pub fn remote(host: &str, port: u16) -> Self {
        Self {
            endpoint: format!("{}:{}", host, port),
            ..Default::default()
        }
    }

    /// Get the WebSocket URL
    pub fn ws_url(&self) -> String {
        let scheme = if self.use_tls { "wss" } else { "ws" };
        format!("{}://{}/api/chat", scheme, self.endpoint)
    }

    /// Get the WebSocket URL with session config as query params
    pub fn ws_url_with_session(&self) -> String {
        let base = self.ws_url();
        let params = serde_urlencoded::to_string(&self.session).unwrap_or_default();
        if params.is_empty() {
            base
        } else {
            format!("{}?{}", base, params)
        }
    }
}

// ============================================================================
// Moshi Streaming Types
// ============================================================================

/// Events emitted by the Moshi stream
#[derive(Debug, Clone)]
pub enum MoshiEvent {
    /// Connection established, ready to communicate
    Ready {
        /// Protocol version number
        protocol_version: u32,
        /// Model version number
        model_version: u32,
    },
    /// Transcribed text from speech input
    Transcription {
        /// The transcribed text
        text: String,
        /// Whether this is the final transcription for this utterance
        is_final: bool,
    },
    /// Audio response from Moshi (Opus encoded)
    AudioResponse {
        /// Opus-encoded audio data
        data: Bytes,
    },
    /// Metadata received from server
    Metadata {
        /// JSON-encoded metadata string
        json: String,
    },
    /// Error from server
    Error {
        /// Error message
        message: String,
    },
    /// Connection closed
    Disconnected,
}

/// Commands that can be sent to Moshi
#[derive(Debug, Clone)]
pub enum MoshiCommand {
    /// Send audio data (PCM f32, 24kHz mono)
    SendAudio {
        /// PCM audio samples (f32, 24kHz mono)
        pcm: Vec<f32>,
    },
    /// Send control message
    Control {
        /// The control signal to send
        control: MoshiControl,
    },
    /// Close the connection
    Close,
}

// ============================================================================
// Audio Codec Helpers
// ============================================================================

/// Opus encoder wrapper for Moshi audio format
pub struct MoshiOpusEncoder {
    encoder: opus::Encoder,
    ogg_writer: Vec<u8>,
    pcm_buffer: VecDeque<f32>,
    total_samples: usize,
    frame_size: usize,
}

impl MoshiOpusEncoder {
    /// Create a new Opus encoder for Moshi (24kHz mono)
    pub fn new() -> Result<Self> {
        let encoder = opus::Encoder::new(24000, opus::Channels::Mono, opus::Application::Voip)
            .map_err(|e| VoiceError::AudioError(format!("Failed to create Opus encoder: {}", e)))?;

        // Frame size: 960 samples = 40ms at 24kHz (valid Opus frame size)
        let frame_size = 960;

        Ok(Self {
            encoder,
            ogg_writer: Vec::with_capacity(8192),
            pcm_buffer: VecDeque::with_capacity(frame_size * 2),
            total_samples: 0,
            frame_size,
        })
    }

    /// Write OGG/Opus header
    fn write_opus_header(out: &mut Vec<u8>) -> Result<()> {
        // OpusHead magic signature
        out.extend_from_slice(b"OpusHead");
        // Version (1)
        out.push(1);
        // Channel count (1 = mono)
        out.push(1);
        // Pre-skip (little endian u16)
        out.extend_from_slice(&312u16.to_le_bytes());
        // Input sample rate (little endian u32)
        out.extend_from_slice(&24000u32.to_le_bytes());
        // Output gain (little endian i16)
        out.extend_from_slice(&0i16.to_le_bytes());
        // Channel mapping family (0 = mono/stereo)
        out.push(0);
        Ok(())
    }

    /// Write OGG/Opus tags
    fn write_opus_tags(out: &mut Vec<u8>) -> Result<()> {
        // OpusTags magic signature
        out.extend_from_slice(b"OpusTags");
        // Vendor string length
        let vendor = b"zoey-voice";
        out.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        out.extend_from_slice(vendor);
        // Comment list length (0)
        out.extend_from_slice(&0u32.to_le_bytes());
        Ok(())
    }

    /// Encode PCM audio to Opus frames
    pub fn encode(&mut self, pcm: &[f32]) -> Result<Vec<Bytes>> {
        self.pcm_buffer.extend(pcm);
        let mut frames = Vec::new();
        let mut output_buf = vec![0u8; 4000];

        while self.pcm_buffer.len() >= self.frame_size {
            // Extract one frame
            let frame: Vec<f32> = self.pcm_buffer.drain(..self.frame_size).collect();

            // Encode to Opus
            let encoded_len = self
                .encoder
                .encode_float(&frame, &mut output_buf)
                .map_err(|e| VoiceError::AudioError(format!("Opus encode error: {}", e)))?;

            if encoded_len > 0 {
                self.total_samples += self.frame_size;

                // Create message: [msg_type] + [opus_data]
                let mut msg = Vec::with_capacity(1 + encoded_len);
                msg.push(MoshiMsgType::Audio.to_u8());
                msg.extend_from_slice(&output_buf[..encoded_len]);

                frames.push(Bytes::from(msg));
            }
        }

        Ok(frames)
    }

    /// Flush remaining audio
    pub fn flush(&mut self) -> Result<Option<Bytes>> {
        if self.pcm_buffer.is_empty() {
            return Ok(None);
        }

        // Pad with silence to fill a frame
        let remaining = self.frame_size - self.pcm_buffer.len();
        self.pcm_buffer.extend(std::iter::repeat(0.0).take(remaining));

        let frame: Vec<f32> = self.pcm_buffer.drain(..).collect();
        let mut output_buf = vec![0u8; 4000];

        let encoded_len = self
            .encoder
            .encode_float(&frame, &mut output_buf)
            .map_err(|e| VoiceError::AudioError(format!("Opus encode error: {}", e)))?;

        if encoded_len > 0 {
            let mut msg = Vec::with_capacity(1 + encoded_len);
            msg.push(MoshiMsgType::Audio.to_u8());
            msg.extend_from_slice(&output_buf[..encoded_len]);
            Ok(Some(Bytes::from(msg)))
        } else {
            Ok(None)
        }
    }
}

/// Opus decoder wrapper for receiving Moshi audio
pub struct MoshiOpusDecoder {
    decoder: opus::Decoder,
    pcm_buffer: Vec<f32>,
}

impl MoshiOpusDecoder {
    /// Create a new Opus decoder (24kHz mono)
    pub fn new() -> Result<Self> {
        let decoder = opus::Decoder::new(24000, opus::Channels::Mono)
            .map_err(|e| VoiceError::AudioError(format!("Failed to create Opus decoder: {}", e)))?;

        Ok(Self {
            decoder,
            pcm_buffer: vec![0.0f32; 24000], // 1 second buffer
        })
    }

    /// Decode Opus frames to PCM
    pub fn decode(&mut self, opus_data: &[u8]) -> Result<Vec<f32>> {
        // Skip OpusHead/OpusTags packets
        if opus_data.starts_with(b"OpusHead") || opus_data.starts_with(b"OpusTags") {
            return Ok(Vec::new());
        }

        let decoded_samples = self
            .decoder
            .decode_float(opus_data, &mut self.pcm_buffer, false)
            .map_err(|e| VoiceError::AudioError(format!("Opus decode error: {}", e)))?;

        Ok(self.pcm_buffer[..decoded_samples].to_vec())
    }
}

// ============================================================================
// Moshi Streaming Client
// ============================================================================

/// Connection state for the Moshi WebSocket client
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoshiConnectionState {
    /// Not connected to server
    Disconnected,
    /// Connection in progress
    Connecting,
    /// Connected but not yet ready
    Connected,
    /// Ready to send/receive audio
    Ready,
    /// Connection error occurred
    Error,
}

/// Moshi streaming client for real-time speech-to-text
pub struct MoshiStreamingClient {
    config: MoshiConfig,
    state: Arc<RwLock<MoshiConnectionState>>,
    event_tx: Option<mpsc::UnboundedSender<MoshiEvent>>,
    command_tx: Option<mpsc::UnboundedSender<MoshiCommand>>,
    transcription_buffer: Arc<Mutex<String>>,
}

impl MoshiStreamingClient {
    /// Create a new streaming client
    pub fn new(config: MoshiConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(MoshiConnectionState::Disconnected)),
            event_tx: None,
            command_tx: None,
            transcription_buffer: Arc::new(Mutex::new(String::new())),
        }
    }

    /// Get current connection state
    pub async fn state(&self) -> MoshiConnectionState {
        *self.state.read().await
    }

    /// Check if connected and ready
    pub async fn is_ready(&self) -> bool {
        matches!(self.state().await, MoshiConnectionState::Ready)
    }

    /// Connect to the Moshi server
    pub async fn connect(&mut self) -> Result<mpsc::UnboundedReceiver<MoshiEvent>> {
        *self.state.write().await = MoshiConnectionState::Connecting;

        let url = self.config.ws_url_with_session();
        info!("Connecting to Moshi server at {}", url);

        // Create channels
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        self.event_tx = Some(event_tx.clone());
        self.command_tx = Some(command_tx);

        // Connect to WebSocket
        let ws_url = url::Url::parse(&url)
            .map_err(|e| VoiceError::NetworkError(format!("Invalid URL: {}", e)))?;

        // Handle TLS for wss://
        let (ws_stream, _response) = if self.config.use_tls {
            // For self-signed certs (common in dev), we need to skip verification
            let connector = tokio_tungstenite::Connector::NativeTls(
                native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .build()
                    .map_err(|e| VoiceError::NetworkError(format!("TLS error: {}", e)))?,
            );

            tokio::time::timeout(
                std::time::Duration::from_secs(self.config.connection_timeout_secs),
                tokio_tungstenite::connect_async_tls_with_config(
                    ws_url,
                    None,
                    false,
                    Some(connector),
                ),
            )
            .await
            .map_err(|_| VoiceError::NetworkError("Connection timeout".to_string()))?
            .map_err(|e| VoiceError::WebSocketError(format!("WebSocket connect failed: {}", e)))?
        } else {
            tokio::time::timeout(
                std::time::Duration::from_secs(self.config.connection_timeout_secs),
                connect_async(ws_url),
            )
            .await
            .map_err(|_| VoiceError::NetworkError("Connection timeout".to_string()))?
            .map_err(|e| VoiceError::WebSocketError(format!("WebSocket connect failed: {}", e)))?
        };

        *self.state.write().await = MoshiConnectionState::Connected;
        info!("Connected to Moshi server");

        // Split the stream
        let (write, read) = ws_stream.split();

        // Spawn send task
        let state_clone = self.state.clone();
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            Self::send_loop(write, command_rx, state_clone, event_tx_clone).await;
        });

        // Spawn receive task
        let state_clone = self.state.clone();
        let transcription_buffer = self.transcription_buffer.clone();
        tokio::spawn(async move {
            Self::receive_loop(read, event_tx, state_clone, transcription_buffer).await;
        });

        Ok(event_rx)
    }

    /// Send loop - handles outgoing messages
    async fn send_loop(
        mut write: futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        mut command_rx: mpsc::UnboundedReceiver<MoshiCommand>,
        state: Arc<RwLock<MoshiConnectionState>>,
        event_tx: mpsc::UnboundedSender<MoshiEvent>,
    ) {
        let mut encoder = match MoshiOpusEncoder::new() {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to create Opus encoder: {}", e);
                let _ = event_tx.send(MoshiEvent::Error {
                    message: format!("Encoder error: {}", e),
                });
                return;
            }
        };

        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                MoshiCommand::SendAudio { pcm } => {
                    // Encode PCM to Opus and send
                    match encoder.encode(&pcm) {
                        Ok(frames) => {
                            for frame in frames {
                                if let Err(e) = write.send(Message::Binary(frame.to_vec().into())).await {
                                    error!("Failed to send audio: {}", e);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Audio encoding error: {}", e);
                        }
                    }
                }
                MoshiCommand::Control { control } => {
                    let msg = vec![MoshiMsgType::Control.to_u8(), control.to_u8()];
                    if let Err(e) = write.send(Message::Binary(msg.into())).await {
                        error!("Failed to send control: {}", e);
                    }
                }
                MoshiCommand::Close => {
                    let _ = write.close().await;
                    *state.write().await = MoshiConnectionState::Disconnected;
                    let _ = event_tx.send(MoshiEvent::Disconnected);
                    break;
                }
            }
        }
    }

    /// Receive loop - handles incoming messages
    async fn receive_loop(
        mut read: futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
        event_tx: mpsc::UnboundedSender<MoshiEvent>,
        state: Arc<RwLock<MoshiConnectionState>>,
        transcription_buffer: Arc<Mutex<String>>,
    ) {
        let mut decoder = match MoshiOpusDecoder::new() {
            Ok(d) => d,
            Err(e) => {
                error!("Failed to create Opus decoder: {}", e);
                let _ = event_tx.send(MoshiEvent::Error {
                    message: format!("Decoder error: {}", e),
                });
                return;
            }
        };

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
                        MoshiMsgType::Handshake => {
                            // Parse handshake: protocol_version (u32) + model_version (u32)
                            if payload.len() >= 8 {
                                let protocol_version =
                                    u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
                                let model_version =
                                    u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);

                                *state.write().await = MoshiConnectionState::Ready;
                                info!(
                                    "Moshi ready: protocol v{}, model v{}",
                                    protocol_version, model_version
                                );
                                let _ = event_tx.send(MoshiEvent::Ready {
                                    protocol_version,
                                    model_version,
                                });
                            }
                        }
                        MoshiMsgType::Audio => {
                            // Audio response from Moshi
                            let _ = event_tx.send(MoshiEvent::AudioResponse {
                                data: Bytes::copy_from_slice(payload),
                            });
                        }
                        MoshiMsgType::Text => {
                            // Transcribed text
                            if let Ok(text) = std::str::from_utf8(payload) {
                                // Append to buffer
                                let mut buffer = transcription_buffer.lock().await;
                                buffer.push_str(text);

                                let _ = event_tx.send(MoshiEvent::Transcription {
                                    text: text.to_string(),
                                    is_final: false, // Moshi streams incrementally
                                });
                            }
                        }
                        MoshiMsgType::Metadata => {
                            if let Ok(json) = std::str::from_utf8(payload) {
                                debug!("Moshi metadata: {}", json);
                                let _ = event_tx.send(MoshiEvent::Metadata {
                                    json: json.to_string(),
                                });
                            }
                        }
                        MoshiMsgType::Error => {
                            if let Ok(msg) = std::str::from_utf8(payload) {
                                error!("Moshi error: {}", msg);
                                let _ = event_tx.send(MoshiEvent::Error {
                                    message: msg.to_string(),
                                });
                            }
                        }
                        MoshiMsgType::Control | MoshiMsgType::Ping => {
                            // These are typically not sent from server
                            debug!("Received {:?} from server", msg_type);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Moshi connection closed");
                    *state.write().await = MoshiConnectionState::Disconnected;
                    let _ = event_tx.send(MoshiEvent::Disconnected);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    *state.write().await = MoshiConnectionState::Error;
                    let _ = event_tx.send(MoshiEvent::Error {
                        message: format!("WebSocket error: {}", e),
                    });
                    break;
                }
                _ => {}
            }
        }
    }

    /// Send audio data (PCM f32, 24kHz mono)
    pub fn send_audio(&self, pcm: Vec<f32>) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.send(MoshiCommand::SendAudio { pcm })
                .map_err(|_| VoiceError::NotReady("Connection closed".to_string()))?;
        }
        Ok(())
    }

    /// Send control message
    pub fn send_control(&self, control: MoshiControl) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.send(MoshiCommand::Control { control })
                .map_err(|_| VoiceError::NotReady("Connection closed".to_string()))?;
        }
        Ok(())
    }

    /// Close the connection
    pub fn close(&self) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(MoshiCommand::Close);
        }
        Ok(())
    }

    /// Get accumulated transcription
    pub async fn get_transcription(&self) -> String {
        self.transcription_buffer.lock().await.clone()
    }

    /// Clear transcription buffer
    pub async fn clear_transcription(&self) {
        self.transcription_buffer.lock().await.clear();
    }
}

// ============================================================================
// Moshi Speech Engine (STT)
// ============================================================================

/// Moshi-based speech-to-text engine
///
/// Uses Kyutai's Moshi model for near real-time speech recognition.
/// Requires a running Moshi server (local or remote).
///
/// # Example
/// ```rust,ignore
/// let config = MoshiConfig::local();
/// let engine = MoshiEngine::new(config);
///
/// // For batch transcription
/// let result = engine.transcribe(&audio, &TranscriptionConfig::default()).await?;
///
/// // For streaming
/// let mut stream = engine.create_stream().await?;
/// stream.send_audio(audio_chunk).await?;
/// while let Some(text) = stream.next_transcription().await {
///     println!("Transcribed: {}", text);
/// }
/// ```
pub struct MoshiEngine {
    config: MoshiConfig,
    client: Option<Arc<Mutex<MoshiStreamingClient>>>,
    is_initialized: Arc<RwLock<bool>>,
}

impl MoshiEngine {
    /// Create a new Moshi engine with configuration
    pub fn new(config: MoshiConfig) -> Self {
        Self {
            config,
            client: None,
            is_initialized: Arc::new(RwLock::new(false)),
        }
    }

    /// Create with default local configuration
    pub fn local() -> Self {
        Self::new(MoshiConfig::local())
    }

    /// Create for remote Moshi server
    pub fn remote(host: &str, port: u16) -> Self {
        Self::new(MoshiConfig::remote(host, port))
    }

    /// Get or create a streaming client
    async fn ensure_client(&self) -> Result<Arc<Mutex<MoshiStreamingClient>>> {
        if let Some(ref client) = self.client {
            let guard = client.lock().await;
            if guard.is_ready().await {
                drop(guard);
                return Ok(client.clone());
            }
        }

        // Create new client and connect
        let mut client = MoshiStreamingClient::new(self.config.clone());
        let _events = client.connect().await?;

        // Wait for ready state
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            async {
                while !client.is_ready().await {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            },
        )
        .await
        .map_err(|_| VoiceError::NotReady("Moshi server not ready".to_string()))?;

        Ok(Arc::new(Mutex::new(client)))
    }

    /// Create a new streaming session
    pub async fn create_stream(&self) -> Result<MoshiStream> {
        let mut client = MoshiStreamingClient::new(self.config.clone());
        let events = client.connect().await?;

        Ok(MoshiStream {
            client,
            events,
            transcription: String::new(),
        })
    }

    /// Resample audio to Moshi's required 24kHz
    fn resample_to_24k(audio: &AudioData) -> Result<Vec<f32>> {
        // Convert bytes to f32 samples
        let samples: Vec<f32> = match audio.format {
            AudioFormat::Pcm => {
                // Assume 16-bit PCM
                audio
                    .data
                    .chunks(2)
                    .map(|chunk| {
                        let sample = i16::from_le_bytes([chunk[0], chunk.get(1).copied().unwrap_or(0)]);
                        sample as f32 / 32768.0
                    })
                    .collect()
            }
            AudioFormat::Wav => {
                // Parse WAV header and extract samples
                // For now, assume raw PCM after header
                if audio.data.len() < 44 {
                    return Err(VoiceError::AudioError("Invalid WAV data".to_string()).into());
                }
                audio.data[44..]
                    .chunks(2)
                    .map(|chunk| {
                        let sample = i16::from_le_bytes([chunk[0], chunk.get(1).copied().unwrap_or(0)]);
                        sample as f32 / 32768.0
                    })
                    .collect()
            }
            _ => {
                return Err(VoiceError::UnsupportedFormat(
                    format!("Format {:?} not supported for Moshi", audio.format),
                )
                .into());
            }
        };

        // Resample if needed
        if audio.sample_rate == 24000 {
            return Ok(samples);
        }

        // Simple linear resampling
        let ratio = 24000.0 / audio.sample_rate as f64;
        let output_len = (samples.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_idx = i as f64 / ratio;
            let idx = src_idx.floor() as usize;
            let frac = src_idx.fract() as f32;

            if idx + 1 < samples.len() {
                let sample = samples[idx] * (1.0 - frac) + samples[idx + 1] * frac;
                resampled.push(sample);
            } else if idx < samples.len() {
                resampled.push(samples[idx]);
            }
        }

        Ok(resampled)
    }
}

#[async_trait]
impl SpeechEngine for MoshiEngine {
    fn name(&self) -> &str {
        "moshi"
    }

    async fn transcribe(
        &self,
        audio: &AudioData,
        config: &TranscriptionConfig,
    ) -> Result<TranscriptionResult> {
        let start_time = std::time::Instant::now();

        // Resample to 24kHz
        let samples = Self::resample_to_24k(audio)?;

        // Create streaming client for this transcription
        let mut client = MoshiStreamingClient::new(self.config.clone());
        let mut events = client.connect().await?;

        // Wait for ready
        loop {
            match events.recv().await {
                Some(MoshiEvent::Ready { .. }) => break,
                Some(MoshiEvent::Error { message }) => {
                    return Err(VoiceError::TranscriptionError(message).into());
                }
                Some(MoshiEvent::Disconnected) => {
                    return Err(VoiceError::NotReady("Disconnected".to_string()).into());
                }
                None => {
                    return Err(VoiceError::NotReady("Connection lost".to_string()).into());
                }
                _ => continue,
            }
        }

        // Send audio in chunks
        let chunk_size = (self.config.sample_rate as f64 / self.config.frame_rate) as usize;
        for chunk in samples.chunks(chunk_size) {
            client.send_audio(chunk.to_vec())?;
        }

        // Send end turn control
        client.send_control(MoshiControl::EndTurn)?;

        // Collect transcription
        let mut full_text = String::new();
        let timeout = std::time::Duration::from_secs(30);
        let deadline = std::time::Instant::now() + timeout;

        while std::time::Instant::now() < deadline {
            match tokio::time::timeout(
                deadline.saturating_duration_since(std::time::Instant::now()),
                events.recv(),
            )
            .await
            {
                Ok(Some(MoshiEvent::Transcription { text, .. })) => {
                    full_text.push_str(&text);
                }
                Ok(Some(MoshiEvent::Disconnected)) | Ok(None) => break,
                Ok(Some(MoshiEvent::Error { message })) => {
                    return Err(VoiceError::TranscriptionError(message).into());
                }
                _ => continue,
            }
        }

        client.close()?;

        let processing_time_ms = start_time.elapsed().as_millis() as u64;
        let duration_ms = audio.duration_ms;

        Ok(TranscriptionResult {
            text: full_text.trim().to_string(),
            language: config.language.clone(),
            language_confidence: None,
            confidence: None,
            duration_ms,
            segments: Vec::new(),
            processing_time_ms: Some(processing_time_ms),
        })
    }

    async fn transcribe_stream(
        &self,
        audio: &AudioData,
        config: &TranscriptionConfig,
    ) -> Result<TranscriptionStream> {
        let (tx, rx) = mpsc::channel(32);

        // Resample audio
        let samples = Self::resample_to_24k(audio)?;
        let moshi_config = self.config.clone();

        tokio::spawn(async move {
            let mut client = MoshiStreamingClient::new(moshi_config);
            let mut events = match client.connect().await {
                Ok(e) => e,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            };

            // Wait for ready
            loop {
                match events.recv().await {
                    Some(MoshiEvent::Ready { .. }) => break,
                    Some(MoshiEvent::Error { message }) => {
                        let _ = tx
                            .send(Err(VoiceError::TranscriptionError(message).into()))
                            .await;
                        return;
                    }
                    _ => continue,
                }
            }

            // Send audio
            let chunk_size = 1920; // 80ms at 24kHz
            for chunk in samples.chunks(chunk_size) {
                if client.send_audio(chunk.to_vec()).is_err() {
                    break;
                }
            }

            // Collect transcriptions
            while let Some(event) = events.recv().await {
                match event {
                    MoshiEvent::Transcription { text, is_final } => {
                        let chunk = TranscriptionChunk {
                            text,
                            is_final,
                            timestamp_ms: None,
                            confidence: None,
                        };
                        if tx.send(Ok(chunk)).await.is_err() {
                            break;
                        }
                    }
                    MoshiEvent::Disconnected => break,
                    _ => continue,
                }
            }
        });

        Ok(rx)
    }

    async fn is_ready(&self) -> bool {
        // Try to connect and verify server is available
        let mut client = MoshiStreamingClient::new(self.config.clone());
        if let Ok(mut events) = client.connect().await {
            let timeout = std::time::Duration::from_secs(5);
            let result = tokio::time::timeout(timeout, async {
                while let Some(event) = events.recv().await {
                    match event {
                        MoshiEvent::Ready { .. } => return true,
                        MoshiEvent::Error { .. } | MoshiEvent::Disconnected => return false,
                        _ => continue,
                    }
                }
                false
            })
            .await
            .unwrap_or(false);
            let _ = client.close();
            result
        } else {
            false
        }
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Pcm, AudioFormat::Wav]
    }

    fn max_duration_secs(&self) -> u32 {
        // ~3 minutes based on max_steps
        180
    }

    fn supported_languages(&self) -> Vec<&'static str> {
        // Moshi is primarily trained on English
        vec!["en"]
    }
}

// ============================================================================
// Moshi Stream (for continuous streaming)
// ============================================================================

/// Streaming session for continuous speech-to-text
pub struct MoshiStream {
    client: MoshiStreamingClient,
    events: mpsc::UnboundedReceiver<MoshiEvent>,
    transcription: String,
}

impl MoshiStream {
    /// Send audio chunk (PCM f32, should be 24kHz mono)
    pub fn send_audio(&self, pcm: Vec<f32>) -> Result<()> {
        self.client.send_audio(pcm)
    }

    /// Send raw PCM bytes (16-bit, will be converted to f32)
    pub fn send_pcm_bytes(&self, data: &[u8], sample_rate: u32) -> Result<()> {
        // Convert to f32
        let samples: Vec<f32> = data
            .chunks(2)
            .map(|chunk| {
                let sample = i16::from_le_bytes([chunk[0], chunk.get(1).copied().unwrap_or(0)]);
                sample as f32 / 32768.0
            })
            .collect();

        // Resample if needed
        if sample_rate != 24000 {
            let ratio = 24000.0 / sample_rate as f64;
            let output_len = (samples.len() as f64 * ratio) as usize;
            let mut resampled = Vec::with_capacity(output_len);

            for i in 0..output_len {
                let src_idx = i as f64 / ratio;
                let idx = src_idx.floor() as usize;
                let frac = src_idx.fract() as f32;

                if idx + 1 < samples.len() {
                    let sample = samples[idx] * (1.0 - frac) + samples[idx + 1] * frac;
                    resampled.push(sample);
                } else if idx < samples.len() {
                    resampled.push(samples[idx]);
                }
            }

            self.client.send_audio(resampled)
        } else {
            self.client.send_audio(samples)
        }
    }

    /// Get next transcription event
    pub async fn next_transcription(&mut self) -> Option<String> {
        while let Some(event) = self.events.recv().await {
            match event {
                MoshiEvent::Transcription { text, .. } => {
                    self.transcription.push_str(&text);
                    return Some(text);
                }
                MoshiEvent::Disconnected => return None,
                _ => continue,
            }
        }
        None
    }

    /// Get full accumulated transcription
    pub fn full_transcription(&self) -> &str {
        &self.transcription
    }

    /// Signal end of speech
    pub fn end_turn(&self) -> Result<()> {
        self.client.send_control(MoshiControl::EndTurn)
    }

    /// Close the stream
    pub fn close(&self) -> Result<()> {
        self.client.close()
    }

    /// Check if stream is still active
    pub async fn is_active(&self) -> bool {
        self.client.is_ready().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moshi_config_default() {
        let config = MoshiConfig::default();
        assert_eq!(config.sample_rate, 24000);
        assert_eq!(config.frame_rate, 12.5);
        assert!(config.use_tls);
    }

    #[test]
    fn test_moshi_config_ws_url() {
        let config = MoshiConfig::local();
        assert!(config.ws_url().starts_with("ws://")); // Local uses ws:// by default

        let mut config = MoshiConfig::local();
        config.use_tls = true;
        assert!(config.ws_url().starts_with("wss://"));
    }

    #[test]
    fn test_msg_type_roundtrip() {
        for i in 0..=6 {
            if let Some(t) = MoshiMsgType::from_u8(i) {
                assert_eq!(t.to_u8(), i);
            }
        }
    }

    #[test]
    fn test_session_config_default() {
        let config = MoshiSessionConfig::default();
        assert_eq!(config.text_temperature, 0.8);
        assert_eq!(config.max_steps, 4500);
    }
}

