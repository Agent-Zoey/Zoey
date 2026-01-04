//! Supertonic TTS Engine - Ultra Fast On-Device Text-to-Speech
//!
//! Supertonic is a lightning-fast, on-device TTS system optimized for minimal
//! computational overhead. It runs entirely locally via ONNX, ensuring privacy
//! and eliminating cloud latency.
//!
//! ## Key Features
//! - **Ultra Low Latency**: ~10-50ms inference time
//! - **On-Device**: No cloud calls, full privacy
//! - **Cross-Platform**: Works via ONNX Runtime
//! - **High Quality**: Neural TTS with natural prosody
//!
//! ## Setup Options
//!
//! ### Option 1: HTTP Server (Recommended for integration)
//! ```bash
//! # Clone Supertonic
//! git clone https://github.com/supertone-inc/supertonic.git
//! cd supertonic
//!
//! # Download models (requires git-lfs)
//! git clone https://huggingface.co/Supertone/supertonic assets
//!
//! # Run Python server
//! cd py
//! pip install -r requirements.txt
//! python server.py --port 8080
//! ```
//!
//! ### Option 2: Rust Native (via ONNX Runtime)
//! Requires `ort` crate and downloaded ONNX models.
//!
//! ## Voices
//! Supertonic supports multiple preset voices. Check the assets repository
//! for available voice presets: https://huggingface.co/Supertone/supertonic
//!
//! Reference: https://github.com/supertone-inc/supertonic

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::types::*;

/// Default Supertonic HTTP server port
const DEFAULT_SUPERTONIC_PORT: u16 = 5080;

/// Default sample rate for Supertonic output (44.1kHz as per model config)
const DEFAULT_SAMPLE_RATE: u32 = 44100;

/// Supertonic voice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupertonicVoice {
    /// Voice identifier/preset name
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Language code
    pub language: String,
    /// Sample rate (usually 24000)
    pub sample_rate: u32,
    /// Voice description
    pub description: Option<String>,
}

impl Default for SupertonicVoice {
    fn default() -> Self {
        Self::default_voice()
    }
}

impl SupertonicVoice {
    /// Default voice preset (Female Voice 1)
    pub fn default_voice() -> Self {
        Self {
            id: "F1".to_string(),
            name: "Female Voice 1".to_string(),
            language: "en-US".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            description: Some("Default Supertonic female voice".to_string()),
        }
    }

    /// Female voice preset
    pub fn female(num: u8) -> Self {
        let num = num.clamp(1, 5);
        Self {
            id: format!("F{}", num),
            name: format!("Female Voice {}", num),
            language: "en-US".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            description: Some(format!("Supertonic female voice {}", num)),
        }
    }

    /// Male voice preset
    pub fn male(num: u8) -> Self {
        let num = num.clamp(1, 5);
        Self {
            id: format!("M{}", num),
            name: format!("Male Voice {}", num),
            language: "en-US".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            description: Some(format!("Supertonic male voice {}", num)),
        }
    }

    /// Create custom voice preset
    pub fn custom(id: &str, name: &str, language: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            language: language.to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            description: None,
        }
    }

    /// Create voice with specific sample rate
    pub fn with_sample_rate(mut self, sample_rate: u32) -> Self {
        self.sample_rate = sample_rate;
        self
    }
}

/// Supertonic synthesis parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupertonicParams {
    /// Speaking speed multiplier (0.5 to 2.0, default 1.0)
    pub speed: f32,
    /// Pitch adjustment (-1.0 to 1.0, default 0.0)
    pub pitch: f32,
    /// Energy/volume adjustment (0.5 to 2.0, default 1.0)
    pub energy: f32,
}

impl Default for SupertonicParams {
    fn default() -> Self {
        Self {
            speed: 1.0,
            pitch: 0.0,
            energy: 1.0,
        }
    }
}

/// Supertonic TTS Engine
///
/// Connects to a Supertonic server for ultra-fast local speech synthesis.
///
/// ## Example
/// ```rust,ignore
/// let engine = SupertonicEngine::new("http://localhost:8080");
/// let audio = engine.synthesize("Hello world!", &config).await?;
/// ```
pub struct SupertonicEngine {
    /// HTTP client (reused for connection pooling)
    client: Client,
    /// Server endpoint URL
    endpoint: String,
    /// Active voice preset
    voice: SupertonicVoice,
    /// Synthesis parameters
    params: SupertonicParams,
}

