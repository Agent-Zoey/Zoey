//! Pocket TTS Engine - Lightweight CPU-Based Text-to-Speech
//!
//! Pocket TTS is a fast, lightweight TTS system by Kyutai Labs that runs entirely
//! on CPU. It achieves ~200ms latency for the first audio chunk and ~6x real-time
//! synthesis speed with only 100M parameters.
//!
//! ## Key Features
//! - **CPU-Only**: No GPU required, runs on any machine
//! - **Low Latency**: ~200ms to first audio chunk
//! - **Fast**: ~6x real-time synthesis speed
//! - **Small Model**: Only 100M parameters
//! - **Voice Cloning**: Clone voices from audio samples
//! - **Audio Streaming**: Stream audio as it's generated
//!
//! ## Setup
//!
//! ### Option 1: pip install (Recommended)
//! ```bash
//! pip install pocket-tts
//! # or with uv:
//! uv pip install pocket-tts
//! ```
//!
//! ### Option 2: Using uvx (No installation needed)
//! ```bash
//! uvx pocket-tts serve
//! ```
//!
//! ## Running the Server
//! ```bash
//! # Start the server (default port 8000)
//! pocket-tts serve
//!
//! # Or specify a port
//! pocket-tts serve --port 8080
//! ```
//!
//! ## Available Voices
//!
//! Pocket TTS includes several built-in voices:
//! - `alba` - Female voice
//! - `marius` - Male voice
//! - `javert` - Male voice
//! - `jean` - Male voice
//! - `fantine` - Female voice
//! - `cosette` - Female voice
//! - `eponine` - Female voice
//! - `azelma` - Female voice
//!
//! ## Voice Cloning
//!
//! You can clone voices by providing a WAV file path or HuggingFace URL:
//! ```rust,ignore
//! // From HuggingFace
//! let engine = PocketTTSEngine::new("http://localhost:8000")
//!     .with_voice_prompt("hf://kyutai/tts-voices/alba-mackenna/casual.wav");
//!
//! // From local file
//! let engine = PocketTTSEngine::new("http://localhost:8000")
//!     .with_voice_prompt("/path/to/voice.wav");
//! ```
//!
//! Reference: https://github.com/kyutai-labs/pocket-tts

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

use crate::types::*;

/// Default Pocket TTS server port
const DEFAULT_POCKET_TTS_PORT: u16 = 8000;

/// Default sample rate for Pocket TTS output (24kHz)
const DEFAULT_SAMPLE_RATE: u32 = 24000;

/// Pocket TTS voice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PocketTTSVoice {
    /// Voice identifier (e.g., "alba", "marius", or path to WAV for cloning)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Language code (English only for now)
    pub language: String,
    /// Sample rate (24000 Hz)
    pub sample_rate: u32,
    /// Whether this is a custom voice (voice cloning)
    pub is_custom: bool,
    /// Voice description
    pub description: Option<String>,
}

impl Default for PocketTTSVoice {
    fn default() -> Self {
        Self::alba()
    }
}

impl PocketTTSVoice {
    /// Alba voice - Female, default voice
    pub fn alba() -> Self {
        Self {
            id: "alba".to_string(),
            name: "Alba".to_string(),
            language: "en".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            is_custom: false,
            description: Some("Clear female voice - default".to_string()),
        }
    }

    /// Marius voice - Male
    pub fn marius() -> Self {
        Self {
            id: "marius".to_string(),
            name: "Marius".to_string(),
            language: "en".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            is_custom: false,
            description: Some("Male voice".to_string()),
        }
    }

    /// Javert voice - Male
    pub fn javert() -> Self {
        Self {
            id: "javert".to_string(),
            name: "Javert".to_string(),
            language: "en".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            is_custom: false,
            description: Some("Male voice".to_string()),
        }
    }

    /// Jean voice - Male
    pub fn jean() -> Self {
        Self {
            id: "jean".to_string(),
            name: "Jean".to_string(),
            language: "en".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            is_custom: false,
            description: Some("Male voice".to_string()),
        }
    }

