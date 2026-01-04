//! Piper TTS Engine - Ultra Low-Latency Local Text-to-Speech
//!
//! Piper is a fast, local neural TTS system that achieves ~50ms latency.
//! Perfect for real-time voice assistants.
//!
//! ## Quick Setup
//!
//! ### Option 1: Docker (Recommended)
//! ```bash
//! docker run -d --name piper \
//!   -p 5500:5500 \
//!   -v piper-voices:/voices \
//!   rhasspy/wyoming-piper \
//!   --voice en_US-lessac-medium
//! ```
//!
//! ### Option 2: Python
//! ```bash
//! pip install piper-tts
//! # Use as library in Python, or run HTTP server
//! ```
//!
//! ### Option 3: Native Binary
//! ```bash
//! # Download from https://github.com/rhasspy/piper/releases
//! ./piper --model en_US-lessac-medium.onnx --output_raw | aplay -r 22050 -f S16_LE
//! ```
//!
//! ## Voices
//!
//! Piper has many voices available. Popular ones:
//! - `en_US-lessac-medium` - Clear American female (recommended)
//! - `en_US-amy-low` - Fast American female  
//! - `en_US-ryan-high` - American male
//! - `en_GB-alba-medium` - British female
//!
//! Browse all: https://rhasspy.github.io/piper-samples/

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::types::*;

/// Default Piper server port (Wyoming protocol)
const DEFAULT_PIPER_PORT: u16 = 5500;

/// Default HTTP API port (if using piper-http)
const DEFAULT_HTTP_PORT: u16 = 5000;

/// Piper voice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiperVoice {
    /// Voice identifier (e.g., "en_US-lessac-medium")
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Language code
    pub language: String,
    /// Quality level
    pub quality: PiperQuality,
    /// Sample rate (usually 22050)
    pub sample_rate: u32,
}

/// Piper voice quality levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiperQuality {
    /// Fastest, lowest quality
    Low,
    /// Good balance of speed and quality
    Medium,
    /// Best quality, slightly slower
    High,
}

impl PiperQuality {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl Default for PiperVoice {
    fn default() -> Self {
        Self::lessac_medium()
    }
}

impl PiperVoice {
    /// Lessac medium - clear American female (recommended default)
    pub fn lessac_medium() -> Self {
        Self {
            id: "en_US-lessac-medium".to_string(),
            name: "Lessac".to_string(),
            language: "en-US".to_string(),
            quality: PiperQuality::Medium,
            sample_rate: 22050,
        }
    }

    /// Amy low - fast American female
    pub fn amy_low() -> Self {
        Self {
            id: "en_US-amy-low".to_string(),
            name: "Amy".to_string(),
            language: "en-US".to_string(),
            quality: PiperQuality::Low,
            sample_rate: 22050,
        }
    }

    /// Ryan high - American male
    pub fn ryan_high() -> Self {
        Self {
            id: "en_US-ryan-high".to_string(),
            name: "Ryan".to_string(),
            language: "en-US".to_string(),
            quality: PiperQuality::High,
            sample_rate: 22050,
        }
    }

    /// Alba medium - British female
    pub fn alba_medium() -> Self {
        Self {
            id: "en_GB-alba-medium".to_string(),
            name: "Alba".to_string(),
            language: "en-GB".to_string(),
            quality: PiperQuality::Medium,
            sample_rate: 22050,
        }
    }

    /// Create custom voice
    pub fn custom(id: &str, name: &str, language: &str, quality: PiperQuality) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            language: language.to_string(),
            quality,
            sample_rate: 22050,
        }
    }
}

/// Piper TTS Engine
///
/// Connects to a local Piper server for ultra-low-latency speech synthesis.
///
/// ## Example
/// ```rust,ignore
/// let piper = PiperEngine::new("http://localhost:5500");
/// let audio = piper.synthesize("Hello world!").await?;
/// play_audio(audio);
/// ```
pub struct PiperEngine {
    /// HTTP client (reused for connection pooling)
    client: Client,
    /// Piper server endpoint
    endpoint: String,
    /// Active voice
    voice: PiperVoice,
    /// Speaking rate (0.5 to 2.0, default 1.0)
    rate: f32,
    /// Use streaming synthesis
    streaming: bool,
}