impl SupertonicEngine {
    /// Create new Supertonic engine with default settings
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .pool_max_idle_per_host(4)
                .build()
                .expect("Failed to create HTTP client"),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            voice: SupertonicVoice::default(),
            params: SupertonicParams::default(),
        }
    }

    /// Create with localhost and default port
    pub fn localhost() -> Self {
        Self::new(&format!("http://127.0.0.1:{}", DEFAULT_SUPERTONIC_PORT))
    }

    /// Set voice preset
    pub fn with_voice(mut self, voice: SupertonicVoice) -> Self {
        self.voice = voice;
        self
    }

    /// Set synthesis parameters
    pub fn with_params(mut self, params: SupertonicParams) -> Self {
        self.params = params;
        self
    }

    /// Set speaking speed (0.5 to 2.0)
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.params.speed = speed.clamp(0.5, 2.0);
        self
    }

    /// Set pitch adjustment (-1.0 to 1.0)
    pub fn with_pitch(mut self, pitch: f32) -> Self {
        self.params.pitch = pitch.clamp(-1.0, 1.0);
        self
    }

    /// Get current voice
    pub fn voice(&self) -> &SupertonicVoice {
        &self.voice
    }

    /// Get endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Health check - verify Supertonic server is reachable
    pub async fn health_check(&self) -> bool {
        let endpoints = [
            format!("{}/health", self.endpoint),
            format!("{}/", self.endpoint),
            format!("{}/api/health", self.endpoint),
        ];

        for url in &endpoints {
            match self.client.get(url).send().await {
                Ok(resp) if resp.status().is_success() => return true,
                Ok(resp) if resp.status().as_u16() == 404 => {
                    // Server responds but endpoint not found - still alive
                    return true;
                }
                _ => continue,
            }
        }

        // Also try a minimal synthesis request
        match self.synthesize_raw("test").await {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Synthesize text to raw PCM audio bytes
    ///
    /// Returns 16-bit signed PCM at the voice's sample rate (usually 24kHz)
    pub async fn synthesize_raw(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }

        // Try multiple API formats for compatibility
        
        // Format 1: POST with JSON body (standard REST API)
        let result = self
            .client
            .post(&format!("{}/api/tts", self.endpoint))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "text": text,
                "voice": self.voice.id,
                "speed": self.params.speed,
                "pitch": self.params.pitch,
                "energy": self.params.energy,
                "format": "pcm"
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

        // Format 2: POST to /synthesize endpoint
        let result = self
            .client
            .post(&format!("{}/synthesize", self.endpoint))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "text": text,
                "voice_id": self.voice.id,
                "speed": self.params.speed,
                "output_format": "raw"
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

        // Format 3: GET with query params (simple servers)
        let result = self
            .client
            .get(&format!("{}/tts", self.endpoint))
            .query(&[
                ("text", text),
                ("voice", &self.voice.id),
                ("format", "pcm"),
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

        // Format 4: POST with plain text body
        let result = self
            .client
            .post(&format!("{}/speak", self.endpoint))
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
            "Failed to connect to Supertonic server. Is it running?".to_string(),
        ))
    }

    /// Synthesize text to PCM samples (16-bit signed)
    pub async fn synthesize_pcm(&self, text: &str) -> Result<Vec<i16>, VoiceError> {
        let bytes = self.synthesize_raw(text).await?;

        let samples: Vec<i16> = bytes
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

    /// Synthesize text to WAV audio
    pub async fn synthesize_wav(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        let pcm = self.synthesize_raw(text).await?;
        Ok(pcm_to_wav(&pcm, self.voice.sample_rate))
    }

    /// Get available voices from server
    pub async fn list_voices(&self) -> Result<Vec<SupertonicVoice>, VoiceError> {
        let endpoints = [
            format!("{}/api/voices", self.endpoint),
            format!("{}/voices", self.endpoint),
        ];

        for url in &endpoints {
            if let Ok(resp) = self.client.get(url).send().await {
                if resp.status().is_success() {
                    if let Ok(voices) = resp.json::<Vec<SupertonicVoice>>().await {
                        return Ok(voices);
                    }
                }
            }
        }

        // Return default voice if server doesn't support listing
        Ok(vec![SupertonicVoice::default()])
    }
}

// Implement VoiceEngine trait for integration with VoicePlugin
#[async_trait]
impl VoiceEngine for SupertonicEngine {
    fn name(&self) -> &str {
        "supertonic"
    }

    async fn synthesize(&self, text: &str, _config: &VoiceConfig) -> zoey_core::Result<AudioData> {
        let start = std::time::Instant::now();
        let pcm = self.synthesize_raw(text).await?;
        let elapsed = start.elapsed();

        debug!(
            text_len = text.len(),
            audio_bytes = pcm.len(),
            latency_ms = elapsed.as_millis(),
            "Supertonic synthesis complete"
        );

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
        // Supertonic is fast enough that we synthesize fully then chunk
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
        let supertonic_voices = self.list_voices().await.unwrap_or_else(|_| vec![SupertonicVoice::default()]);

        Ok(supertonic_voices
            .into_iter()
            .map(|v| Voice::custom(v.id, v.name, VoiceGender::Neutral, v.language))
            .collect())
    }

    async fn is_ready(&self) -> bool {
        self.health_check().await
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Pcm, AudioFormat::Wav]
    }

    fn max_text_length(&self) -> usize {
        10000 // Supertonic handles long text well
    }
}

