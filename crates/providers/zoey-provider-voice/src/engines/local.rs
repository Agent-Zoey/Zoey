//! Local HTTP TTS Engine
//!
//! Supports local TTS servers like:
//! - OpenVoice (https://github.com/myshell-ai/OpenVoice)
//! - Coqui TTS (https://github.com/coqui-ai/TTS)
//! - RealtimeTTS servers (https://github.com/KoljaB/RealtimeTTS)
//! - Any HTTP-based TTS API
//!
//! This engine provides flexibility to connect to any local or remote
//! TTS service that exposes an HTTP API.

use async_trait::async_trait;
use bytes::Bytes;
use zoey_core::Result;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;

use crate::types::*;

/// Shared HTTP client for connection pooling
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

/// Local TTS server protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalProtocol {
    /// OpenVoice API format
    OpenVoice,
    /// Coqui TTS API format
    Coqui,
    /// Generic REST API (POST with JSON body)
    Generic,
    /// Simple GET with query parameters
    SimpleGet,
}

impl Default for LocalProtocol {
    fn default() -> Self {
        Self::Generic
    }
}

/// OpenVoice request format
#[derive(Debug, Serialize)]
struct OpenVoiceRequest {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f32>,
}

/// Coqui TTS request format
#[derive(Debug, Serialize)]
struct CoquiRequest {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    speaker_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    style_wav: Option<String>,
}

/// Generic TTS request format
#[derive(Debug, Serialize)]
struct GenericTTSRequest {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

/// Local voice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalVoiceConfig {
    /// Server endpoint URL
    pub endpoint: String,
    /// Protocol type
    pub protocol: LocalProtocol,
    /// Path for TTS endpoint (default: /tts or /api/tts)
    pub tts_path: String,
    /// Path for streaming endpoint (if supported)
    pub stream_path: Option<String>,
    /// Path for listing voices (if supported)
    pub voices_path: Option<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Custom headers
    pub headers: std::collections::HashMap<String, String>,
}

impl Default for LocalVoiceConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8000".to_string(),
            protocol: LocalProtocol::Generic,
            tts_path: "/api/tts".to_string(),
            stream_path: Some("/api/tts/stream".to_string()),
            voices_path: Some("/api/voices".to_string()),
            timeout_secs: 30,
            headers: std::collections::HashMap::new(),
        }
    }
}

impl LocalVoiceConfig {
    /// Create config for OpenVoice server
    pub fn openvoice(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            protocol: LocalProtocol::OpenVoice,
            tts_path: "/synthesize".to_string(),
            stream_path: None,
            voices_path: Some("/speakers".to_string()),
            timeout_secs: 60,
            headers: std::collections::HashMap::new(),
        }
    }

    /// Create config for Coqui TTS server
    pub fn coqui(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            protocol: LocalProtocol::Coqui,
            tts_path: "/api/tts".to_string(),
            stream_path: None,
            voices_path: Some("/api/speakers".to_string()),
            timeout_secs: 60,
            headers: std::collections::HashMap::new(),
        }
    }

    /// Create config for generic TTS server
    pub fn generic(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            ..Default::default()
        }
    }
}

/// Local voice engine
pub struct LocalVoiceEngine {
    /// Server configuration
    config: LocalVoiceConfig,
}

impl LocalVoiceEngine {
    /// Create new local voice engine with endpoint
    pub fn new(endpoint: String) -> Self {
        Self {
            config: LocalVoiceConfig::generic(&endpoint),
        }
    }

    /// Create with OpenVoice server
    pub fn openvoice(endpoint: &str) -> Self {
        Self {
            config: LocalVoiceConfig::openvoice(endpoint),
        }
    }

    /// Create with Coqui TTS server
    pub fn coqui(endpoint: &str) -> Self {
        Self {
            config: LocalVoiceConfig::coqui(endpoint),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: LocalVoiceConfig) -> Self {
        Self { config }
    }

    /// Get HTTP client
    fn client() -> &'static Client {
        HTTP_CLIENT.get_or_init(|| {
            Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client")
        })
    }

    /// Build full URL
    fn build_url(&self, path: &str) -> String {
        format!("{}{}", self.config.endpoint, path)
    }