impl PiperEngine {
    /// Create new Piper engine with default settings
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .pool_max_idle_per_host(2)
                .build()
                .expect("Failed to create HTTP client"),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            voice: PiperVoice::default(),
            rate: 1.0,
            streaming: false,
        }
    }

    /// Create with specific voice
    pub fn with_voice(mut self, voice: PiperVoice) -> Self {
        self.voice = voice;
        self
    }

    /// Set speaking rate (0.5 to 2.0)
    pub fn with_rate(mut self, rate: f32) -> Self {
        self.rate = rate.clamp(0.5, 2.0);
        self
    }

    /// Enable streaming synthesis
    pub fn with_streaming(mut self, enabled: bool) -> Self {
        self.streaming = enabled;
        self
    }

    /// Create with localhost and default port
    pub fn localhost() -> Self {
        Self::new(&format!("http://127.0.0.1:{}", DEFAULT_HTTP_PORT))
    }

    /// Get current voice
    pub fn voice(&self) -> &PiperVoice {
        &self.voice
    }

    /// Health check - verify Piper server is reachable
    pub async fn health_check(&self) -> bool {
        // Try different endpoints that Piper servers might expose
        let endpoints = [
            format!("{}/", self.endpoint),
            format!("{}/health", self.endpoint),
            format!("{}/api/voices", self.endpoint),
        ];

        for url in &endpoints {
            if let Ok(resp) = self.client.get(url).send().await {
                if resp.status().is_success() || resp.status().as_u16() == 404 {
                    // Server is responding
                    return true;
                }
            }
        }

        false
    }

    /// Synthesize text to raw PCM audio
    ///
    /// Returns 16-bit signed PCM at the voice's sample rate (usually 22050Hz)
    pub async fn synthesize_pcm(&self, text: &str) -> Result<Vec<i16>, VoiceError> {
        let audio_bytes = self.synthesize_raw(text).await?;

        // Convert bytes to i16 samples
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

        Ok(samples)
    }

    /// Synthesize text to raw audio bytes (16-bit PCM)
    pub async fn synthesize_raw(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        // Try different API formats that Piper servers use

        // Format 1: POST with JSON body (piper-http style)
        let result = self
            .client
            .post(&format!("{}/api/tts", self.endpoint))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "text": text,
                "voice": self.voice.id,
                "rate": self.rate,
                "output_type": "raw"
            }))
            .send()
            .await;

        if let Ok(resp) = result {
            if resp.status().is_success() {
                return Ok(resp
                    .bytes()
                    .await
                    .map_err(|e| VoiceError::NetworkError(e.to_string()))?
                    .to_vec());
            }
        }

        // Format 2: GET with query params (simpler servers)
        let result = self
            .client
            .get(&format!("{}/api/tts", self.endpoint))
            .query(&[
                ("text", text),
                ("voice", &self.voice.id),
            ])
            .send()
            .await;

        if let Ok(resp) = result {
            if resp.status().is_success() {
                return Ok(resp
                    .bytes()
                    .await
                    .map_err(|e| VoiceError::NetworkError(e.to_string()))?
                    .to_vec());
            }
        }

        // Format 3: POST with plain text body
        let result = self
            .client
            .post(&format!("{}/synthesize", self.endpoint))
            .header("Content-Type", "text/plain")
            .body(text.to_string())
            .send()
            .await;

        if let Ok(resp) = result {
            if resp.status().is_success() {
                return Ok(resp
                    .bytes()
                    .await
                    .map_err(|e| VoiceError::NetworkError(e.to_string()))?
                    .to_vec());
            }
        }

        Err(VoiceError::NetworkError(
            "Failed to connect to Piper server. Is it running?".to_string(),
        ))
    }

    /// Synthesize text to WAV audio
    pub async fn synthesize_wav(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        let pcm = self.synthesize_raw(text).await?;

        // Create WAV header
        let mut wav = Vec::with_capacity(44 + pcm.len());

        // RIFF header
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&((36 + pcm.len()) as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVE");

        // fmt chunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&self.voice.sample_rate.to_le_bytes()); // sample rate
        wav.extend_from_slice(&(self.voice.sample_rate * 2).to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        // data chunk
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        wav.extend_from_slice(&pcm);

        Ok(wav)
    }

    /// Convert 22050Hz mono to 48000Hz stereo (for Discord)
    pub fn resample_for_discord(samples_22k: &[i16]) -> Vec<i16> {
        // Upsample from 22050 to 48000 (ratio ~2.177)
        // Using linear interpolation for simplicity
        let ratio = 48000.0 / 22050.0;
        let output_len = (samples_22k.len() as f64 * ratio) as usize;
        let mut output = Vec::with_capacity(output_len * 2); // stereo

        for i in 0..output_len {
            let src_pos = i as f64 / ratio;
            let src_idx = src_pos as usize;
            let frac = src_pos - src_idx as f64;

            let sample = if src_idx + 1 < samples_22k.len() {
                let s0 = samples_22k[src_idx] as f64;
                let s1 = samples_22k[src_idx + 1] as f64;
                (s0 + frac * (s1 - s0)) as i16
            } else if src_idx < samples_22k.len() {
                samples_22k[src_idx]
            } else {
                0
            };

            // Duplicate for stereo
            output.push(sample);
            output.push(sample);
        }

        output
    }
}

// Implement VoiceEngine trait for integration with VoicePlugin
#[async_trait]
impl VoiceEngine for PiperEngine {
    fn name(&self) -> &str {
        "piper"
    }

    async fn synthesize(&self, text: &str, config: &VoiceConfig) -> zoey_core::Result<AudioData> {
        let pcm = self.synthesize_raw(text).await?;

        Ok(AudioData {
            data: Bytes::from(pcm),
            format: AudioFormat::Pcm,
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
        // Piper doesn't support true streaming, so synthesize and emit chunks
        let (tx, rx) = create_audio_stream(32);

        let audio = self.synthesize(text, config).await;

        tokio::spawn(async move {
            match audio {
                Ok(data) => {
                    // Emit in chunks for better streaming behavior
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
        // Return common Piper voices
        Ok(vec![
            Voice::custom(
                "en_US-lessac-medium".to_string(),
                "Lessac".to_string(),
                VoiceGender::Female,
                "en-US".to_string(),
            ),
            Voice::custom(
                "en_US-amy-low".to_string(),
                "Amy".to_string(),
                VoiceGender::Female,
                "en-US".to_string(),
            ),
            Voice::custom(
                "en_US-ryan-high".to_string(),
                "Ryan".to_string(),
                VoiceGender::Male,
                "en-US".to_string(),
            ),
            Voice::custom(
                "en_GB-alba-medium".to_string(),
                "Alba".to_string(),
                VoiceGender::Female,
                "en-GB".to_string(),
            ),
        ])
    }

    async fn is_ready(&self) -> bool {
        self.health_check().await
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Pcm, AudioFormat::Wav]
    }

    fn max_text_length(&self) -> usize {
        10000 // Piper handles long text well
    }
}

// ============================================================================
// Docker Setup Helper
// ============================================================================

/// Generate docker-compose.yml for Piper
pub fn docker_compose_yaml() -> &'static str {
    r#"version: '3.8'
services:
  piper:
    image: rhasspy/wyoming-piper
    container_name: piper-tts
    restart: unless-stopped
    ports:
      - "5500:5500"
    volumes:
      - piper-data:/data
    command: --voice en_US-lessac-medium
    
volumes:
  piper-data:
"#
}

/// Print setup instructions
pub fn print_setup_instructions() {
    println!(
        r#"
╔══════════════════════════════════════════════════════════════════╗
║                    PIPER TTS SETUP                               ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  Option 1: Docker (Recommended)                                  ║
║  ─────────────────────────────────                               ║
║  docker run -d --name piper -p 5500:5500 \                       ║
║    rhasspy/wyoming-piper --voice en_US-lessac-medium             ║
║                                                                  ║
║  Option 2: Docker Compose                                        ║
║  ────────────────────────────                                    ║
║  Save the docker-compose.yml and run:                            ║
║    docker-compose up -d                                          ║
║                                                                  ║
║  Option 3: Native Binary                                         ║
║  ───────────────────────────                                     ║
║  Download from: https://github.com/rhasspy/piper/releases        ║
║  Run: ./piper --model en_US-lessac-medium.onnx                   ║
║                                                                  ║
║  Available Voices:                                               ║
║  • en_US-lessac-medium  (Female, clear, recommended)             ║
║  • en_US-amy-low        (Female, fast)                           ║
║  • en_US-ryan-high      (Male, high quality)                     ║
║  • en_GB-alba-medium    (British female)                         ║
║                                                                  ║
║  Browse all: https://rhasspy.github.io/piper-samples/            ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
"#
    );
}

// ============================================================================
// Embedded Piper Server (runs alongside Zoey)
// ============================================================================

/// Configuration for embedded Piper server
#[derive(Debug, Clone)]
pub struct EmbeddedPiperConfig {
    /// Port to listen on
    pub port: u16,
    /// Host to bind to
    pub host: String,
    /// Path to Piper binary
    pub piper_path: std::path::PathBuf,
    /// Path to voice model
    pub model_path: std::path::PathBuf,
}

impl Default for EmbeddedPiperConfig {
    fn default() -> Self {
        Self {
            port: 5500,
            host: "127.0.0.1".to_string(),
            piper_path: std::path::PathBuf::from("voices/piper/piper"),
            model_path: std::path::PathBuf::from("voices/models/en_US-amy-low.onnx"),
        }
    }
}

/// Run Piper directly without HTTP server (for embedding)
/// 
/// Returns raw PCM audio (16-bit, 22050Hz, mono)
pub async fn synthesize_with_piper(
    piper_path: &std::path::Path,
    model_path: &std::path::Path,
    text: &str,
) -> Result<Vec<u8>, VoiceError> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    // Find library path
    let lib_path = piper_path.parent().unwrap_or(piper_path);
    let ld_path = format!(
        "{}:{}",
        lib_path.display(),
        std::env::var("LD_LIBRARY_PATH").unwrap_or_default()
    );

    let mut cmd = Command::new(piper_path);
    cmd.arg("--model")
        .arg(model_path)
        .arg("--output-raw")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LD_LIBRARY_PATH", ld_path);

    let mut child = cmd
        .spawn()
        .map_err(|e| VoiceError::NotReady(format!("Failed to start Piper: {}", e)))?;

    // Write text to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| VoiceError::AudioError(format!("Failed to write to Piper: {}", e)))?;
    }

    // Wait for output
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| VoiceError::AudioError(format!("Piper failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VoiceError::AudioError(format!("Piper error: {}", stderr)));
    }

    Ok(output.stdout)
}

/// Convert raw PCM to WAV format
pub fn pcm_to_wav(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = pcm.len() as u32;

    let mut wav = Vec::with_capacity(44 + pcm.len());

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm);

    wav
}

/// Local Piper engine that calls the binary directly (no HTTP)
/// 
/// This is the fastest option - no network overhead at all.
/// 
/// ## Example
/// ```rust,ignore
/// let piper = LocalPiperEngine::new(
///     "voices/piper/piper",
///     "voices/models/en_US-amy-low.onnx",
/// );
/// 
/// let audio = piper.synthesize("Hello!").await?;
/// ```
pub struct LocalPiperEngine {
    piper_path: std::path::PathBuf,
    model_path: std::path::PathBuf,
    voice: PiperVoice,
}

impl LocalPiperEngine {
    /// Create new local Piper engine
    pub fn new(piper_path: impl Into<std::path::PathBuf>, model_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            piper_path: piper_path.into(),
            model_path: model_path.into(),
            voice: PiperVoice::default(),
        }
    }

    /// Create with auto-detected paths (looks in standard locations)
    pub fn auto() -> Option<Self> {
        let piper_candidates = [
            std::path::PathBuf::from("voices/piper/piper"),
            std::path::PathBuf::from("crates/providers/zoey-provider-voice/voices/piper/piper"),
        ];
        
        let model_candidates = [
            std::path::PathBuf::from("voices/models/en_US-amy-low.onnx"),
            std::path::PathBuf::from("crates/providers/zoey-provider-voice/voices/models/en_US-amy-low.onnx"),
        ];

        let piper_path = piper_candidates.into_iter().find(|p| p.exists())?;
        let model_path = model_candidates.into_iter().find(|p| p.exists())?;

        Some(Self::new(piper_path, model_path))
    }

    /// Set voice info
    pub fn with_voice(mut self, voice: PiperVoice) -> Self {
        self.voice = voice;
        self
    }

    /// Check if piper binary exists
    pub fn is_available(&self) -> bool {
        self.piper_path.exists() && self.model_path.exists()
    }

    /// Synthesize text to raw PCM
    pub async fn synthesize_pcm(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        synthesize_with_piper(&self.piper_path, &self.model_path, text).await
    }

    /// Synthesize text to WAV
    pub async fn synthesize_wav(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        let pcm = self.synthesize_pcm(text).await?;
        Ok(pcm_to_wav(&pcm, self.voice.sample_rate))
    }
}

#[async_trait]
impl VoiceEngine for LocalPiperEngine {
    fn name(&self) -> &str {
        "piper-local"
    }

    async fn synthesize(&self, text: &str, _config: &VoiceConfig) -> zoey_core::Result<AudioData> {
        let pcm = self.synthesize_pcm(text).await?;

        Ok(AudioData {
            data: Bytes::from(pcm),
            format: AudioFormat::Pcm,
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
        let (tx, rx) = create_audio_stream(32);
        let audio = self.synthesize(text, config).await;

        tokio::spawn(async move {
            match audio {
                Ok(data) => {
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
        Ok(vec![Voice::custom(
            self.voice.id.clone(),
            self.voice.name.clone(),
            VoiceGender::Female,
            self.voice.language.clone(),
        )])
    }

    async fn is_ready(&self) -> bool {
        self.is_available()
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Pcm, AudioFormat::Wav]
    }

    fn max_text_length(&self) -> usize {
        10000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_defaults() {
        let voice = PiperVoice::default();
        assert_eq!(voice.id, "en_US-lessac-medium");
        assert_eq!(voice.sample_rate, 22050);
    }

    #[test]
    fn test_engine_creation() {
        let engine = PiperEngine::new("http://localhost:5500")
            .with_voice(PiperVoice::amy_low())
            .with_rate(1.2);

        assert_eq!(engine.voice.id, "en_US-amy-low");
        assert_eq!(engine.rate, 1.2);
    }

    #[test]
    fn test_resample_discord() {
        // Simple test with known input
        let input = vec![0i16, 1000, 2000, 3000];
        let output = PiperEngine::resample_for_discord(&input);

        // Should be stereo and upsampled
        assert!(output.len() > input.len() * 2);
        // Every other sample should be the same (stereo)
        for i in (0..output.len()).step_by(2) {
            if i + 1 < output.len() {
                assert_eq!(output[i], output[i + 1], "Stereo samples should match");
            }
        }
    }

    #[test]
    fn test_docker_compose() {
        let yaml = docker_compose_yaml();
        assert!(yaml.contains("rhasspy/wyoming-piper"));
        assert!(yaml.contains("en_US-lessac-medium"));
    }

    #[test]
    fn test_pcm_to_wav() {
        let pcm = vec![0u8; 100];
        let wav = pcm_to_wav(&pcm, 22050);
        
        // WAV header is 44 bytes
        assert_eq!(wav.len(), 44 + 100);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }
}

