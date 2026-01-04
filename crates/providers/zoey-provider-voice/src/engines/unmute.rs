//! Unmute STT/TTS Engine (Premium)
//!
//! GPU-accelerated speech-to-text and text-to-speech using Unmute.
//! Connects to a local or remote Unmute instance via WebSocket.
//!
//! Unmute uses a protocol based on OpenAI's Realtime API.
//! See: https://github.com/kyutai-labs/unmute

#[cfg(feature = "unmute")]
use async_trait::async_trait;
#[cfg(feature = "unmute")]
use bytes::Bytes;
#[cfg(feature = "unmute")]
use futures_util::{SinkExt, StreamExt};
#[cfg(feature = "unmute")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "unmute")]
use std::sync::Arc;
#[cfg(feature = "unmute")]
use std::time::Duration;
#[cfg(feature = "unmute")]
use tokio::sync::{mpsc, RwLock};
#[cfg(feature = "unmute")]
use tokio::time::timeout;
#[cfg(feature = "unmute")]
use tokio_tungstenite::{connect_async, tungstenite::Message};
#[cfg(feature = "unmute")]
use tracing::{debug, error, info, warn};
#[cfg(feature = "unmute")]
use zoey_core::Result;

#[cfg(feature = "unmute")]
use crate::types::*;

/// Default Unmute WebSocket endpoint
#[cfg(feature = "unmute")]
const DEFAULT_ENDPOINT: &str = "ws://localhost:8000";

/// Connection timeout
#[cfg(feature = "unmute")]
const CONNECT_TIMEOUT_SECS: u64 = 10;

/// Response timeout for STT/TTS operations
#[cfg(feature = "unmute")]
const OPERATION_TIMEOUT_SECS: u64 = 60;

// ============================================================================
// Unmute Protocol Messages (based on OpenAI Realtime API)
// ============================================================================

/// Client message types for Unmute
#[cfg(feature = "unmute")]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum UnmuteClientMessage {
    /// Send audio for transcription
    #[serde(rename = "input_audio_buffer.append")]
    InputAudioAppend {
        /// Base64-encoded audio data
        audio: String,
    },
    
    /// Commit audio buffer for processing
    #[serde(rename = "input_audio_buffer.commit")]
    InputAudioCommit,
    
    /// Clear audio buffer
    #[serde(rename = "input_audio_buffer.clear")]
    InputAudioClear,
    
    /// Create a response (triggers TTS)
    #[serde(rename = "response.create")]
    ResponseCreate {
        #[serde(default)]
        response: Option<ResponseConfig>,
    },
    
    /// Cancel current response
    #[serde(rename = "response.cancel")]
    ResponseCancel,
    
    /// Update session configuration
    #[serde(rename = "session.update")]
    SessionUpdate {
        #[serde(default)]
        session: Option<SessionConfig>,
    },
}

/// Response configuration
#[cfg(feature = "unmute")]
#[derive(Debug, Clone, Serialize, Default)]
pub struct ResponseConfig {
    /// Instructions/system prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Voice ID for TTS
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    /// Output modalities (text, audio)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
}

/// Session configuration
#[cfg(feature = "unmute")]
#[derive(Debug, Clone, Serialize, Default)]
pub struct SessionConfig {
    /// Input audio format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_audio_format: Option<String>,
    /// Output audio format  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_audio_format: Option<String>,
    /// Voice ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    /// Instructions/system prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Input audio transcription config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_audio_transcription: Option<TranscriptionSettings>,
}