// ============================================================================
// Local Supertonic Engine (Direct ONNX Inference)
// ============================================================================

/// Configuration for local Supertonic inference
#[derive(Debug, Clone)]
pub struct LocalSupertonicConfig {
    /// Path to ONNX models directory (contains vocoder.onnx, text_encoder.onnx, etc.)
    pub models_dir: PathBuf,
    /// Path to voice styles directory (contains F1.json, M1.json, etc.)
    pub voice_styles_dir: PathBuf,
    /// Selected voice style (e.g., "F1", "M1")
    pub voice_style: String,
    /// Sample rate for output (44100 Hz)
    pub sample_rate: u32,
}

impl Default for LocalSupertonicConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from(".zoey/voice/supertonic/onnx"),
            voice_styles_dir: PathBuf::from(".zoey/voice/supertonic/voice_styles"),
            voice_style: "F1".to_string(), // Female voice 1 (default)
            sample_rate: DEFAULT_SAMPLE_RATE,
        }
    }
}

/// Available Supertonic voice presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupertonicPreset {
    /// Female voice 1
    F1,
    /// Female voice 2
    F2,
    /// Female voice 3
    F3,
    /// Female voice 4
    F4,
    /// Female voice 5
    F5,
    /// Male voice 1
    M1,
    /// Male voice 2
    M2,
    /// Male voice 3
    M3,
    /// Male voice 4
    M4,
    /// Male voice 5
    M5,
}

impl SupertonicPreset {
    /// Get preset name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::F1 => "F1",
            Self::F2 => "F2",
            Self::F3 => "F3",
            Self::F4 => "F4",
            Self::F5 => "F5",
            Self::M1 => "M1",
            Self::M2 => "M2",
            Self::M3 => "M3",
            Self::M4 => "M4",
            Self::M5 => "M5",
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::F1 => "Female Voice 1",
            Self::F2 => "Female Voice 2",
            Self::F3 => "Female Voice 3",
            Self::F4 => "Female Voice 4",
            Self::F5 => "Female Voice 5",
            Self::M1 => "Male Voice 1",
            Self::M2 => "Male Voice 2",
            Self::M3 => "Male Voice 3",
            Self::M4 => "Male Voice 4",
            Self::M5 => "Male Voice 5",
        }
    }

    /// Get all available presets
    pub fn all() -> &'static [SupertonicPreset] {
        &[
            Self::F1, Self::F2, Self::F3, Self::F4, Self::F5,
            Self::M1, Self::M2, Self::M3, Self::M4, Self::M5,
        ]
    }
}

impl Default for SupertonicPreset {
    fn default() -> Self {
        Self::F1
    }
}