    /// Fantine voice - Female
    pub fn fantine() -> Self {
        Self {
            id: "fantine".to_string(),
            name: "Fantine".to_string(),
            language: "en".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            is_custom: false,
            description: Some("Female voice".to_string()),
        }
    }

    /// Cosette voice - Female
    pub fn cosette() -> Self {
        Self {
            id: "cosette".to_string(),
            name: "Cosette".to_string(),
            language: "en".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            is_custom: false,
            description: Some("Female voice".to_string()),
        }
    }

    /// Eponine voice - Female
    pub fn eponine() -> Self {
        Self {
            id: "eponine".to_string(),
            name: "Eponine".to_string(),
            language: "en".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            is_custom: false,
            description: Some("Female voice".to_string()),
        }
    }

    /// Azelma voice - Female
    pub fn azelma() -> Self {
        Self {
            id: "azelma".to_string(),
            name: "Azelma".to_string(),
            language: "en".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            is_custom: false,
            description: Some("Female voice".to_string()),
        }
    }

    /// Create a custom voice for voice cloning
    /// 
    /// The voice_path can be:
    /// - A local file path: `/path/to/voice.wav`
    /// - A HuggingFace URL: `hf://kyutai/tts-voices/alba-mackenna/casual.wav`
    pub fn custom(voice_path: &str, name: &str) -> Self {
        Self {
            id: voice_path.to_string(),
            name: name.to_string(),
            language: "en".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            is_custom: true,
            description: Some(format!("Custom cloned voice from {}", voice_path)),
        }
    }

    /// Get all available built-in voices
    pub fn all_builtin() -> Vec<Self> {
        vec![
            Self::alba(),
            Self::marius(),
            Self::javert(),
            Self::jean(),
            Self::fantine(),
            Self::cosette(),
            Self::eponine(),
            Self::azelma(),
        ]
    }
}

/// Pocket TTS Engine
///
/// Connects to a Pocket TTS server for CPU-based speech synthesis.
///
/// ## Example
/// ```rust,ignore
/// let engine = PocketTTSEngine::new("http://localhost:8000");
/// let audio = engine.synthesize("Hello world!", &config).await?;
/// ```
pub struct PocketTTSEngine {
    /// HTTP client (reused for connection pooling)
    client: Client,
    /// Server endpoint URL
    endpoint: String,
    /// Active voice
    voice: PocketTTSVoice,
}