/// Transcription settings for Unmute
#[cfg(feature = "unmute")]
#[derive(Debug, Clone, Serialize, Default)]
pub struct TranscriptionSettings {
    /// Model to use (e.g., "whisper-1")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Language hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Server message types from Unmute
#[cfg(feature = "unmute")]
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum UnmuteServerMessage {
    /// Session created
    #[serde(rename = "session.created")]
    SessionCreated {
        session: serde_json::Value,
    },
    
    /// Session updated
    #[serde(rename = "session.updated")]
    SessionUpdated {
        session: serde_json::Value,
    },
    
    /// Transcription completed
    #[serde(rename = "conversation.item.input_audio_transcription.completed")]
    TranscriptionCompleted {
        transcript: String,
        #[serde(default)]
        item_id: Option<String>,
    },
    
    /// Transcription failed
    #[serde(rename = "conversation.item.input_audio_transcription.failed")]
    TranscriptionFailed {
        error: serde_json::Value,
    },
    
    /// Response audio delta (streaming TTS)
    #[serde(rename = "response.audio.delta")]
    AudioDelta {
        /// Base64-encoded audio chunk
        delta: String,
        #[serde(default)]
        response_id: Option<String>,
    },
    
    /// Response audio done
    #[serde(rename = "response.audio.done")]
    AudioDone {
        #[serde(default)]
        response_id: Option<String>,
    },
    
    /// Response text delta (streaming text)
    #[serde(rename = "response.text.delta")]
    TextDelta {
        delta: String,
        #[serde(default)]
        response_id: Option<String>,
    },
    
    /// Response text done
    #[serde(rename = "response.text.done")]
    TextDone {
        text: String,
        #[serde(default)]
        response_id: Option<String>,
    },
    
    /// Response completed
    #[serde(rename = "response.done")]
    ResponseDone {
        response: serde_json::Value,
    },
    
    /// Error
    #[serde(rename = "error")]
    Error {
        error: ErrorInfo,
    },
    
    /// Input audio buffer committed
    #[serde(rename = "input_audio_buffer.committed")]
    InputAudioCommitted {
        #[serde(default)]
        item_id: Option<String>,
    },
    
    /// Input audio buffer cleared
    #[serde(rename = "input_audio_buffer.cleared")]
    InputAudioCleared,
    
    /// Speech started (VAD detected speech)
    #[serde(rename = "input_audio_buffer.speech_started")]
    SpeechStarted {
        #[serde(default)]
        audio_start_ms: Option<u64>,
    },
    
    /// Speech stopped (VAD detected silence)
    #[serde(rename = "input_audio_buffer.speech_stopped")]
    SpeechStopped {
        #[serde(default)]
        audio_end_ms: Option<u64>,
    },
    
    /// Catch-all for unknown messages
    #[serde(other)]
    Unknown,
}

/// Error information from Unmute
#[cfg(feature = "unmute")]
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorInfo {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub code: Option<String>,
}

// ============================================================================
// Unmute Engine
// ============================================================================

/// Unmute speech engine for GPU-accelerated STT and TTS
#[cfg(feature = "unmute")]
pub struct UnmuteEngine {
    /// WebSocket endpoint URL
    endpoint: String,
    /// Voice ID for TTS
    voice_id: Option<String>,
    /// Connection state
    connected: Arc<RwLock<bool>>,
}

#[cfg(feature = "unmute")]
impl UnmuteEngine {
    /// Create a new Unmute engine with default endpoint
    pub fn new() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            voice_id: None,
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Create with custom endpoint
    pub fn with_endpoint(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            voice_id: None,
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Set voice ID for TTS
    pub fn with_voice(mut self, voice_id: &str) -> Self {
        self.voice_id = Some(voice_id.to_string());
        self
    }

    /// Get the endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Check if Unmute service is available
    pub async fn health_check(&self) -> bool {
        // Try to establish a brief WebSocket connection
        let ws_url = format!("{}/ws", self.endpoint.trim_end_matches('/'));
        
        match timeout(
            Duration::from_secs(5),
            connect_async(&ws_url),
        )
        .await
        {
            Ok(Ok((ws, _))) => {
                debug!("Unmute health check passed");
                // Close connection gracefully
                drop(ws);
                true
            }
            Ok(Err(e)) => {
                debug!("Unmute health check failed: {}", e);
                false
            }
            Err(_) => {
                debug!("Unmute health check timed out");
                false
            }
        }
    }

    /// Connect and send audio for transcription
    async fn send_audio_for_transcription(
        &self,
        audio: &AudioData,
        config: &TranscriptionConfig,
    ) -> Result<String> {
        let ws_url = format!("{}/ws", self.endpoint.trim_end_matches('/'));
        
        // Connect to Unmute
        let (ws_stream, _) = timeout(
            Duration::from_secs(CONNECT_TIMEOUT_SECS),
            connect_async(&ws_url),
        )
        .await
        .map_err(|_| VoiceError::WebSocketError("Connection timeout".to_string()))?
        .map_err(|e| VoiceError::WebSocketError(format!("Failed to connect: {}", e)))?;

        let (mut write, mut read) = ws_stream.split();

        // Configure session for transcription
        let session_config = SessionConfig {
            input_audio_format: Some(audio.format.as_str().to_string()),
            input_audio_transcription: Some(TranscriptionSettings {
                model: Some("whisper-1".to_string()),
                language: config.language.clone(),
            }),
            ..Default::default()
        };

        let session_msg = UnmuteClientMessage::SessionUpdate {
            session: Some(session_config),
        };
        
        write
            .send(Message::Text(serde_json::to_string(&session_msg).unwrap()))
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("Failed to send session config: {}", e)))?;

        // Send audio data
        let audio_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &audio.data,
        );
        