    /// Build request based on protocol
    fn build_request_body(&self, text: &str, voice_config: &VoiceConfig) -> serde_json::Value {
        match self.config.protocol {
            LocalProtocol::OpenVoice => serde_json::to_value(OpenVoiceRequest {
                text: text.to_string(),
                speaker: Some(voice_config.voice.id.clone()),
                language: Some(voice_config.voice.language.clone()),
                speed: if (voice_config.speed - 1.0).abs() > 0.01 {
                    Some(voice_config.speed)
                } else {
                    None
                },
            })
            .unwrap_or_default(),
            LocalProtocol::Coqui => serde_json::to_value(CoquiRequest {
                text: text.to_string(),
                speaker_id: Some(voice_config.voice.id.clone()),
                language_id: None,
                style_wav: None,
            })
            .unwrap_or_default(),
            LocalProtocol::Generic | LocalProtocol::SimpleGet => {
                serde_json::to_value(GenericTTSRequest {
                    text: text.to_string(),
                    voice: Some(voice_config.voice.id.clone()),
                    language: Some(voice_config.voice.language.clone()),
                    speed: if (voice_config.speed - 1.0).abs() > 0.01 {
                        Some(voice_config.speed)
                    } else {
                        None
                    },
                    format: Some(voice_config.output_format.as_str().to_string()),
                })
                .unwrap_or_default()
            }
        }
    }

    /// Get default local voices
    pub fn get_default_voices() -> Vec<Voice> {
        vec![
            Voice {
                id: "default".to_string(),
                name: "Default".to_string(),
                gender: VoiceGender::Female,
                language: "en-US".to_string(),
                description: Some("Default local TTS voice".to_string()),
                preview_url: None,
            },
            Voice {
                id: "female_1".to_string(),
                name: "Female Voice 1".to_string(),
                gender: VoiceGender::Female,
                language: "en-US".to_string(),
                description: Some("Female voice option 1".to_string()),
                preview_url: None,
            },
            Voice {
                id: "female_2".to_string(),
                name: "Female Voice 2".to_string(),
                gender: VoiceGender::Female,
                language: "en-US".to_string(),
                description: Some("Female voice option 2".to_string()),
                preview_url: None,
            },
            Voice {
                id: "male_1".to_string(),
                name: "Male Voice 1".to_string(),
                gender: VoiceGender::Male,
                language: "en-US".to_string(),
                description: Some("Male voice option 1".to_string()),
                preview_url: None,
            },
        ]
    }
}

#[async_trait]
impl VoiceEngine for LocalVoiceEngine {
    fn name(&self) -> &str {
        "local"
    }