impl PocketTTSEngine {
    /// Create new Pocket TTS engine with default settings
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .pool_max_idle_per_host(4)
                .build()
                .expect("Failed to create HTTP client"),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            voice: PocketTTSVoice::default(),
        }
    }

    /// Create with localhost and default port (8000)
    pub fn localhost() -> Self {
        Self::new(&format!("http://127.0.0.1:{}", DEFAULT_POCKET_TTS_PORT))
    }

    /// Create engine with auto-start server
    /// 
    /// This will automatically start a pocket-tts server process if not already running.
    /// Requires pocket-tts to be installed via pip or uvx to be available.
    /// 
    /// ## Example
    /// ```rust,ignore
    /// let engine = PocketTTSEngine::with_auto_start().await?;
    /// // Server is now running, ready to synthesize
    /// let audio = engine.synthesize("Hello!", &config).await?;
    /// ```
    pub async fn with_auto_start() -> Result<Self, crate::types::VoiceError> {
        use super::pocket_tts_server::start_pocket_tts_server;
        
        // Check if server is already running
        let engine = Self::localhost();
        if engine.health_check().await {
            return Ok(engine);
        }
        
        // Start server
        let server = start_pocket_tts_server().await?;
        
        // Leak the server handle so it stays alive
        // In production, use AutoStartPocketTTS for proper lifecycle management
        Box::leak(Box::new(server));
        
        Ok(Self::localhost())
    }

    /// Create engine with auto-start using uvx (no installation required)
    /// 
    /// ## Example
    /// ```rust,ignore
    /// let engine = PocketTTSEngine::with_auto_start_uvx().await?;
    /// ```
    pub async fn with_auto_start_uvx() -> Result<Self, crate::types::VoiceError> {
        use super::pocket_tts_server::start_pocket_tts_server_uvx;
        
        // Check if server is already running
        let engine = Self::localhost();
        if engine.health_check().await {
            return Ok(engine);
        }
        
        // Start server using uvx
        let server = start_pocket_tts_server_uvx().await?;
        Box::leak(Box::new(server));
        
        Ok(Self::localhost())
    }

    /// Create engine on a specific port with auto-start
    pub async fn with_auto_start_on_port(port: u16) -> Result<Self, crate::types::VoiceError> {
        use super::pocket_tts_server::start_pocket_tts_server_on_port;
        
        let endpoint = format!("http://127.0.0.1:{}", port);
        let engine = Self::new(&endpoint);
        
        if engine.health_check().await {
            return Ok(engine);
        }
        
        let server = start_pocket_tts_server_on_port(port).await?;
        Box::leak(Box::new(server));
        
        Ok(Self::new(&endpoint))
    }

    /// Set voice preset
    pub fn with_voice(mut self, voice: PocketTTSVoice) -> Self {
        self.voice = voice;
        self
    }

    /// Set voice by name (built-in voices)
    pub fn with_voice_name(mut self, name: &str) -> Self {
        self.voice = match name.to_lowercase().as_str() {
            "alba" => PocketTTSVoice::alba(),
            "marius" => PocketTTSVoice::marius(),
            "javert" => PocketTTSVoice::javert(),
            "jean" => PocketTTSVoice::jean(),
            "fantine" => PocketTTSVoice::fantine(),
            "cosette" => PocketTTSVoice::cosette(),
            "eponine" => PocketTTSVoice::eponine(),
            "azelma" => PocketTTSVoice::azelma(),
            _ => {
                warn!("Unknown voice '{}', using default (alba)", name);
                PocketTTSVoice::alba()
            }
        };
        self
    }

    /// Set custom voice prompt for voice cloning
    /// 
    /// The voice_path can be:
    /// - A local file path: `/path/to/voice.wav`
    /// - A HuggingFace URL: `hf://kyutai/tts-voices/alba-mackenna/casual.wav`
    pub fn with_voice_prompt(mut self, voice_path: &str) -> Self {
        self.voice = PocketTTSVoice::custom(voice_path, "Custom Voice");
        self
    }

    /// Get current voice configuration
    pub fn voice(&self) -> &PocketTTSVoice {
        &self.voice
    }

    /// Get server endpoint
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Health check - verify Pocket TTS server is reachable
    pub async fn health_check(&self) -> bool {
        // Try the root endpoint which should return the web interface
        let endpoints = [
            format!("{}/", self.endpoint),
            format!("{}/health", self.endpoint),
        ];

        for url in &endpoints {
            if let Ok(resp) = self.client.get(url).send().await {
                if resp.status().is_success() {
                    return true;
                }
            }
        }

        false
    }

    /// Synthesize text to WAV audio bytes
    pub async fn synthesize_wav(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        // Build the request URL with query parameters
        // The pocket-tts serve API accepts GET/POST with text and voice params
        let url = format!("{}/api/generate", self.endpoint);

        let request_body = if self.voice.is_custom {
            serde_json::json!({
                "text": text,
                "voice": self.voice.id,
            })
        } else {
            serde_json::json!({
                "text": text,
                "voice": self.voice.id,
            })
        };

        debug!("Pocket TTS request to {}: {:?}", url, request_body);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| VoiceError::NetworkError(format!("Failed to connect to Pocket TTS server: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::NetworkError(format!(
                "Pocket TTS server error ({}): {}",
                status, error_text
            )));
        }

        let audio_bytes = response
            .bytes()
            .await
            .map_err(|e| VoiceError::NetworkError(format!("Failed to read audio response: {}", e)))?;

        Ok(audio_bytes.to_vec())
    }

    /// Synthesize text to raw PCM audio (extracts from WAV)
    pub async fn synthesize_pcm(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        let wav_data = self.synthesize_wav(text).await?;
        
        // WAV header is 44 bytes, data follows
        if wav_data.len() <= 44 {
            return Err(VoiceError::AudioError("Invalid WAV data received".to_string()));
        }

        // Verify it's a valid WAV file
        if &wav_data[0..4] != b"RIFF" || &wav_data[8..12] != b"WAVE" {
            // If it's already raw PCM, return as-is
            debug!("Response is not WAV format, assuming raw PCM");
            return Ok(wav_data);
        }

        // Find the data chunk
        let mut offset = 12;
        while offset + 8 < wav_data.len() {
            let chunk_id = &wav_data[offset..offset + 4];
            let chunk_size = u32::from_le_bytes([
                wav_data[offset + 4],
                wav_data[offset + 5],
                wav_data[offset + 6],
                wav_data[offset + 7],
            ]) as usize;

            if chunk_id == b"data" {
                let data_start = offset + 8;
                let data_end = (data_start + chunk_size).min(wav_data.len());
                return Ok(wav_data[data_start..data_end].to_vec());
            }

            offset += 8 + chunk_size;
            // Align to even boundary
            if offset % 2 != 0 {
                offset += 1;
            }
        }

        // Fallback: skip standard 44-byte header
        Ok(wav_data[44..].to_vec())
    }

    /// Convert PCM data to i16 samples
    pub fn pcm_to_samples(pcm: &[u8]) -> Vec<i16> {
        pcm.chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some(i16::from_le_bytes([chunk[0], chunk[1]]))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[async_trait]
impl VoiceEngine for PocketTTSEngine {
    fn name(&self) -> &str {
        "pocket-tts"
    }

    async fn synthesize(&self, text: &str, _config: &VoiceConfig) -> zoey_core::Result<AudioData> {
        let wav_data = self.synthesize_wav(text).await?;

        // Determine if response is WAV or raw PCM
        let (format, data) = if wav_data.len() > 4 && &wav_data[0..4] == b"RIFF" {
            (AudioFormat::Wav, wav_data)
        } else {
            (AudioFormat::Pcm, wav_data)
        };

        Ok(AudioData {
            data: Bytes::from(data),
            format,
            sample_rate: self.voice.sample_rate,
            duration_ms: None,
            character_count: text.len(),
        })
    }

    async fn synthesize_stream(
        &self,
        text: &str,
        config: &VoiceConfig,
    ) -> zoey_core::Result<AudioStream> {
        // Pocket TTS supports streaming, but for simplicity we'll synthesize
        // and emit chunks. This can be enhanced to use true streaming later.
        let (tx, rx) = create_audio_stream(32);

        let audio = self.synthesize(text, config).await;

        tokio::spawn(async move {
            match audio {
                Ok(data) => {
                    // Emit in chunks for streaming behavior
                    let chunk_size = 4096;
                    let chunks: Vec<_> = data.data.chunks(chunk_size).collect();
                    let total = chunks.len();

                    for (i, chunk) in chunks.into_iter().enumerate() {
                        let _ = tx
                            .send(Ok(AudioChunk {
                                data: Bytes::copy_from_slice(chunk),
                                index: i,
                                is_final: i == total - 1,
                                timestamp_ms: None,
                            }))
                            .await;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        });

        Ok(rx)
    }

    async fn available_voices(&self) -> zoey_core::Result<Vec<Voice>> {
        Ok(PocketTTSVoice::all_builtin()
            .into_iter()
            .map(|v| {
                let gender = if v.id == "marius" || v.id == "javert" || v.id == "jean" {
                    VoiceGender::Male
                } else {
                    VoiceGender::Female
                };
                Voice {
                    id: v.id,
                    name: v.name,
                    gender,
                    language: v.language,
                    description: v.description,
                    preview_url: None,
                }
            })
            .collect())
    }

    async fn is_ready(&self) -> bool {
        self.health_check().await
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Wav, AudioFormat::Pcm]
    }

    fn max_text_length(&self) -> usize {
        // Pocket TTS can handle infinitely long text inputs
        100000
    }
}

// ============================================================================
// Setup Instructions
// ============================================================================

/// Generate setup instructions for Pocket TTS
pub fn print_setup_instructions() {
    println!(
        r#"
╔══════════════════════════════════════════════════════════════════╗
║                   POCKET TTS SETUP                               ║
║           Lightweight CPU-Based Text-to-Speech                   ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  Installation                                                    ║
║  ────────────                                                    ║
║  pip install pocket-tts                                          ║
║  # or with uv:                                                   ║
║  uv pip install pocket-tts                                       ║
║                                                                  ║
║  Running the Server                                              ║
║  ──────────────────                                              ║
║  pocket-tts serve                # Default port 8000             ║
║  pocket-tts serve --port 8080    # Custom port                   ║
║                                                                  ║
║  # Or without installation (using uvx):                          ║
║  uvx pocket-tts serve                                            ║
║                                                                  ║
║  Available Voices                                                ║
║  ────────────────                                                ║
║  • alba     (Female, default)                                    ║
║  • marius   (Male)                                               ║
║  • javert   (Male)                                               ║
║  • jean     (Male)                                               ║
║  • fantine  (Female)                                             ║
║  • cosette  (Female)                                             ║
║  • eponine  (Female)                                             ║
║  • azelma   (Female)                                             ║
║                                                                  ║
║  Voice Cloning                                                   ║
║  ─────────────                                                   ║
║  Provide a WAV file or HuggingFace URL:                          ║
║  hf://kyutai/tts-voices/alba-mackenna/casual.wav                 ║
║                                                                  ║
║  Key Features                                                    ║
║  ────────────                                                    ║
║  • ~200ms latency to first audio chunk                           ║
║  • ~6x real-time synthesis speed                                 ║
║  • 100M parameters (small model)                                 ║
║  • CPU-only (no GPU required)                                    ║
║  • Uses only 2 CPU cores                                         ║
║  • English only (for now)                                        ║
║                                                                  ║
║  Reference: https://github.com/kyutai-labs/pocket-tts            ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_defaults() {
        let voice = PocketTTSVoice::default();
        assert_eq!(voice.id, "alba");
        assert_eq!(voice.sample_rate, 24000);
        assert!(!voice.is_custom);
    }

    #[test]
    fn test_all_builtin_voices() {
        let voices = PocketTTSVoice::all_builtin();
        assert_eq!(voices.len(), 8);
        
        let voice_ids: Vec<&str> = voices.iter().map(|v| v.id.as_str()).collect();
        assert!(voice_ids.contains(&"alba"));
        assert!(voice_ids.contains(&"marius"));
        assert!(voice_ids.contains(&"javert"));
        assert!(voice_ids.contains(&"jean"));
        assert!(voice_ids.contains(&"fantine"));
        assert!(voice_ids.contains(&"cosette"));
        assert!(voice_ids.contains(&"eponine"));
        assert!(voice_ids.contains(&"azelma"));
    }

    #[test]
    fn test_custom_voice() {
        let voice = PocketTTSVoice::custom(
            "hf://kyutai/tts-voices/alba-mackenna/casual.wav",
            "Custom Alba",
        );
        assert!(voice.is_custom);
        assert_eq!(voice.name, "Custom Alba");
    }

    #[test]
    fn test_engine_creation() {
        let engine = PocketTTSEngine::new("http://localhost:8000")
            .with_voice(PocketTTSVoice::marius());

        assert_eq!(engine.voice.id, "marius");
        assert_eq!(engine.endpoint, "http://localhost:8000");
    }

    #[test]
    fn test_engine_with_voice_name() {
        let engine = PocketTTSEngine::localhost()
            .with_voice_name("jean");

        assert_eq!(engine.voice.id, "jean");
    }

    #[test]
    fn test_engine_with_voice_prompt() {
        let engine = PocketTTSEngine::localhost()
            .with_voice_prompt("/path/to/voice.wav");

        assert!(engine.voice.is_custom);
        assert_eq!(engine.voice.id, "/path/to/voice.wav");
    }

    #[test]
    fn test_pcm_to_samples() {
        let pcm = vec![0x00, 0x00, 0xFF, 0x7F, 0x00, 0x80];
        let samples = PocketTTSEngine::pcm_to_samples(&pcm);
        
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0], 0);       // 0x0000
        assert_eq!(samples[1], 32767);   // 0x7FFF
        assert_eq!(samples[2], -32768);  // 0x8000
    }
}
