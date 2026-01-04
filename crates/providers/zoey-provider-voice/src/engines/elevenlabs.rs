//! ElevenLabs TTS Engine
//!
//! High-quality text-to-speech with natural voices and emotion control.
//! Supports streaming for low latency playback.
//!
//! Default female voice: Rachel (conversational, natural)

use async_trait::async_trait;
use bytes::Bytes;
use zoey_core::{ZoeyError, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::OnceLock;

use crate::types::*;

/// ElevenLabs API base URL
const ELEVENLABS_API_BASE: &str = "https://api.elevenlabs.io/v1";

/// Shared HTTP client for connection pooling
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

/// ElevenLabs model ID
#[derive(Debug, Clone)]
pub enum ElevenLabsModel {
    /// Multilingual v2 - Best quality, supports many languages
    MultilingualV2,
    /// Turbo v2.5 - Fastest, English-only
    TurboV2_5,
    /// Monolingual v1 - Legacy English model
    MonolingualV1,
    /// Custom model ID
    Custom(String),
}

impl ElevenLabsModel {
    /// Get model ID string
    pub fn as_str(&self) -> &str {
        match self {
            Self::MultilingualV2 => "eleven_multilingual_v2",
            Self::TurboV2_5 => "eleven_turbo_v2_5",
            Self::MonolingualV1 => "eleven_monolingual_v1",
            Self::Custom(id) => id,
        }
    }
}

impl Default for ElevenLabsModel {
    fn default() -> Self {
        Self::TurboV2_5 // Low latency default
    }
}

/// ElevenLabs voice settings
#[derive(Debug, Serialize)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_speaker_boost: Option<bool>,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            stability: 0.5,
            similarity_boost: 0.75,
            style: None,
            use_speaker_boost: Some(true),
        }
    }
}

/// ElevenLabs TTS request
#[derive(Debug, Serialize)]
struct ElevenLabsTTSRequest {
    text: String,
    model_id: String,
    voice_settings: VoiceSettings,
}

/// ElevenLabs voice info from API
#[derive(Debug, Deserialize)]
struct ElevenLabsVoiceInfo {
    voice_id: String,
    name: String,
    #[serde(default)]
    labels: std::collections::HashMap<String, String>,
    preview_url: Option<String>,
}

/// ElevenLabs voices response
#[derive(Debug, Deserialize)]
struct ElevenLabsVoicesResponse {
    voices: Vec<ElevenLabsVoiceInfo>,
}

/// ElevenLabs voice engine
pub struct ElevenLabsVoiceEngine {
    /// API key (optional, uses ELEVENLABS_API_KEY env var if not set)
    api_key: Option<String>,
    /// Model to use
    model: ElevenLabsModel,
}

impl ElevenLabsVoiceEngine {
    /// Create new ElevenLabs voice engine
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key,
            model: ElevenLabsModel::default(),
        }
    }

    /// Create with multilingual model for best quality
    pub fn with_multilingual(api_key: Option<String>) -> Self {
        Self {
            api_key,
            model: ElevenLabsModel::MultilingualV2,
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
            .or_else(|| env::var("ELEVENLABS_API_KEY").ok())
            .ok_or_else(|| {
                ZoeyError::other(
                    "ElevenLabs API key not found. Set ELEVENLABS_API_KEY environment variable or provide key.",
                )
            })
    }

    /// Map audio format to ElevenLabs output format
    fn map_format(format: AudioFormat) -> &'static str {
        match format {
            AudioFormat::Mp3 => "mp3_44100_128",
            AudioFormat::Pcm => "pcm_24000",
            _ => "mp3_44100_128", // Default to high quality MP3
        }
    }

    /// Get predefined ElevenLabs voices
    pub fn get_predefined_voices() -> Vec<Voice> {
        vec![
            Voice::elevenlabs_rachel(),
            Voice::elevenlabs_domi(),
            Voice::elevenlabs_bella(),
            Voice {
                id: "pNInz6obpgDQGcFmaJgB".to_string(),
                name: "Adam".to_string(),
                gender: VoiceGender::Male,
                language: "en-US".to_string(),
                description: Some("Deep, clear male voice".to_string()),
                preview_url: None,
            },
            Voice {
                id: "ErXwobaYiN019PkySvjV".to_string(),
                name: "Antoni".to_string(),
                gender: VoiceGender::Male,
                language: "en-US".to_string(),
                description: Some("Well-rounded, warm male voice".to_string()),
                preview_url: None,
            },
            Voice {
                id: "VR6AewLTigWG4xSOukaG".to_string(),
                name: "Arnold".to_string(),
                gender: VoiceGender::Male,
                language: "en-US".to_string(),
                description: Some("Crisp, authoritative male voice".to_string()),
                preview_url: None,
            },
        ]
    }

    /// Map API voice info to Voice struct
    fn map_voice_info(info: ElevenLabsVoiceInfo) -> Voice {
        let gender = info
            .labels
            .get("gender")
            .map(|g| match g.to_lowercase().as_str() {
                "female" => VoiceGender::Female,
                "male" => VoiceGender::Male,
                _ => VoiceGender::Neutral,
            })
            .unwrap_or(VoiceGender::Neutral);

        Voice {
            id: info.voice_id,
            name: info.name,
            gender,
            language: "en-US".to_string(),
            description: info.labels.get("description").cloned(),
            preview_url: info.preview_url,
        }
    }
}