        let audio_msg = UnmuteClientMessage::InputAudioAppend { audio: audio_b64 };
        write
            .send(Message::Text(serde_json::to_string(&audio_msg).unwrap()))
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("Failed to send audio: {}", e)))?;

        // Commit audio buffer
        let commit_msg = UnmuteClientMessage::InputAudioCommit;
        write
            .send(Message::Text(serde_json::to_string(&commit_msg).unwrap()))
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("Failed to commit audio: {}", e)))?;

        // Wait for transcription result
        let mut transcript = String::new();
        
        let result = timeout(Duration::from_secs(OPERATION_TIMEOUT_SECS), async {
            while let Some(msg_result) = read.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<UnmuteServerMessage>(&text) {
                            Ok(UnmuteServerMessage::TranscriptionCompleted { transcript: t, .. }) => {
                                transcript = t;
                                break;
                            }
                            Ok(UnmuteServerMessage::TranscriptionFailed { error }) => {
                                return Err(VoiceError::TranscriptionError(
                                    format!("Unmute transcription failed: {:?}", error)
                                ));
                            }
                            Ok(UnmuteServerMessage::Error { error }) => {
                                return Err(VoiceError::TranscriptionError(
                                    format!("Unmute error: {}", error.message)
                                ));
                            }
                            Ok(_) => {
                                // Ignore other messages
                                continue;
                            }
                            Err(e) => {
                                debug!("Failed to parse Unmute message: {}", e);
                                continue;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        return Err(VoiceError::WebSocketError(format!("WebSocket error: {}", e)));
                    }
                    _ => continue,
                }
            }
            Ok(transcript)
        })
        .await
        .map_err(|_| VoiceError::TranscriptionError("Transcription timeout".to_string()))?;

        result.map_err(|e| e.into())
    }

    /// Connect and synthesize text to speech
    async fn synthesize_via_ws(
        &self,
        text: &str,
        voice_config: &VoiceConfig,
    ) -> Result<Vec<u8>> {
        let ws_url = format!("{}/ws", self.endpoint.trim_end_matches('/'));
        
        // Connect to Unmute
        let (ws_stream, _) = timeout(
            Duration::from_secs(CONNECT_TIMEOUT_SECS),
            connect_async(&ws_url),
        )
        .await
        .map_err(|_| VoiceError::WebSocketError("Connection timeout".to_string()))?
        .map_err(|e| VoiceError::WebSocketError(format!("Failed to connect: {}", e)))?;

        let (mut write, mut read) = ws_stream.split();

        // Configure session for TTS
        let voice = self.voice_id.clone()
            .or_else(|| Some(voice_config.voice.id.clone()))
            .unwrap_or_else(|| "default".to_string());

        let session_config = SessionConfig {
            output_audio_format: Some(voice_config.output_format.as_str().to_string()),
            voice: Some(voice.clone()),
            ..Default::default()
        };

        let session_msg = UnmuteClientMessage::SessionUpdate {
            session: Some(session_config),
        };
        
        write
            .send(Message::Text(serde_json::to_string(&session_msg).unwrap()))
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("Failed to send session config: {}", e)))?;

        // Create response (this triggers TTS)
        let response_msg = UnmuteClientMessage::ResponseCreate {
            response: Some(ResponseConfig {
                instructions: Some(text.to_string()),
                voice: Some(voice),
                modalities: Some(vec!["audio".to_string()]),
            }),
        };
        
        write
            .send(Message::Text(serde_json::to_string(&response_msg).unwrap()))
            .await
            .map_err(|e| VoiceError::WebSocketError(format!("Failed to request TTS: {}", e)))?;

        // Collect audio chunks
        let mut audio_data = Vec::new();
        
        let result = timeout(Duration::from_secs(OPERATION_TIMEOUT_SECS), async {
            while let Some(msg_result) = read.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<UnmuteServerMessage>(&text) {
                            Ok(UnmuteServerMessage::AudioDelta { delta, .. }) => {
                                if let Ok(chunk) = base64::Engine::decode(
                                    &base64::engine::general_purpose::STANDARD,
                                    &delta,
                                ) {
                                    audio_data.extend(chunk);
                                }
                            }
                            Ok(UnmuteServerMessage::AudioDone { .. }) => {
                                break;
                            }
                            Ok(UnmuteServerMessage::ResponseDone { .. }) => {
                                break;
                            }
                            Ok(UnmuteServerMessage::Error { error }) => {
                                return Err(VoiceError::AudioError(
                                    format!("Unmute TTS error: {}", error.message)
                                ));
                            }
                            Ok(_) => continue,
                            Err(e) => {
                                debug!("Failed to parse Unmute message: {}", e);
                                continue;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        return Err(VoiceError::WebSocketError(format!("WebSocket error: {}", e)));
                    }
                    _ => continue,
                }
            }
            Ok(audio_data)
        })
        .await
        .map_err(|_| VoiceError::AudioError("TTS timeout".to_string()))?;

        result.map_err(|e| e.into())
    }
}