/// Local Supertonic engine for direct ONNX inference
///
/// This is the fastest option with zero network overhead.
/// Requires ONNX models to be downloaded locally.
///
/// ## Models Location
/// Models should be in `.zoey/voice/supertonic/`:
/// - `onnx/` - ONNX model files (vocoder.onnx, text_encoder.onnx, etc.)
/// - `voice_styles/` - Voice presets (F1.json through F5.json, M1.json through M5.json)
///
/// ## Example
/// ```rust,ignore
/// let config = LocalSupertonicConfig::default();
/// let engine = LocalSupertonicEngine::new(config)?;
/// let audio = engine.synthesize("Hello!").await?;
/// ```
pub struct LocalSupertonicEngine {
    config: LocalSupertonicConfig,
    voice: SupertonicVoice,
}

impl LocalSupertonicEngine {
    /// Create new local engine with config
    pub fn new(config: LocalSupertonicConfig) -> Result<Self, VoiceError> {
        let vocoder_path = config.models_dir.join("vocoder.onnx");
        if !vocoder_path.exists() {
            return Err(VoiceError::ModelError(format!(
                "Supertonic models not found at {}. Download with:\n\
                 cd .zoey/voice && git clone https://huggingface.co/Supertone/supertonic supertonic",
                config.models_dir.display()
            )));
        }

        let voice_style_path = config.voice_styles_dir.join(format!("{}.json", config.voice_style));
        if !voice_style_path.exists() {
            warn!(
                voice_style = %config.voice_style,
                "Voice style not found, will use default embedding"
            );
        }

        let voice = SupertonicVoice {
            id: config.voice_style.clone(),
            name: format!("Supertonic {}", config.voice_style),
            language: "en-US".to_string(),
            sample_rate: config.sample_rate,
            description: Some(format!("Supertonic preset voice {}", config.voice_style)),
        };

        Ok(Self { config, voice })
    }

    /// Create with auto-detected paths (looks in standard locations)
    pub fn auto() -> Option<Self> {
        let model_candidates = [
            PathBuf::from(".zoey/voice/supertonic/onnx"),
            PathBuf::from("assets/supertonic/onnx"),
            PathBuf::from("supertonic/onnx"),
        ];

        let voice_candidates = [
            PathBuf::from(".zoey/voice/supertonic/voice_styles"),
            PathBuf::from("assets/supertonic/voice_styles"),
            PathBuf::from("supertonic/voice_styles"),
        ];

        let models_dir = model_candidates.into_iter().find(|p| p.join("vocoder.onnx").exists())?;
        let voice_styles_dir = voice_candidates.into_iter().find(|p| p.join("F1.json").exists())?;

        info!(
            models_dir = %models_dir.display(),
            voice_styles_dir = %voice_styles_dir.display(),
            "Auto-detected Supertonic models"
        );

        Self::new(LocalSupertonicConfig {
            models_dir,
            voice_styles_dir,
            voice_style: "F1".to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
        }).ok()
    }

    /// Create with specific preset voice
    pub fn with_preset(preset: SupertonicPreset) -> Option<Self> {
        let mut engine = Self::auto()?;
        engine.config.voice_style = preset.as_str().to_string();
        engine.voice.id = preset.as_str().to_string();
        engine.voice.name = preset.display_name().to_string();
        Some(engine)
    }

    /// Set voice style
    pub fn with_voice_style(mut self, style: &str) -> Self {
        self.config.voice_style = style.to_string();
        self.voice.id = style.to_string();
        self
    }

    /// Set voice
    pub fn with_voice(mut self, voice: SupertonicVoice) -> Self {
        self.voice = voice;
        self
    }

    /// Check if model files exist
    pub fn is_available(&self) -> bool {
        self.config.models_dir.join("vocoder.onnx").exists()
            && self.config.models_dir.join("text_encoder.onnx").exists()
            && self.config.models_dir.join("vector_estimator.onnx").exists()
            && self.config.models_dir.join("duration_predictor.onnx").exists()
    }

    /// Get path to a specific ONNX model
    pub fn model_path(&self, name: &str) -> PathBuf {
        self.config.models_dir.join(name)
    }