#[async_trait]
impl VoiceEngine for ElevenLabsVoiceEngine {
    fn name(&self) -> &str {
        "elevenlabs"
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

        let voice_settings = VoiceSettings {
            stability: config.stability.unwrap_or(0.5),
            similarity_boost: config.similarity_boost.unwrap_or(0.75),
            style: config.style,
            use_speaker_boost: Some(true),
        };

        let request = ElevenLabsTTSRequest {
            text: text.to_string(),
            model_id: model.to_string(),
            voice_settings,
        };

        let output_format = Self::map_format(config.output_format);

        tracing::debug!(
            "ElevenLabs TTS request: model={}, voice={}, format={}, text_len={}",
            model,
            config.voice.id,
            output_format,
            text.len()
        );

        let url = format!(
            "{}/text-to-speech/{}?output_format={}",
            ELEVENLABS_API_BASE, config.voice.id, output_format
        );

        let response = Self::client()
            .post(&url)
            .header("xi-api-key", &api_key)
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
                "ElevenLabs TTS error ({}): {}",
                status, error_text
            ))
            .into());
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| VoiceError::NetworkError(e.to_string()))?;

        tracing::debug!("ElevenLabs TTS response: {} bytes", bytes.len());

        Ok(AudioData {
            data: bytes,
            format: config.output_format,
            sample_rate: 44100, // ElevenLabs default
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
            .map(|m| m.to_string())
            .unwrap_or_else(|| self.model.as_str().to_string());

        let voice_id = config.voice.id.clone();
        let stability = config.stability.unwrap_or(0.5);
        let similarity_boost = config.similarity_boost.unwrap_or(0.75);
        let style = config.style;
        let text = text.to_string();
        let output_format = Self::map_format(config.output_format).to_string();

        let (tx, rx) = create_audio_stream(32);

        // Spawn async task to stream audio
        let client = Self::client().clone();
        tokio::spawn(async move {
            let voice_settings = VoiceSettings {
                stability,
                similarity_boost,
                style,
                use_speaker_boost: Some(true),
            };

            let request = ElevenLabsTTSRequest {
                text,
                model_id: model,
                voice_settings,
            };

            let url = format!(
                "{}/text-to-speech/{}/stream?output_format={}",
                ELEVENLABS_API_BASE, voice_id, output_format
            );

            let result = client
                .post(&url)
                .header("xi-api-key", &api_key)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await;

            match result {
                Ok(response) => {
                    if !response.status().is_success() {
                        let error = VoiceError::Other(format!(
                            "ElevenLabs TTS error: {}",
                            response.status()
                        ));
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
        let api_key = match self.get_api_key() {
            Ok(key) => key,
            Err(_) => {
                // Return predefined voices if no API key
                return Ok(Self::get_predefined_voices());
            }
        };

        let response = Self::client()
            .get(format!("{}/voices", ELEVENLABS_API_BASE))
            .header("xi-api-key", &api_key)
            .send()
            .await
            .map_err(|e| VoiceError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            // Return predefined voices on error
            return Ok(Self::get_predefined_voices());
        }

        let voices_response: ElevenLabsVoicesResponse = response
            .json()
            .await
            .map_err(|e| VoiceError::Other(format!("Failed to parse voices: {}", e)))?;

        Ok(voices_response
            .voices
            .into_iter()
            .map(Self::map_voice_info)
            .collect())
    }

    async fn is_ready(&self) -> bool {
        self.get_api_key().is_ok()
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Mp3, AudioFormat::Pcm]
    }

    fn max_text_length(&self) -> usize {
        5000 // ElevenLabs limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elevenlabs_predefined_voices() {
        let voices = ElevenLabsVoiceEngine::get_predefined_voices();
        assert!(!voices.is_empty());

        // Check Rachel is available (default female voice)
        let rachel = voices.iter().find(|v| v.name == "Rachel");
        assert!(rachel.is_some());
        assert_eq!(rachel.unwrap().gender, VoiceGender::Female);
    }

    #[test]
    fn test_model_strings() {
        assert_eq!(ElevenLabsModel::TurboV2_5.as_str(), "eleven_turbo_v2_5");
        assert_eq!(
            ElevenLabsModel::MultilingualV2.as_str(),
            "eleven_multilingual_v2"
        );
    }

    #[test]
    fn test_default_voice_settings() {
        let settings = VoiceSettings::default();
        assert_eq!(settings.stability, 0.5);
        assert_eq!(settings.similarity_boost, 0.75);
    }
}