#[cfg(feature = "unmute")]
impl Default for UnmuteEngine {
    fn default() -> Self {
        Self::new()
    }
}

// Implement SpeechEngine for STT
#[cfg(feature = "unmute")]
#[async_trait]
impl SpeechEngine for UnmuteEngine {
    fn name(&self) -> &str {
        "unmute"
    }

    async fn transcribe(
        &self,
        audio: &AudioData,
        config: &TranscriptionConfig,
    ) -> Result<TranscriptionResult> {
        let text = self.send_audio_for_transcription(audio, config).await?;
        
        Ok(TranscriptionResult {
            text,
            language: config.language.clone(),
            language_confidence: None,
            confidence: None,
            duration_ms: audio.duration_ms,
            segments: Vec::new(),
            processing_time_ms: None,
        })
    }

    async fn transcribe_stream(
        &self,
        audio: &AudioData,
        config: &TranscriptionConfig,
    ) -> Result<TranscriptionStream> {
        // For now, use non-streaming and emit single result
        // Real streaming would require maintaining persistent WebSocket
        let (tx, rx) = create_transcription_stream(16);
        
        let result = self.transcribe(audio, config).await;
        
        tokio::spawn(async move {
            match result {
                Ok(transcription) => {
                    let _ = tx.send(Ok(TranscriptionChunk {
                        text: transcription.text,
                        is_final: true,
                        timestamp_ms: None,
                        confidence: None,
                    })).await;
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        });
        
        Ok(rx)
    }

    async fn is_ready(&self) -> bool {
        self.health_check().await
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![
            AudioFormat::Pcm,
            AudioFormat::Wav,
            AudioFormat::Mp3,
            AudioFormat::Opus,
        ]
    }

    fn max_duration_secs(&self) -> u32 {
        600 // 10 minutes
    }

    fn supported_languages(&self) -> Vec<&'static str> {
        // Unmute supports same languages as Whisper
        vec!["en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr"]
    }
}

// Implement VoiceEngine for TTS
#[cfg(feature = "unmute")]
#[async_trait]
impl VoiceEngine for UnmuteEngine {
    fn name(&self) -> &str {
        "unmute"
    }

    async fn synthesize(&self, text: &str, config: &VoiceConfig) -> Result<AudioData> {
        let audio_bytes = self.synthesize_via_ws(text, config).await?;
        
        Ok(AudioData {
            data: Bytes::from(audio_bytes),
            format: config.output_format,
            sample_rate: config.sample_rate,
            duration_ms: None,
            character_count: text.len(),
        })
    }

