//! OpenAI TTS Engine
//!
//! Supports OpenAI's text-to-speech API with:
//! - tts-1: Optimized for low latency (default)
//! - tts-1-hd: Higher quality, slightly higher latency
//!
//! Voices: alloy, echo, fable, onyx, nova, shimmer
//! Default female voice: shimmer

use async_trait::async_trait;
use bytes::Bytes;
use zoey_core::{ZoeyError, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use std::env;
use std::sync::OnceLock;

use crate::types::*;

/// OpenAI API base URL
const OPENAI_API_BASE: &str = "https://api.openai.com/v1";

/// Shared HTTP client for connection pooling
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

/// OpenAI TTS model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAIModel {
    /// tts-1: Low latency, good quality
    Tts1,
    /// tts-1-hd: Higher quality, slightly higher latency
    Tts1Hd,
}

impl OpenAIModel {
    /// Get model string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tts1 => "tts-1",
            Self::Tts1Hd => "tts-1-hd",
        }
    }
}

impl Default for OpenAIModel {
    fn default() -> Self {
        Self::Tts1 // Low latency default
    }
}

/// OpenAI TTS request
#[derive(Debug, Serialize)]
struct OpenAITTSRequest {
    model: String,
    input: String,
    voice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f32>,
}

/// OpenAI voice engine
pub struct OpenAIVoiceEngine {
    /// API key (optional, uses OPENAI_API_KEY env var if not set)
    api_key: Option<String>,
    /// Model to use
    model: OpenAIModel,
}

impl OpenAIVoiceEngine {
    /// Create new OpenAI voice engine
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key,
            model: OpenAIModel::default(),
        }
    }

    /// Create with HD model for higher quality
    pub fn with_hd_model(api_key: Option<String>) -> Self {
        Self {
            api_key,
            model: OpenAIModel::Tts1Hd,
        }
    }

    /// Get HTTP client
    fn client() -> &'static Client {
        HTTP_CLIENT.get_or_init(|| {
            Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client")
        })
    }

    /// Get API key
    fn get_api_key(&self) -> Result<String> {
        self.api_key
            .clone()
            .or_else(|| env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| {
                ZoeyError::other(
                    "OpenAI API key not found. Set OPENAI_API_KEY environment variable or provide key.",
                )
            })
    }

    /// Map audio format to OpenAI format string
    fn map_format(format: AudioFormat) -> &'static str {
        match format {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Opus => "opus",
            AudioFormat::Aac => "aac",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
            AudioFormat::Pcm => "pcm",
        }
    }

    /// Get available OpenAI voices
    pub fn get_voices() -> Vec<Voice> {
        vec![
            Voice::openai_alloy(),
            Voice::openai_echo(),
            Voice::openai_fable(),
            Voice::openai_nova(),
            Voice::openai_onyx(),
            Voice::openai_shimmer(),
        ]
    }
}

#[async_trait]
impl VoiceEngine for OpenAIVoiceEngine {
    fn name(&self) -> &str {
        "openai"
    }

    async fn synthesize(&self, text: &str, config: &VoiceConfig) -> Result<AudioData> {
        let api_key = self.get_api_key()?;

        // Check text length
        if text.len() > self.max_text_length() {
            return Err(VoiceError::TextTooLong {
                length: text.len(),
                max: self.max_text_length(),
            }
            .into());
        }

        let model = config
            .model
            .as_ref()
            .map(|m| m.as_str())
            .unwrap_or_else(|| self.model.as_str());

        let request = OpenAITTSRequest {
            model: model.to_string(),
            input: text.to_string(),
            voice: config.voice.id.clone(),
            response_format: Some(Self::map_format(config.output_format).to_string()),
            speed: if (config.speed - 1.0).abs() > 0.01 {
                Some(config.speed)
            } else {
                None
            },
        };

        tracing::debug!(
            "OpenAI TTS request: model={}, voice={}, format={}, text_len={}",
            model,
            config.voice.id,
            config.output_format.as_str(),
            text.len()
        );

        let response = Self::client()
            .post(format!("{}/audio/speech", OPENAI_API_BASE))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| VoiceError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();

            if status.as_u16() == 401 {
                return Err(VoiceError::AuthenticationError(error_text).into());
            } else if status.as_u16() == 429 {
                return Err(VoiceError::RateLimitError(error_text).into());
            }

            return Err(VoiceError::Other(format!(
                "OpenAI TTS error ({}): {}",
                status, error_text
            ))
            .into());
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| VoiceError::NetworkError(e.to_string()))?;