    async fn synthesize(&self, text: &str, config: &VoiceConfig) -> Result<AudioData> {
        // Check text length
        if text.len() > self.max_text_length() {
            return Err(VoiceError::TextTooLong {
                length: text.len(),
                max: self.max_text_length(),
            }
            .into());
        }

        let url = self.build_url(&self.config.tts_path);

        tracing::debug!(
            "Local TTS request: endpoint={}, protocol={:?}, text_len={}",
            url,
            self.config.protocol,
            text.len()
        );

        let client = Self::client();
        let mut request = client
            .post(&url)
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .header("Content-Type", "application/json");

        // Add custom headers
        for (key, value) in &self.config.headers {
            request = request.header(key, value);
        }

        // Handle SimpleGet protocol differently
        let response = if self.config.protocol == LocalProtocol::SimpleGet {
            client
                .get(&url)
                .timeout(Duration::from_secs(self.config.timeout_secs))
                .query(&[
                    ("text", text),
                    ("voice", &config.voice.id),
                    ("format", config.output_format.as_str()),
                ])
                .send()
                .await
        } else {
            let body = self.build_request_body(text, config);
            request.json(&body).send().await
        };

        let response = response.map_err(|e| {
            if e.is_connect() {
                VoiceError::NotReady(format!(
                    "Cannot connect to local TTS server at {}: {}",
                    self.config.endpoint, e
                ))
            } else if e.is_timeout() {
                VoiceError::NetworkError(format!("Request timed out: {}", e))
            } else {
                VoiceError::NetworkError(e.to_string())
            }
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();

            return Err(
                VoiceError::Other(format!("Local TTS error ({}): {}", status, error_text)).into(),
            );
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| VoiceError::NetworkError(e.to_string()))?;

        tracing::debug!("Local TTS response: {} bytes", bytes.len());

        Ok(AudioData {
            data: bytes,
            format: config.output_format,
            sample_rate: config.sample_rate,
            duration_ms: None,
            character_count: text.len(),
        })
    }

    async fn synthesize_stream(&self, text: &str, config: &VoiceConfig) -> Result<AudioStream> {
        let stream_path = match &self.config.stream_path {
            Some(path) => path.clone(),
            None => {
                // Fallback to non-streaming and simulate stream
                let audio = self.synthesize(text, config).await?;
                let (tx, rx) = create_audio_stream(2);

                tokio::spawn(async move {
                    let _ = tx
                        .send(Ok(AudioChunk {
                            data: audio.data,
                            index: 0,
                            is_final: false,
                            timestamp_ms: None,
                        }))
                        .await;

                    let _ = tx
                        .send(Ok(AudioChunk {
                            data: Bytes::new(),
                            index: 1,
                            is_final: true,
                            timestamp_ms: None,
                        }))
                        .await;
                });

                return Ok(rx);
            }
        };

        let url = self.build_url(&stream_path);
        let body = self.build_request_body(text, config);
        let timeout_secs = self.config.timeout_secs;
        let headers = self.config.headers.clone();

        let (tx, rx) = create_audio_stream(32);

        tokio::spawn(async move {
            let client = LocalVoiceEngine::client();
            let mut request = client
                .post(&url)
                .timeout(Duration::from_secs(timeout_secs))
                .header("Content-Type", "application/json");

            for (key, value) in &headers {
                request = request.header(key, value);
            }

            let result = request.json(&body).send().await;

            match result {
                Ok(response) => {
                    if !response.status().is_success() {
                        let error =
                            VoiceError::Other(format!("Local TTS error: {}", response.status()));
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
                                    break;
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

                    let final_chunk = AudioChunk {
                        data: Bytes::new(),
                        index: chunk_index,
                        is_final: true,
                        timestamp_ms: None,
                    };
                    let _ = tx.send(Ok(final_chunk)).await;
                }
                Err(e) => {
                    let error = if e.is_connect() {
                        VoiceError::NotReady(format!("Cannot connect to local TTS server: {}", e))
                    } else {
                        VoiceError::NetworkError(e.to_string())
                    };
                    let _ = tx.send(Err(error.into())).await;
                }
            }
        });

        Ok(rx)
    }

    async fn available_voices(&self) -> Result<Vec<Voice>> {
        let voices_path = match &self.config.voices_path {
            Some(path) => path,
            None => return Ok(Self::get_default_voices()),
        };

        let url = self.build_url(voices_path);

        let response = Self::client()
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                // Try to parse JSON array of voice info
                #[derive(Deserialize)]
                struct VoiceInfo {
                    #[serde(alias = "voice_id", alias = "speaker_id")]
                    id: String,
                    #[serde(alias = "speaker_name")]
                    name: Option<String>,
                    #[serde(default)]
                    gender: Option<String>,
                    #[serde(default)]
                    language: Option<String>,
                }

                if let Ok(voices) = resp.json::<Vec<VoiceInfo>>().await {
                    return Ok(voices
                        .into_iter()
                        .map(|v| Voice {
                            id: v.id.clone(),
                            name: v.name.unwrap_or_else(|| v.id.clone()),
                            gender: v
                                .gender
                                .map(|g| match g.to_lowercase().as_str() {
                                    "female" | "f" => VoiceGender::Female,
                                    "male" | "m" => VoiceGender::Male,
                                    _ => VoiceGender::Neutral,
                                })
                                .unwrap_or(VoiceGender::Neutral),
                            language: v.language.unwrap_or_else(|| "en-US".to_string()),
                            description: None,
                            preview_url: None,
                        })
                        .collect());
                }
            }
            _ => {}
        }

        // Return default voices if API call fails
        Ok(Self::get_default_voices())
    }

    async fn is_ready(&self) -> bool {
        // Try to connect to the server
        let url = self.build_url("/health");
        let alt_url = self.build_url("/");

        let client = Self::client();

        // Try health endpoint first
        if let Ok(resp) = client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            if resp.status().is_success() {
                return true;
            }
        }

        // Try root endpoint
        if let Ok(resp) = client
            .get(&alt_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            return resp.status().is_success();
        }

        false
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        // Most local TTS servers support WAV and MP3
        vec![AudioFormat::Wav, AudioFormat::Mp3, AudioFormat::Pcm]
    }

    fn max_text_length(&self) -> usize {
        // Local servers typically have higher limits
        10000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_voice_config_default() {
        let config = LocalVoiceConfig::default();
        assert_eq!(config.endpoint, "http://localhost:8000");
        assert_eq!(config.protocol, LocalProtocol::Generic);
    }

    #[test]
    fn test_openvoice_config() {
        let config = LocalVoiceConfig::openvoice("http://localhost:5000");
        assert_eq!(config.endpoint, "http://localhost:5000");
        assert_eq!(config.protocol, LocalProtocol::OpenVoice);
        assert_eq!(config.tts_path, "/synthesize");
    }

    #[test]
    fn test_coqui_config() {
        let config = LocalVoiceConfig::coqui("http://localhost:5002");
        assert_eq!(config.endpoint, "http://localhost:5002");
        assert_eq!(config.protocol, LocalProtocol::Coqui);
        assert_eq!(config.tts_path, "/api/tts");
    }

    #[test]
    fn test_default_voices() {
        let voices = LocalVoiceEngine::get_default_voices();
        assert!(!voices.is_empty());

        // Check default female voice exists
        let female = voices.iter().find(|v| v.gender == VoiceGender::Female);
        assert!(female.is_some());
    }
}