    /// Get path to voice style JSON
    pub fn voice_style_path(&self) -> PathBuf {
        self.config.voice_styles_dir.join(format!("{}.json", self.config.voice_style))
    }

    /// List available voice styles
    pub fn available_styles(&self) -> Vec<String> {
        if let Ok(entries) = std::fs::read_dir(&self.config.voice_styles_dir) {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let path = e.path();
                    if path.extension().map(|e| e == "json").unwrap_or(false) {
                        path.file_stem()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            vec!["F1".to_string()] // Default fallback
        }
    }

    /// Synthesize text to PCM audio
    /// 
    /// Note: Full ONNX inference requires the `ort` crate.
    /// This is a placeholder that returns an error if models aren't set up.
    pub async fn synthesize_pcm(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        if !self.is_available() {
            return Err(VoiceError::NotReady(format!(
                "Supertonic models not found at {}. Download with:\n\
                 cd .zoey/voice && git clone https://huggingface.co/Supertone/supertonic supertonic",
                self.config.models_dir.display()
            )));
        }

        // TODO: Implement ONNX inference using ort crate
        // Required models:
        // - text_encoder.onnx: Text -> embeddings
        // - duration_predictor.onnx: Predict phoneme durations
        // - vector_estimator.onnx: Generate mel spectrogram
        // - vocoder.onnx: Mel spectrogram -> waveform
        //
        // Voice style embedding from: voice_styles/{style}.json
        
        Err(VoiceError::NotReady(
            "Local ONNX inference not yet implemented. Use SupertonicEngine with HTTP server, or run:\n\
             cd supertonic/py && python server.py --port 8080".to_string()
        ))
    }

    /// Synthesize to WAV
    pub async fn synthesize_wav(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        let pcm = self.synthesize_pcm(text).await?;
        Ok(pcm_to_wav(&pcm, self.config.sample_rate))
    }
}

#[async_trait]
impl VoiceEngine for LocalSupertonicEngine {
    fn name(&self) -> &str {
        "supertonic-local"
    }

    async fn synthesize(&self, text: &str, _config: &VoiceConfig) -> zoey_core::Result<AudioData> {
        let pcm = self.synthesize_pcm(text).await?;

        Ok(AudioData {
            data: Bytes::from(pcm),
            format: AudioFormat::Pcm,
            sample_rate: self.config.sample_rate,
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
        let styles = self.available_styles();
        Ok(styles
            .into_iter()
            .map(|style| {
                let gender = if style.starts_with('F') {
                    VoiceGender::Female
                } else if style.starts_with('M') {
                    VoiceGender::Male
                } else {
                    VoiceGender::Neutral
                };
                Voice::custom(
                    style.clone(),
                    format!("Supertonic {}", style),
                    gender,
                    "en-US".to_string(),
                )
            })
            .collect())
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

// ============================================================================
// Helper Functions
// ============================================================================

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

/// Print setup instructions for Supertonic
pub fn print_setup_instructions() {
    println!(
        r#"
╔══════════════════════════════════════════════════════════════════╗
║                   SUPERTONIC TTS SETUP                           ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  Supertonic is an ultra-fast, on-device TTS engine.              ║
║                                                                  ║
║  Option 1: Python Server (Recommended)                           ║
║  ─────────────────────────────────────                           ║
║  git clone https://github.com/supertone-inc/supertonic.git       ║
║  cd supertonic                                                   ║
║  git clone https://huggingface.co/Supertone/supertonic assets    ║
║  cd py && pip install -r requirements.txt                        ║
║  python server.py --port 8080                                    ║
║                                                                  ║
║  Option 2: Docker                                                ║
║  ──────────────────                                              ║
║  docker run -d --name supertonic -p 8080:8080 \                  ║
║    supertone/supertonic:latest                                   ║
║                                                                  ║
║  Configuration:                                                  ║
║  • Default endpoint: http://127.0.0.1:8080                       ║
║  • Sample rate: 24kHz mono PCM                                   ║
║                                                                  ║
║  GitHub: https://github.com/supertone-inc/supertonic             ║
║  Models: https://huggingface.co/Supertone/supertonic             ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
"#
    );
}

/// Generate docker-compose.yml for Supertonic
pub fn docker_compose_yaml() -> &'static str {
    r#"version: '3.8'
services:
  supertonic:
    image: supertone/supertonic:latest
    container_name: supertonic-tts
    restart: unless-stopped
    ports:
      - "8080:8080"
    volumes:
      - supertonic-models:/app/assets
    environment:
      - SUPERTONIC_PORT=8080
      - SUPERTONIC_HOST=0.0.0.0

volumes:
  supertonic-models:
"#
}

// ============================================================================
// Supertonic Server Manager
// ============================================================================

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for Supertonic server
#[derive(Debug, Clone)]
pub struct SupertonicServerConfig {
    /// Base directory for supertonic installation
    pub base_dir: PathBuf,
    /// Port to run server on
    pub port: u16,
    /// Host to bind to
    pub host: String,
    /// Path to assets (models)
    pub assets_dir: PathBuf,
    /// Voice style to use
    pub voice_style: String,
}

impl Default for SupertonicServerConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from(".zoey/voice/supertonic-server"),
            port: DEFAULT_SUPERTONIC_PORT,
            host: "127.0.0.1".to_string(),
            assets_dir: PathBuf::from(".zoey/voice/supertonic"),
            voice_style: "F1".to_string(),
        }
    }
}

/// Supertonic server manager - handles setup and running the Python TTS server
///
/// ## Usage
/// ```rust,ignore
/// let server = SupertonicServer::new(SupertonicServerConfig::default());
/// server.setup().await?; // Clone repo and install deps
/// server.start().await?; // Start the Python server
///
/// // Use with SupertonicEngine
/// let engine = SupertonicEngine::localhost();
/// let audio = engine.synthesize("Hello!").await?;
///
/// // Stop when done
/// server.stop().await;
/// ```
pub struct SupertonicServer {
    config: SupertonicServerConfig,
    process: Arc<RwLock<Option<Child>>>,
}

impl SupertonicServer {
    /// Create new server manager
    pub fn new(config: SupertonicServerConfig) -> Self {
        Self {
            config,
            process: Arc::new(RwLock::new(None)),
        }
    }