        tracing::debug!("OpenAI TTS response: {} bytes", bytes.len());

        Ok(AudioData {
            data: bytes,
            format: config.output_format,
            sample_rate: config.sample_rate,
            duration_ms: None,
            character_count: text.len(),
        })
    }

    async fn synthesize_stream(&self, text: &str, config: &VoiceConfig) -> Result<AudioStream> {
        let api_key = self.get_api_key()?;

        // Check text length
        if text.len() > self.max_text_length() {
            return Err(VoiceError::TextTooLong {
                length: text.len(),
                max: self.max_text_length(),
            }
            .into());
        }

        let model = config
            .model
            .as_ref()
            .map(|m| m.as_str())
            .unwrap_or_else(|| self.model.as_str());

        let request = OpenAITTSRequest {
            model: model.to_string(),
            input: text.to_string(),
            voice: config.voice.id.clone(),
            response_format: Some(Self::map_format(config.output_format).to_string()),
            speed: if (config.speed - 1.0).abs() > 0.01 {
                Some(config.speed)
            } else {
                None
            },
        };

        let (tx, rx) = create_audio_stream(32);

        // Spawn async task to stream audio
        let client = Self::client().clone();
        tokio::spawn(async move {
            let result = client
                .post(format!("{}/audio/speech", OPENAI_API_BASE))
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await;

            match result {
                Ok(response) => {
                    if !response.status().is_success() {
                        let error =
                            VoiceError::Other(format!("OpenAI TTS error: {}", response.status()));
                        let _ = tx.send(Err(error.into())).await;
                        return;
                    }

                    let mut stream = response.bytes_stream();
                    let mut chunk_index = 0;

                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                let audio_chunk = AudioChunk {
                                    data: chunk,
                                    index: chunk_index,
                                    is_final: false,
                                    timestamp_ms: None,
                                };
                                if tx.send(Ok(audio_chunk)).await.is_err() {
                                    break; // Receiver dropped
                                }
                                chunk_index += 1;
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(Err(VoiceError::NetworkError(e.to_string()).into()))
                                    .await;
                                return;
                            }
                        }
                    }

                    // Send final chunk marker
                    let final_chunk = AudioChunk {
                        data: Bytes::new(),
                        index: chunk_index,
                        is_final: true,
                        timestamp_ms: None,
                    };
                    let _ = tx.send(Ok(final_chunk)).await;
                }
                Err(e) => {
                    let _ = tx
                        .send(Err(VoiceError::NetworkError(e.to_string()).into()))
                        .await;
                }
            }
        });

        Ok(rx)
    }

    async fn available_voices(&self) -> Result<Vec<Voice>> {
        Ok(Self::get_voices())
    }

    async fn is_ready(&self) -> bool {
        self.get_api_key().is_ok()
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![
            AudioFormat::Mp3,
            AudioFormat::Opus,
            AudioFormat::Aac,
            AudioFormat::Flac,
            AudioFormat::Wav,
            AudioFormat::Pcm,
        ]
    }

    fn max_text_length(&self) -> usize {
        4096
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_voices() {
        let voices = OpenAIVoiceEngine::get_voices();
        assert_eq!(voices.len(), 6);

        // Check shimmer is available (default female voice)
        let shimmer = voices.iter().find(|v| v.id == "shimmer");
        assert!(shimmer.is_some());
        assert_eq!(shimmer.unwrap().gender, VoiceGender::Female);
    }

    #[test]
    fn test_model_strings() {
        assert_eq!(OpenAIModel::Tts1.as_str(), "tts-1");
        assert_eq!(OpenAIModel::Tts1Hd.as_str(), "tts-1-hd");
    }

    #[test]
    fn test_format_mapping() {
        assert_eq!(OpenAIVoiceEngine::map_format(AudioFormat::Mp3), "mp3");
        assert_eq!(OpenAIVoiceEngine::map_format(AudioFormat::Opus), "opus");
    }
}