    async fn synthesize_stream(&self, text: &str, config: &VoiceConfig) -> Result<AudioStream> {
        // Real-time streaming: connect to Unmute and stream audio chunks as they arrive
        let (tx, rx) = create_audio_stream(32);
        
        let endpoint = self.endpoint.clone();
        let voice_id = self.voice_id.clone();
        let voice_config = config.clone();
        let text = text.to_string();
        
        tokio::spawn(async move {
            let ws_url = format!("{}/ws", endpoint.trim_end_matches('/'));
            
            // Connect to Unmute
            let (ws_stream, _) = match timeout(
                Duration::from_secs(CONNECT_TIMEOUT_SECS),
                connect_async(&ws_url),
            ).await {
                Ok(Ok(stream)) => stream,
                Ok(Err(e)) => {
                    let _ = tx.send(Err(VoiceError::WebSocketError(format!("Failed to connect: {}", e)).into())).await;
                    return;
                }
                Err(_) => {
                    let _ = tx.send(Err(VoiceError::WebSocketError("Connection timeout".to_string()).into())).await;
                    return;
                }
            };

            let (mut write, mut read) = ws_stream.split();

            // Configure session for TTS
            let voice = voice_id
                .or_else(|| Some(voice_config.voice.id.clone()))
                .unwrap_or_else(|| "default".to_string());

            let session_config = SessionConfig {
                output_audio_format: Some(voice_config.output_format.as_str().to_string()),
                voice: Some(voice.clone()),
                ..Default::default()
            };

            let session_msg = UnmuteClientMessage::SessionUpdate {
                session: Some(session_config),
            };
            
            if let Err(e) = write.send(Message::Text(serde_json::to_string(&session_msg).unwrap())).await {
                let _ = tx.send(Err(VoiceError::WebSocketError(format!("Failed to send session config: {}", e)).into())).await;
                return;
            }

            // Create response (this triggers TTS)
            let response_msg = UnmuteClientMessage::ResponseCreate {
                response: Some(ResponseConfig {
                    instructions: Some(text),
                    voice: Some(voice),
                    modalities: Some(vec!["audio".to_string()]),
                }),
            };
            
            if let Err(e) = write.send(Message::Text(serde_json::to_string(&response_msg).unwrap())).await {
                let _ = tx.send(Err(VoiceError::WebSocketError(format!("Failed to request TTS: {}", e)).into())).await;
                return;
            }

            // Stream audio chunks as they arrive (realtime!)
            let mut chunk_index = 0;
            let start_time = std::time::Instant::now();
            
            let result = timeout(Duration::from_secs(OPERATION_TIMEOUT_SECS), async {
                while let Some(msg_result) = read.next().await {
                    match msg_result {
                        Ok(Message::Text(text)) => {
                            match serde_json::from_str::<UnmuteServerMessage>(&text) {
                                Ok(UnmuteServerMessage::AudioDelta { delta, .. }) => {
                                    // Decode and emit chunk immediately (realtime streaming!)
                                    if let Ok(chunk_data) = base64::Engine::decode(
                                        &base64::engine::general_purpose::STANDARD,
                                        &delta,
                                    ) {
                                        let latency_ms = start_time.elapsed().as_millis() as u64;
                                        
                                        // Emit chunk immediately - this is realtime!
                                        if let Err(_) = tx.send(Ok(AudioChunk {
                                            data: Bytes::from(chunk_data),
                                            index: chunk_index,
                                            is_final: false,
                                            timestamp_ms: Some(latency_ms),
                                        })).await {
                                            // Receiver dropped, stop streaming
                                            break;
                                        }
                                        chunk_index += 1;
                                    }
                                }
                                Ok(UnmuteServerMessage::AudioDone { .. }) => {
                                    // Send final chunk marker
                                    let _ = tx.send(Ok(AudioChunk {
                                        data: Bytes::new(),
                                        index: chunk_index,
                                        is_final: true,
                                        timestamp_ms: Some(start_time.elapsed().as_millis() as u64),
                                    })).await;
                                    break;
                                }
                                Ok(UnmuteServerMessage::ResponseDone { .. }) => {
                                    break;
                                }
                                Ok(UnmuteServerMessage::Error { error }) => {
                                    let _ = tx.send(Err(VoiceError::AudioError(
                                        format!("Unmute TTS error: {}", error.message)
                                    ).into())).await;
                                    break;
                                }
                                Ok(_) => continue,
                                Err(e) => {
                                    debug!("Failed to parse Unmute message: {}", e);
                                    continue;
                                }
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Err(e) => {
                            let _ = tx.send(Err(VoiceError::WebSocketError(format!("WebSocket error: {}", e)).into())).await;
                            break;
                        }
                        _ => continue,
                    }
                }
            })
            .await;

            if result.is_err() {
                let _ = tx.send(Err(VoiceError::AudioError("TTS timeout".to_string()).into())).await;
            }
        });
        
        Ok(rx)
    }

    async fn available_voices(&self) -> Result<Vec<Voice>> {
        // Unmute voices depend on configuration
        // Return a placeholder list
        Ok(vec![
            Voice::custom(
                "default".to_string(),
                "Default".to_string(),
                VoiceGender::Neutral,
                "en-US".to_string(),
            ),
        ])
    }

    async fn is_ready(&self) -> bool {
        self.health_check().await
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Mp3, AudioFormat::Opus, AudioFormat::Wav]
    }

    fn max_text_length(&self) -> usize {
        8192
    }
}

// Stub implementation when unmute feature is disabled
#[cfg(not(feature = "unmute"))]
pub struct UnmuteEngine;

#[cfg(not(feature = "unmute"))]
impl UnmuteEngine {
    pub fn new() -> Self {
        Self
    }
    
    pub fn with_endpoint(_endpoint: &str) -> Self {
        Self
    }
}

#[cfg(all(test, feature = "unmute"))]
mod tests {
    use super::*;

    #[test]
    fn test_unmute_default_endpoint() {
        let engine = UnmuteEngine::new();
        assert_eq!(engine.endpoint(), DEFAULT_ENDPOINT);
    }

    #[test]
    fn test_unmute_custom_endpoint() {
        let engine = UnmuteEngine::with_endpoint("ws://custom:9000");
        assert_eq!(engine.endpoint(), "ws://custom:9000");
    }
}