    /// Create with default config
    pub fn default_config() -> Self {
        Self::new(SupertonicServerConfig::default())
    }

    /// Get the endpoint URL for connecting
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}", self.config.host, self.config.port)
    }

    /// Check if supertonic repo is cloned
    pub fn is_setup(&self) -> bool {
        self.config.base_dir.join("py").exists()
            && self.config.base_dir.join("py/requirements.txt").exists()
    }

    /// Check if assets/models exist
    pub fn has_models(&self) -> bool {
        self.config.assets_dir.join("onnx/vocoder.onnx").exists()
    }

    /// Setup supertonic - clone repo if needed
    pub async fn setup(&self) -> Result<(), VoiceError> {
        // Check if models exist
        if !self.has_models() {
            return Err(VoiceError::ModelError(format!(
                "Supertonic models not found at {}. Download with:\n\
                 cd .zoey/voice && git clone https://huggingface.co/Supertone/supertonic supertonic",
                self.config.assets_dir.display()
            )));
        }

        // Clone supertonic repo if not present
        if !self.is_setup() {
            info!(
                base_dir = %self.config.base_dir.display(),
                "Cloning supertonic repository..."
            );

            // Create base directory
            std::fs::create_dir_all(&self.config.base_dir)
                .map_err(|e| VoiceError::Other(format!("Failed to create directory: {}", e)))?;

            // Clone the repo
            let output = Command::new("git")
                .args(["clone", "--depth", "1", "https://github.com/supertone-inc/supertonic.git", "."])
                .current_dir(&self.config.base_dir)
                .output()
                .map_err(|e| VoiceError::Other(format!("Failed to clone repo: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(VoiceError::Other(format!("Git clone failed: {}", stderr)));
            }

            info!("Supertonic repository cloned successfully");
        }

        // Create symlink to assets if needed
        let assets_link = self.config.base_dir.join("assets");
        if !assets_link.exists() {
            info!(
                assets_dir = %self.config.assets_dir.display(),
                link = %assets_link.display(),
                "Creating symlink to assets..."
            );

            #[cfg(unix)]
            {
                let abs_assets = std::fs::canonicalize(&self.config.assets_dir)
                    .map_err(|e| VoiceError::Other(format!("Failed to get absolute path: {}", e)))?;
                std::os::unix::fs::symlink(&abs_assets, &assets_link)
                    .map_err(|e| VoiceError::Other(format!("Failed to create symlink: {}", e)))?;
            }

            #[cfg(windows)]
            {
                let abs_assets = std::fs::canonicalize(&self.config.assets_dir)
                    .map_err(|e| VoiceError::Other(format!("Failed to get absolute path: {}", e)))?;
                std::os::windows::fs::symlink_dir(&abs_assets, &assets_link)
                    .map_err(|e| VoiceError::Other(format!("Failed to create symlink: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Install Python dependencies
    pub async fn install_deps(&self) -> Result<(), VoiceError> {
        if !self.is_setup() {
            return Err(VoiceError::NotReady("Supertonic not setup. Call setup() first.".to_string()));
        }

        let py_dir = self.config.base_dir.join("py");
        let requirements = py_dir.join("requirements.txt");

        if !requirements.exists() {
            return Err(VoiceError::NotReady(format!(
                "requirements.txt not found at {}",
                requirements.display()
            )));
        }

        info!("Installing Python dependencies...");

        let output = Command::new("pip")
            .args(["install", "-r", "requirements.txt"])
            .current_dir(&py_dir)
            .output()
            .map_err(|e| VoiceError::Other(format!("Failed to run pip: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(stderr = %stderr, "pip install had warnings/errors");
        }

        info!("Python dependencies installed");
        Ok(())
    }

    /// Start the Supertonic Python server
    pub async fn start(&self) -> Result<(), VoiceError> {
        // Check if already running
        {
            let proc = self.process.read().await;
            if proc.is_some() {
                return Ok(()); // Already running
            }
        }

        if !self.is_setup() {
            return Err(VoiceError::NotReady(
                "Supertonic not setup. Call setup() first.".to_string()
            ));
        }

        let py_dir = self.config.base_dir.join("py");

        // Check for server.py or main entry point
        let server_script = if py_dir.join("server.py").exists() {
            "server.py"
        } else if py_dir.join("app.py").exists() {
            "app.py"
        } else if py_dir.join("main.py").exists() {
            "main.py"
        } else {
            // Try running as module
            return self.start_as_module().await;
        };

        info!(
            script = %server_script,
            port = %self.config.port,
            "Starting Supertonic server..."
        );

        let child = Command::new("python")
            .args([
                server_script,
                "--port", &self.config.port.to_string(),
                "--host", &self.config.host,
            ])
            .current_dir(&py_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| VoiceError::Other(format!("Failed to start server: {}", e)))?;

        let pid = child.id();
        {
            let mut proc = self.process.write().await;
            *proc = Some(child);
        }

        // Wait for server to be ready
        self.wait_for_ready().await?;

        info!(pid = pid, endpoint = %self.endpoint(), "Supertonic server started");
        Ok(())
    }

    /// Start as Python module (fallback)
    async fn start_as_module(&self) -> Result<(), VoiceError> {
        let py_dir = self.config.base_dir.join("py");

        info!(
            port = %self.config.port,
            "Starting Supertonic as module..."
        );

        let child = Command::new("python")
            .args([
                "-m", "supertonic.server",
                "--port", &self.config.port.to_string(),
                "--host", &self.config.host,
            ])
            .current_dir(&py_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| VoiceError::Other(format!("Failed to start server: {}", e)))?;

        let pid = child.id();
        {
            let mut proc = self.process.write().await;
            *proc = Some(child);
        }

        self.wait_for_ready().await?;

        info!(pid = pid, endpoint = %self.endpoint(), "Supertonic server started");
        Ok(())
    }

    /// Wait for server to be ready
    async fn wait_for_ready(&self) -> Result<(), VoiceError> {
        let client = Client::new();
        let health_url = format!("{}/health", self.endpoint());

        for i in 0..30 {
            tokio::time::sleep(Duration::from_millis(500)).await;

            match client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return Ok(());
                }
                Ok(_) => {
                    // Server responding but not healthy yet
                }
                Err(_) => {
                    // Not ready yet
                }
            }

            if i > 0 && i % 10 == 0 {
                debug!(attempt = i, "Waiting for Supertonic server...");
            }
        }

        Err(VoiceError::NotReady(
            "Supertonic server failed to start within 15 seconds".to_string()
        ))
    }

    /// Stop the server
    pub async fn stop(&self) {
        let mut proc = self.process.write().await;
        if let Some(mut child) = proc.take() {
            info!("Stopping Supertonic server...");
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Check if server is running
    pub async fn is_running(&self) -> bool {
        let proc = self.process.read().await;
        if proc.is_some() {
            // Also check if process is still alive
            let client = Client::new();
            client.get(&format!("{}/health", self.endpoint()))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Create a SupertonicEngine connected to this server
    pub fn engine(&self) -> SupertonicEngine {
        SupertonicEngine::new(&self.endpoint())
            .with_voice(SupertonicVoice {
                id: self.config.voice_style.clone(),
                name: format!("Supertonic {}", self.config.voice_style),
                language: "en-US".to_string(),
                sample_rate: DEFAULT_SAMPLE_RATE,
                description: None,
            })
    }
}

impl Drop for SupertonicServer {
    fn drop(&mut self) {
        // Try to stop the server synchronously
        if let Ok(mut proc) = self.process.try_write() {
            if let Some(mut child) = proc.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

/// Start Supertonic server with default config (convenience function)
pub async fn start_supertonic_server() -> Result<SupertonicServer, VoiceError> {
    let server = SupertonicServer::default_config();
    server.setup().await?;
    server.start().await?;
    Ok(server)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_defaults() {
        let voice = SupertonicVoice::default();
        assert_eq!(voice.id, "F1");
        assert_eq!(voice.sample_rate, 44100);
    }

    #[test]
    fn test_voice_presets() {
        let female = SupertonicVoice::female(2);
        assert_eq!(female.id, "F2");
        assert_eq!(female.name, "Female Voice 2");

        let male = SupertonicVoice::male(3);
        assert_eq!(male.id, "M3");
        assert_eq!(male.name, "Male Voice 3");

        // Test clamping
        let clamped = SupertonicVoice::female(10);
        assert_eq!(clamped.id, "F5");
    }

    #[test]
    fn test_engine_creation() {
        let engine = SupertonicEngine::new("http://localhost:8080")
            .with_voice(SupertonicVoice::custom("custom", "Custom Voice", "en-US"))
            .with_speed(1.2);

        assert_eq!(engine.voice().id, "custom");
        assert_eq!(engine.params.speed, 1.2);
    }

    #[test]
    fn test_localhost_engine() {
        let engine = SupertonicEngine::localhost();
        assert!(engine.endpoint().contains("127.0.0.1:8080"));
    }

    #[test]
    fn test_pcm_to_wav() {
        let pcm = vec![0u8; 100];
        let wav = pcm_to_wav(&pcm, 44100);

        // WAV header is 44 bytes
        assert_eq!(wav.len(), 44 + 100);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn test_docker_compose() {
        let yaml = docker_compose_yaml();
        assert!(yaml.contains("supertonic"));
        assert!(yaml.contains("8080"));
    }

    #[test]
    fn test_params_clamping() {
        let engine = SupertonicEngine::localhost()
            .with_speed(5.0)  // Should clamp to 2.0
            .with_pitch(-5.0); // Should clamp to -1.0

        assert_eq!(engine.params.speed, 2.0);
        assert_eq!(engine.params.pitch, -1.0);
    }

    #[test]
    fn test_preset_enum() {
        assert_eq!(SupertonicPreset::F1.as_str(), "F1");
        assert_eq!(SupertonicPreset::M3.display_name(), "Male Voice 3");
        assert_eq!(SupertonicPreset::all().len(), 10);
    }

    #[test]
    fn test_local_config_defaults() {
        let config = LocalSupertonicConfig::default();
        assert_eq!(config.voice_style, "F1");
        assert_eq!(config.sample_rate, 44100);
        assert!(config.models_dir.to_string_lossy().contains("supertonic"));
    }
}

