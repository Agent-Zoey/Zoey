//! Voice Provider for ZoeyOS
//!
//! Provides text-to-speech (TTS) and speech-to-text (STT) capabilities.
//!
//! ## TTS Engines
//! - OpenAI TTS (tts-1, tts-1-hd) - Low latency, streaming support
//! - ElevenLabs - High quality voices with emotion control
//! - Local HTTP - For OpenVoice, Coqui, or other local TTS servers
//! - Unmute (premium) - GPU-accelerated, real-time
//!
//! ## STT Engines  
//! - Whisper (default) - CPU/GPU via whisper.cpp, auto-downloads models
//! - Unmute (premium) - GPU-accelerated real-time streaming
//!
//! Default voice: Female (shimmer for OpenAI, Rachel for ElevenLabs)

#![warn(missing_docs)]
#![warn(clippy::all)]

mod engines;
mod types;

pub use engines::*;
pub use types::*;

use async_trait::async_trait;
use zoey_core::types::*;
use zoey_core::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Once;
use tokio::sync::RwLock;
use bytes::Bytes;

struct SettingRow {
    name: String,
    value: String,
    source: String,
    change: String,
}
fn pad(s: &str, w: usize) -> String {
    let mut out = s.to_string();
    if out.len() > w {
        out.truncate(w);
    }
    let pad_len = if w > out.len() { w - out.len() } else { 0 };
    out + &" ".repeat(pad_len)
}
fn render(name: &str, color: &str, deco: &str, rows: Vec<SettingRow>) {
    let reset = "\x1b[0m";
    let top = format!("{}+{}+{}", color, "-".repeat(78), reset);
    let title = format!(" {} ", name.to_uppercase());
    let d1 = pad(&format!("{}{}{}", deco, deco, deco), 25);
    let d2 = pad(&format!("{}{}{}", deco, deco, deco), 25);
    let line1 = format!("{}|{}|{}", color, pad(&(d1 + &title + &d2), 78), reset);
    let line2 = format!(
        "{}|{}|{}",
        color,
        pad(&format!("{} Provider • Voice • TTS {}", deco, deco), 78),
        reset
    );
    let sep = format!("{}+{}+{}", color, "=".repeat(78), reset);
    let header = format!(
        "{}|{}|{}|{}|{}|{}",
        color,
        pad("Setting", 24),
        pad("Value", 20),
        pad("Source", 10),
        pad("Change", 24),
        reset
    );
    let mid = format!("{}+{}+{}", color, "-".repeat(78), reset);
    tracing::info!("{}", top);
    tracing::info!("{}", line1);
    tracing::info!("{}", line2);
    tracing::info!("{}", sep);
    tracing::info!("{}", header);
    tracing::info!("{}", mid);
    if rows.is_empty() {
        let row = format!(
            "{}|{}|{}|{}|{}|{}",
            color,
            pad("<none>", 24),
            pad("-", 20),
            pad("-", 10),
            pad("Use code defaults", 24),
            reset
        );
        tracing::info!("{}", row);
    } else {
        for r in rows {
            let row = format!(
                "{}|{}|{}|{}|{}|{}",
                color,
                pad(&r.name, 24),
                pad(&r.value, 20),
                pad(&r.source, 10),
                pad(&r.change, 24),
                reset
            );
            tracing::info!("{}", row);
        }
    }
    let bottom = format!("{}+{}+{}", color, "-".repeat(78), reset);
    tracing::info!("{}", bottom);
}
static INIT: Once = Once::new();

/// Voice provider plugin for TTS and STT capabilities
pub struct VoicePlugin {
    /// Active TTS engine
    tts_engine: Arc<RwLock<Box<dyn VoiceEngine>>>,
    /// Active STT engine (optional)
    #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
    stt_engine: Option<Arc<RwLock<Box<dyn SpeechEngine>>>>,
    /// TTS Configuration
    tts_config: VoiceConfig,
    /// STT Configuration
    stt_config: TranscriptionConfig,
}

impl VoicePlugin {
    /// Create a new voice plugin with the specified TTS engine
    pub fn new(engine: Box<dyn VoiceEngine>, config: VoiceConfig) -> Self {
        Self {
            tts_engine: Arc::new(RwLock::new(engine)),
            #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
            stt_engine: None,
            tts_config: config,
            stt_config: TranscriptionConfig::default(),
        }
    }

    /// Create a new voice plugin with both TTS and STT engines
    #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
    pub fn new_with_stt(
        tts_engine: Box<dyn VoiceEngine>,
        stt_engine: Box<dyn SpeechEngine>,
        tts_config: VoiceConfig,
        stt_config: TranscriptionConfig,
    ) -> Self {
        Self {
            tts_engine: Arc::new(RwLock::new(tts_engine)),
            stt_engine: Some(Arc::new(RwLock::new(stt_engine))),
            tts_config,
            stt_config,
        }
    }

    /// Create with OpenAI TTS engine (default female voice: shimmer)
    pub fn with_openai(api_key: Option<String>) -> Self {
        let engine = engines::openai::OpenAIVoiceEngine::new(api_key);
        Self::new(
            Box::new(engine),
            VoiceConfig {
                engine_type: VoiceEngineType::OpenAI,
                voice: Voice::default_female(),
                ..Default::default()
            },
        )
    }

    /// Create with ElevenLabs TTS engine (default female voice: Rachel)
    pub fn with_elevenlabs(api_key: Option<String>) -> Self {
        let engine = engines::elevenlabs::ElevenLabsVoiceEngine::new(api_key);
        Self::new(
            Box::new(engine),
            VoiceConfig {
                engine_type: VoiceEngineType::ElevenLabs,
                voice: Voice::elevenlabs_rachel(),
                ..Default::default()
            },
        )
    }

    /// Create with local HTTP TTS server (OpenVoice, Coqui, etc.)
    pub fn with_local(endpoint: String) -> Self {
        let engine = engines::local::LocalVoiceEngine::new(endpoint);
        Self::new(
            Box::new(engine),
            VoiceConfig {
                engine_type: VoiceEngineType::Local,
                voice: Voice::default_female(),
                ..Default::default()
            },
        )
    }

    /// Create with Piper TTS (ultra low-latency local TTS)
    /// 
    /// Piper is a fast, local neural TTS that achieves ~50ms latency.
    /// 
    /// ## Setup Piper Server
    /// ```bash
    /// docker run -d -p 5500:5500 rhasspy/wyoming-piper --voice en_US-lessac-medium
    /// ```
    /// 
    /// ## Example
    /// ```rust,ignore
    /// let plugin = VoicePlugin::with_piper("http://localhost:5500");
    /// let audio = plugin.synthesize("Hello!").await?;
    /// ```
    pub fn with_piper(endpoint: &str) -> Self {
        let engine = engines::piper::PiperEngine::new(endpoint);
        Self::new(
            Box::new(engine),
            VoiceConfig {
                engine_type: VoiceEngineType::Local,
                voice: Voice::custom(
                    "en_US-lessac-medium".to_string(),
                    "Lessac".to_string(),
                    VoiceGender::Female,
                    "en-US".to_string(),
                ),
                sample_rate: 22050,
                ..Default::default()
            },
        )
    }

    /// Create with Piper TTS + Whisper STT (fully local, low latency)
    /// 
    /// This is the recommended setup for real-time voice with minimal latency:
    /// - Piper TTS: ~50ms synthesis latency
    /// - Whisper STT: ~100-200ms transcription latency
    /// - Total: ~400-600ms end-to-end (similar to real Unmute!)
    /// 
    /// ## Setup
    /// ```bash
    /// # Start Piper TTS server
    /// docker run -d -p 5500:5500 rhasspy/wyoming-piper --voice en_US-lessac-medium
    /// ```
    /// 
    /// ## Example
    /// ```rust,ignore
    /// let plugin = VoicePlugin::with_piper_and_whisper(
    ///     "http://localhost:5500",
    ///     WhisperModel::Base,
    /// );
    /// 
    /// // Transcribe
    /// let text = plugin.transcribe(&audio).await?;
    /// 
    /// // Synthesize response
    /// let response_audio = plugin.synthesize("I heard you!").await?;
    /// ```
    #[cfg(feature = "whisper")]
    pub fn with_piper_and_whisper(piper_endpoint: &str, whisper_model: WhisperModel) -> Self {
        let tts_engine = engines::piper::PiperEngine::new(piper_endpoint);
        let stt_engine = engines::whisper::WhisperEngine::new(whisper_model);
        
        Self {
            tts_engine: Arc::new(RwLock::new(Box::new(tts_engine))),
            stt_engine: Some(Arc::new(RwLock::new(Box::new(stt_engine)))),
            tts_config: VoiceConfig {
                engine_type: VoiceEngineType::Local,
                voice: Voice::custom(
                    "en_US-lessac-medium".to_string(),
                    "Lessac".to_string(),
                    VoiceGender::Female,
                    "en-US".to_string(),
                ),
                sample_rate: 22050,
                ..Default::default()
            },
            stt_config: TranscriptionConfig {
                engine_type: SpeechEngineType::Whisper,
                whisper_model,
                ..Default::default()
            },
        }
    }

    /// Create with Whisper STT engine (default, CPU/GPU)
    /// Uses OpenAI TTS for synthesis
    #[cfg(feature = "whisper")]
    pub fn with_whisper(model: WhisperModel) -> Self {
        let tts_engine = engines::openai::OpenAIVoiceEngine::new(None);
        let stt_engine = engines::whisper::WhisperEngine::new(model);
        
        Self {
            tts_engine: Arc::new(RwLock::new(Box::new(tts_engine))),
            stt_engine: Some(Arc::new(RwLock::new(Box::new(stt_engine)))),
            tts_config: VoiceConfig {
                engine_type: VoiceEngineType::OpenAI,
                voice: Voice::default_female(),
                ..Default::default()
            },
            stt_config: TranscriptionConfig {
                engine_type: SpeechEngineType::Whisper,
                whisper_model: model,
                ..Default::default()
            },
        }
    }

    /// Create with Supertonic TTS engine (ultra-fast on-device TTS)
    /// 
    /// Supertonic provides lightning-fast local TTS via ONNX models.
    /// Performance: ~10-50ms latency, 44.1kHz sample rate.
    /// 
    /// ## Setup
    /// ```bash
    /// # Download models
    /// cd .zoey/voice && git clone https://huggingface.co/Supertone/supertonic supertonic
    /// 
    /// # Clone server code and run
    /// git clone https://github.com/supertone-inc/supertonic.git supertonic-server
    /// cd supertonic-server/py && pip install -r requirements.txt
    /// python server.py --port 8080
    /// ```
    /// 
    /// ## Example
    /// ```rust,ignore
    /// let plugin = VoicePlugin::with_supertonic("http://localhost:8080");
    /// let audio = plugin.synthesize("Hello!").await?;
    /// ```
    pub fn with_supertonic(endpoint: &str) -> Self {
        let engine = engines::supertonic::SupertonicEngine::new(endpoint);
        Self::new(
            Box::new(engine),
            VoiceConfig {
                engine_type: VoiceEngineType::Local,
                voice: Voice::custom(
                    "F1".to_string(),
                    "Supertonic F1".to_string(),
                    VoiceGender::Female,
                    "en-US".to_string(),
                ),
                sample_rate: 44100,
                output_format: AudioFormat::Pcm,
                endpoint: Some(endpoint.to_string()),
                ..Default::default()
            },
        )
    }

    /// Create with Pocket TTS engine (lightweight CPU-based TTS by Kyutai Labs)
    /// 
    /// Pocket TTS is a fast, lightweight TTS system that runs entirely on CPU.
    /// It achieves ~200ms latency for the first audio chunk and ~6x real-time
    /// synthesis speed with only 100M parameters.
    /// 
    /// ## Setup
    /// ```bash
    /// pip install pocket-tts
    /// pocket-tts serve  # Runs on http://localhost:8000
    /// ```
    /// 
    /// ## Example
    /// ```rust,ignore
    /// let plugin = VoicePlugin::with_pocket_tts("http://localhost:8000");
    /// let audio = plugin.synthesize("Hello!").await?;
    /// ```
    /// 
    /// ## References
    /// - GitHub: https://github.com/kyutai-labs/pocket-tts
    pub fn with_pocket_tts(endpoint: &str) -> Self {
        let engine = engines::pocket_tts::PocketTTSEngine::new(endpoint);
        Self::new(
            Box::new(engine),
            VoiceConfig {
                engine_type: VoiceEngineType::Local,
                voice: Voice::custom(
                    "alba".to_string(),
                    "Alba".to_string(),
                    VoiceGender::Female,
                    "en".to_string(),
                ),
                sample_rate: 24000,
                output_format: AudioFormat::Wav,
                endpoint: Some(endpoint.to_string()),
                ..Default::default()
            },
        )
    }

    /// Create with Pocket TTS engine with a specific voice
    /// 
    /// Available voices: alba, marius, javert, jean, fantine, cosette, eponine, azelma
    /// 
    /// ## Example
    /// ```rust,ignore
    /// let plugin = VoicePlugin::with_pocket_tts_voice("http://localhost:8000", "marius");
    /// let audio = plugin.synthesize("Hello!").await?;
    /// ```
    pub fn with_pocket_tts_voice(endpoint: &str, voice_name: &str) -> Self {
        let engine = engines::pocket_tts::PocketTTSEngine::new(endpoint)
            .with_voice_name(voice_name);
        let gender = if voice_name == "marius" || voice_name == "javert" || voice_name == "jean" {
            VoiceGender::Male
        } else {
            VoiceGender::Female
        };
        Self::new(
            Box::new(engine),
            VoiceConfig {
                engine_type: VoiceEngineType::Local,
                voice: Voice::custom(
                    voice_name.to_string(),
                    voice_name.to_string(),
                    gender,
                    "en".to_string(),
                ),
                sample_rate: 24000,
                output_format: AudioFormat::Wav,
                endpoint: Some(endpoint.to_string()),
                ..Default::default()
            },
        )
    }

    /// Create with Pocket TTS engine with voice cloning
    /// 
    /// Provide a WAV file path or HuggingFace URL for voice cloning.
    /// 
    /// ## Example
    /// ```rust,ignore
    /// // Clone from HuggingFace voice sample
    /// let plugin = VoicePlugin::with_pocket_tts_clone(
    ///     "http://localhost:8000",
    ///     "hf://kyutai/tts-voices/alba-mackenna/casual.wav",
    /// );
    /// 
    /// // Clone from local file
    /// let plugin = VoicePlugin::with_pocket_tts_clone(
    ///     "http://localhost:8000",
    ///     "/path/to/voice.wav",
    /// );
    /// ```
    pub fn with_pocket_tts_clone(endpoint: &str, voice_path: &str) -> Self {
        let engine = engines::pocket_tts::PocketTTSEngine::new(endpoint)
            .with_voice_prompt(voice_path);
        Self::new(
            Box::new(engine),
            VoiceConfig {
                engine_type: VoiceEngineType::Local,
                voice: Voice::custom(
                    voice_path.to_string(),
                    "Custom Voice".to_string(),
                    VoiceGender::Neutral,
                    "en".to_string(),
                ),
                sample_rate: 24000,
                output_format: AudioFormat::Wav,
                endpoint: Some(endpoint.to_string()),
                ..Default::default()
            },
        )
    }

    /// Create with Unmute engine for both STT and TTS (premium, GPU)
    /// 
    /// Connects to an existing Unmute instance at the given endpoint.
    #[cfg(feature = "unmute")]
    pub fn with_unmute(endpoint: &str) -> Self {
        let engine = engines::unmute::UnmuteEngine::with_endpoint(endpoint);
        let engine_arc: Arc<RwLock<Box<dyn VoiceEngine>>> = 
            Arc::new(RwLock::new(Box::new(engines::unmute::UnmuteEngine::with_endpoint(endpoint))));
        let stt_engine: Arc<RwLock<Box<dyn SpeechEngine>>> = 
            Arc::new(RwLock::new(Box::new(engine)));
        
        Self {
            tts_engine: engine_arc,
            stt_engine: Some(stt_engine),
            tts_config: VoiceConfig {
                engine_type: VoiceEngineType::Local, // Unmute acts as local
                voice: Voice::default_female(),
                endpoint: Some(endpoint.to_string()),
                ..Default::default()
            },
            stt_config: TranscriptionConfig {
                engine_type: SpeechEngineType::Unmute,
                unmute_endpoint: Some(endpoint.to_string()),
                ..Default::default()
            },
        }
    }

    /// Create with Unmute Dockerless (starts and manages unmute services)
    /// 
    /// This method starts the full unmute stack without Docker:
    /// - Backend service
    /// - LLM service (requires 6.1 GB VRAM)
    /// - STT service (requires 2.5 GB VRAM)
    /// - TTS service (requires 5.3 GB VRAM)
    /// 
    /// ## Prerequisites
    /// - Unmute repository cloned with dockerless/ directory
    /// - CUDA 12.1 installed
    /// - GPU with at least 13.9 GB VRAM
    /// 
    /// ## Example
    /// ```rust,ignore
    /// use zoey_provider_voice::VoicePlugin;
    /// 
    /// // Start dockerless unmute and create plugin
    /// let (mut manager, plugin) = VoicePlugin::with_unmute_dockerless("/path/to/unmute").await?;
    /// 
    /// // Use the plugin
    /// let audio = plugin.synthesize("Hello!").await?;
    /// 
    /// // Cleanup when done
    /// manager.stop_all().await?;
    /// ```
    #[cfg(feature = "unmute")]
    pub async fn with_unmute_dockerless<P: AsRef<std::path::Path>>(
        unmute_dir: P,
    ) -> Result<(engines::unmute_dockerless::UnmuteDockerless, Self)> {
        use engines::unmute_dockerless::UnmuteDockerless;
        
        let mut manager = UnmuteDockerless::builder()
            .unmute_dir(unmute_dir)
            .build()
            .await?;

        manager.start_all().await?;
        let endpoint = manager.endpoint();

        let plugin = Self::with_unmute(&endpoint);

        Ok((manager, plugin))
    }

    /// Create with Moshi STT engine (real-time speech-text via Moshi server)
    /// 
    /// Moshi is a speech-text foundation model and full-duplex spoken dialogue
    /// framework by Kyutai. It provides near real-time transcription with very
    /// low latency (~200ms).
    /// 
    /// ## Prerequisites
    /// - Running Moshi server (local or remote)
    /// - Start with: `cargo run --features cuda --bin moshi-backend -r -- --config config.json standalone`
    /// 
    /// ## Example
    /// ```rust,ignore
    /// use zoey_provider_voice::VoicePlugin;
    /// 
    /// // Connect to local Moshi server (default: localhost:8998)
    /// let plugin = VoicePlugin::with_moshi("localhost:8998");
    /// 
    /// // Transcribe audio
    /// let result = plugin.transcribe(&audio).await?;
    /// println!("Transcribed: {}", result.text);
    /// ```
    /// 
    /// ## References
    /// - GitHub: https://github.com/kyutai-labs/moshi
    /// - Paper: https://arxiv.org/abs/2410.00037
    #[cfg(feature = "moshi")]
    pub fn with_moshi(endpoint: &str) -> Self {
        let tts_engine = engines::openai::OpenAIVoiceEngine::new(None);
        let moshi_config = engines::moshi::MoshiConfig {
            endpoint: endpoint.to_string(),
            ..Default::default()
        };
        let stt_engine = engines::moshi::MoshiEngine::new(moshi_config);
        
        Self {
            tts_engine: Arc::new(RwLock::new(Box::new(tts_engine))),
            #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
            stt_engine: Some(Arc::new(RwLock::new(Box::new(stt_engine)))),
            tts_config: VoiceConfig {
                engine_type: VoiceEngineType::OpenAI,
                voice: Voice::default_female(),
                ..Default::default()
            },
            stt_config: TranscriptionConfig {
                engine_type: SpeechEngineType::Moshi,
                moshi_endpoint: Some(endpoint.to_string()),
                ..Default::default()
            },
        }
    }

    /// Create with Moshi STT + Piper TTS (fully local, near real-time)
    /// 
    /// This combines:
    /// - Moshi for STT: ~200ms latency, full-duplex speech-text
    /// - Piper for TTS: ~50ms synthesis latency
    /// 
    /// Total end-to-end latency: ~300-500ms (comparable to human response time)
    /// 
    /// ## Example
    /// ```rust,ignore
    /// let plugin = VoicePlugin::with_moshi_and_piper(
    ///     "localhost:8998",      // Moshi server
    ///     "http://localhost:5500" // Piper server
    /// );
    /// ```
    #[cfg(feature = "moshi")]
    pub fn with_moshi_and_piper(moshi_endpoint: &str, piper_endpoint: &str) -> Self {
        let tts_engine = engines::piper::PiperEngine::new(piper_endpoint);
        let moshi_config = engines::moshi::MoshiConfig {
            endpoint: moshi_endpoint.to_string(),
            ..Default::default()
        };
        let stt_engine = engines::moshi::MoshiEngine::new(moshi_config);
        
        Self {
            tts_engine: Arc::new(RwLock::new(Box::new(tts_engine))),
            #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
            stt_engine: Some(Arc::new(RwLock::new(Box::new(stt_engine)))),
            tts_config: VoiceConfig {
                engine_type: VoiceEngineType::Local,
                voice: Voice::custom(
                    "en_US-lessac-medium".to_string(),
                    "Lessac".to_string(),
                    VoiceGender::Female,
                    "en-US".to_string(),
                ),
                sample_rate: 22050,
                ..Default::default()
            },
            stt_config: TranscriptionConfig {
                engine_type: SpeechEngineType::Moshi,
                moshi_endpoint: Some(moshi_endpoint.to_string()),
                ..Default::default()
            },
        }
    }

    /// Add Whisper STT to an existing plugin
    #[cfg(feature = "whisper")]
    pub fn add_whisper_stt(&mut self, model: WhisperModel) {
        let stt_engine = engines::whisper::WhisperEngine::new(model);
        self.stt_engine = Some(Arc::new(RwLock::new(Box::new(stt_engine))));
        self.stt_config.engine_type = SpeechEngineType::Whisper;
        self.stt_config.whisper_model = model;
    }

    /// Add Unmute STT to an existing plugin
    #[cfg(feature = "unmute")]
    pub fn add_unmute_stt(&mut self, endpoint: &str) {
        let stt_engine = engines::unmute::UnmuteEngine::with_endpoint(endpoint);
        self.stt_engine = Some(Arc::new(RwLock::new(Box::new(stt_engine))));
        self.stt_config.engine_type = SpeechEngineType::Unmute;
        self.stt_config.unmute_endpoint = Some(endpoint.to_string());
    }

    /// Add Moshi STT to an existing plugin
    /// 
    /// ## Example
    /// ```rust,ignore
    /// let mut plugin = VoicePlugin::with_openai(None);
    /// plugin.add_moshi_stt("localhost:8998");
    /// 
    /// // Now has both OpenAI TTS and Moshi STT
    /// let text = plugin.transcribe(&audio).await?;
    /// ```
    #[cfg(feature = "moshi")]
    pub fn add_moshi_stt(&mut self, endpoint: &str) {
        let moshi_config = engines::moshi::MoshiConfig {
            endpoint: endpoint.to_string(),
            ..Default::default()
        };
        let stt_engine = engines::moshi::MoshiEngine::new(moshi_config);
        self.stt_engine = Some(Arc::new(RwLock::new(Box::new(stt_engine))));
        self.stt_config.engine_type = SpeechEngineType::Moshi;
        self.stt_config.moshi_endpoint = Some(endpoint.to_string());
    }

    // =========================================================================
    // TTS Methods
    // =========================================================================

    /// Synthesize text to speech
    pub async fn synthesize(&self, text: &str) -> Result<AudioData> {
        let engine = self.tts_engine.read().await;
        engine.synthesize(text, &self.tts_config).await
    }

    /// Synthesize text to speech with streaming (low latency)
    pub async fn synthesize_stream(&self, text: &str) -> Result<AudioStream> {
        let engine = self.tts_engine.read().await;
        engine.synthesize_stream(text, &self.tts_config).await
    }

    /// Set the voice
    pub fn set_voice(&mut self, voice: Voice) {
        self.tts_config.voice = voice;
    }

    /// Set the speaking speed (0.25 to 4.0, default 1.0)
    pub fn set_speed(&mut self, speed: f32) {
        self.tts_config.speed = speed.clamp(0.25, 4.0);
    }

    /// Enable/disable streaming mode
    pub fn set_streaming(&mut self, enabled: bool) {
        self.tts_config.streaming = enabled;
    }

    /// Get the current TTS engine type
    pub fn engine_type(&self) -> VoiceEngineType {
        self.tts_config.engine_type
    }

    /// Get available voices for the current TTS engine
    pub async fn available_voices(&self) -> Result<Vec<Voice>> {
        let engine = self.tts_engine.read().await;
        engine.available_voices().await
    }

    /// Get the TTS config
    pub fn tts_config(&self) -> &VoiceConfig {
        &self.tts_config
    }

    // =========================================================================
    // STT Methods
    // =========================================================================

    /// Check if STT is available
    #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
    pub fn has_stt(&self) -> bool {
        self.stt_engine.is_some()
    }

    /// Check if STT is available (stub when no STT features)
    #[cfg(not(any(feature = "whisper", feature = "unmute", feature = "moshi")))]
    pub fn has_stt(&self) -> bool {
        false
    }

    /// Transcribe audio to text
    #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
    pub async fn transcribe(&self, audio: &AudioData) -> Result<TranscriptionResult> {
        let engine = self.stt_engine.as_ref().ok_or_else(|| {
            VoiceError::NotReady("STT engine not configured".to_string())
        })?;
        
        let engine_guard = engine.read().await;
        engine_guard.transcribe(audio, &self.stt_config).await
    }

    /// Transcribe audio to text (stub when no STT features)
    #[cfg(not(any(feature = "whisper", feature = "unmute", feature = "moshi")))]
    pub async fn transcribe(&self, _audio: &AudioData) -> Result<TranscriptionResult> {
        Err(VoiceError::NotReady(
            "STT not available. Compile with 'whisper', 'unmute', or 'moshi' feature".to_string()
        ).into())
    }

    /// Transcribe audio to text with streaming results
    #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
    pub async fn transcribe_stream(&self, audio: &AudioData) -> Result<TranscriptionStream> {
        let engine = self.stt_engine.as_ref().ok_or_else(|| {
            VoiceError::NotReady("STT engine not configured".to_string())
        })?;
        
        let engine_guard = engine.read().await;
        engine_guard.transcribe_stream(audio, &self.stt_config).await
    }

    /// Transcribe audio to text with streaming (stub when no STT features)
    #[cfg(not(any(feature = "whisper", feature = "unmute", feature = "moshi")))]
    pub async fn transcribe_stream(&self, _audio: &AudioData) -> Result<TranscriptionStream> {
        Err(VoiceError::NotReady(
            "STT not available. Compile with 'whisper', 'unmute', or 'moshi' feature".to_string()
        ).into())
    }

    /// Transcribe raw audio bytes (convenience method)
    /// Assumes 16-bit PCM mono at the specified sample rate
    #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
    pub async fn transcribe_pcm(&self, pcm_data: &[u8], sample_rate: u32) -> Result<TranscriptionResult> {
        let audio = AudioData {
            data: Bytes::copy_from_slice(pcm_data),
            format: AudioFormat::Pcm,
            sample_rate,
            duration_ms: Some((pcm_data.len() as u64 * 1000) / (sample_rate as u64 * 2)),
            character_count: 0,
        };
        self.transcribe(&audio).await
    }

    /// Transcribe raw PCM (stub when no STT features)
    #[cfg(not(any(feature = "whisper", feature = "unmute", feature = "moshi")))]
    pub async fn transcribe_pcm(&self, _pcm_data: &[u8], _sample_rate: u32) -> Result<TranscriptionResult> {
        Err(VoiceError::NotReady(
            "STT not available. Compile with 'whisper', 'unmute', or 'moshi' feature".to_string()
        ).into())
    }

    /// Get the STT config
    pub fn stt_config(&self) -> &TranscriptionConfig {
        &self.stt_config
    }

    /// Set STT language hint
    pub fn set_stt_language(&mut self, language: Option<String>) {
        self.stt_config.language = language;
    }

    /// Enable/disable STT timestamps
    pub fn set_stt_timestamps(&mut self, enabled: bool) {
        self.stt_config.timestamps = enabled;
    }

    /// Get the current STT engine type
    pub fn stt_engine_type(&self) -> SpeechEngineType {
        self.stt_config.engine_type
    }

    /// Check if STT engine is ready
    #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
    pub async fn is_stt_ready(&self) -> bool {
        if let Some(ref engine) = self.stt_engine {
            let guard = engine.read().await;
            guard.is_ready().await
        } else {
            false
        }
    }

    /// Check if STT engine is ready (stub)
    #[cfg(not(any(feature = "whisper", feature = "unmute", feature = "moshi")))]
    pub async fn is_stt_ready(&self) -> bool {
        false
    }
}

impl Default for VoicePlugin {
    fn default() -> Self {
        Self::with_openai(None)
    }
}

#[async_trait]
impl Plugin for VoicePlugin {
    fn name(&self) -> &str {
        "voice"
    }

    fn description(&self) -> &str {
        "Voice synthesis (TTS) and speech recognition (STT) with multiple engine support"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        INIT.call_once(|| {
            let mut rows = vec![
                SettingRow {
                    name: "TTS_ENGINE".to_string(),
                    value: self.tts_config.engine_type.as_str().to_string(),
                    source: "default".to_string(),
                    change: "set via VoicePlugin::with_*".to_string(),
                },
                SettingRow {
                    name: "VOICE_NAME".to_string(),
                    value: self.tts_config.voice.name.clone(),
                    source: "default".to_string(),
                    change: "set via config or set_voice()".to_string(),
                },
                SettingRow {
                    name: "STREAMING".to_string(),
                    value: format!("{}", self.tts_config.streaming),
                    source: "default".to_string(),
                    change: "set via set_streaming(bool)".to_string(),
                },
                SettingRow {
                    name: "STT_ENGINE".to_string(),
                    value: self.stt_config.engine_type.as_str().to_string(),
                    source: "default".to_string(),
                    change: "set via with_whisper/with_unmute".to_string(),
                },
            ];
            
            // Add STT status
            #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
            {
                rows.push(SettingRow {
                    name: "STT_AVAILABLE".to_string(),
                    value: if self.stt_engine.is_some() { "yes" } else { "no" }.to_string(),
                    source: "feature".to_string(),
                    change: "add_whisper_stt/add_unmute_stt/add_moshi_stt".to_string(),
                });
            }
            
            render("voice", "\x1b[34m", "=", rows);
        });

        Ok(())
    }

    fn models(&self) -> HashMap<String, ModelHandler> {
        let mut models = HashMap::new();

        // Register TTS model handler
        let tts_engine = Arc::clone(&self.tts_engine);
        let tts_config = self.tts_config.clone();

        let tts_handler: ModelHandler = Arc::new(move |params: ModelHandlerParams| {
            let engine = Arc::clone(&tts_engine);
            let config = tts_config.clone();

            Box::pin(async move {
                let text = params.params.prompt;

                // Use tokio RwLock which is Send-safe across await points
                let engine_guard = engine.read().await;
                let audio = engine_guard.synthesize(&text, &config).await?;

                // Return audio as base64-encoded string
                let encoded =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &audio.data);

                Ok(serde_json::json!({
                    "audio": encoded,
                    "format": audio.format.as_str(),
                    "sample_rate": audio.sample_rate,
                    "duration_ms": audio.duration_ms,
                })
                .to_string())
            })
        });

        models.insert("TTS".to_string(), tts_handler.clone());
        models.insert("VOICE".to_string(), tts_handler);

        // Register STT model handler (when STT features are enabled)
        #[cfg(any(feature = "whisper", feature = "unmute", feature = "moshi"))]
        if let Some(ref stt_engine) = self.stt_engine {
            let engine = Arc::clone(stt_engine);
            let config = self.stt_config.clone();

            let stt_handler: ModelHandler = Arc::new(move |params: ModelHandlerParams| {
                let engine = Arc::clone(&engine);
                let config = config.clone();

                Box::pin(async move {
                    // Expect audio data in base64 in the prompt field
                    let audio_b64 = params.params.prompt;
                    
                    let audio_bytes = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        &audio_b64,
                    ).map_err(|e| zoey_core::ZoeyError::other(format!("Invalid base64 audio: {}", e)))?;

                    // Default sample rate for STT (16kHz is standard for Whisper)
                    let sample_rate = 16000u32;

                    let audio = AudioData {
                        data: Bytes::from(audio_bytes),
                        format: AudioFormat::Pcm,
                        sample_rate,
                        duration_ms: None,
                        character_count: 0,
                    };

                    let engine_guard = engine.read().await;
                    let result = engine_guard.transcribe(&audio, &config).await?;

                    Ok(serde_json::json!({
                        "text": result.text,
                        "language": result.language,
                        "confidence": result.confidence,
                        "duration_ms": result.duration_ms,
                        "segments": result.segments,
                    })
                    .to_string())
                })
            });

            models.insert("STT".to_string(), stt_handler.clone());
            models.insert("TRANSCRIBE".to_string(), stt_handler);
        }

        models
    }
}

#[async_trait]
impl Provider for VoicePlugin {
    fn name(&self) -> &str {
        "voice"
    }

    fn capabilities(&self) -> Option<Vec<String>> {
        let mut caps = vec![
            "TTS".to_string(),
            "VOICE".to_string(),
            "SPEECH_SYNTHESIS".to_string(),
        ];
        
        // Add STT capabilities if available
        if self.has_stt() {
            caps.push("STT".to_string());
            caps.push("TRANSCRIBE".to_string());
            caps.push("SPEECH_RECOGNITION".to_string());
        }
        
        Some(caps)
    }

    fn description(&self) -> Option<String> {
        let stt_status = if self.has_stt() {
            format!(" + {} STT", self.stt_config.engine_type.as_str())
        } else {
            String::new()
        };
        
        Some(format!(
            "Voice provider: {} TTS{}",
            self.tts_config.engine_type.as_str(),
            stt_status
        ))
    }

    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        // Return provider metadata
        let stt_info = if self.has_stt() {
            format!(", {} STT", self.stt_config.engine_type.as_str())
        } else {
            String::new()
        };
        
        Ok(ProviderResult {
            text: Some(format!(
                "Voice provider: {} TTS with {} voice{}",
                self.tts_config.engine_type.as_str(),
                self.tts_config.voice.name,
                stt_info
            )),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_plugin_creation() {
        let plugin = VoicePlugin::with_openai(None);
        assert_eq!(Plugin::name(&plugin), "voice");
        assert_eq!(plugin.engine_type(), VoiceEngineType::OpenAI);
    }

    #[test]
    fn test_voice_config_defaults() {
        let config = VoiceConfig::default();
        assert_eq!(config.speed, 1.0);
        assert!(config.streaming);
        assert_eq!(config.output_format, AudioFormat::Mp3);
    }

    #[test]
    fn test_default_female_voice() {
        let voice = Voice::default_female();
        assert_eq!(voice.name, "shimmer");
        assert_eq!(voice.gender, VoiceGender::Female);
    }

    #[test]
    fn test_voice_plugin_models() {
        let plugin = VoicePlugin::default();
        let models = Plugin::models(&plugin);
        assert!(models.contains_key("TTS"));
        assert!(models.contains_key("VOICE"));
    }

    #[test]
    fn test_stt_config_defaults() {
        let config = TranscriptionConfig::default();
        assert_eq!(config.engine_type, SpeechEngineType::Whisper);
        assert_eq!(config.whisper_model, WhisperModel::Base);
        assert!(!config.timestamps);
    }

    #[test]
    fn test_transcription_result() {
        let result = TranscriptionResult::new("Hello world".to_string());
        assert_eq!(result.text, "Hello world");
        assert!(!result.is_empty());
        
        let empty = TranscriptionResult::new(String::new());
        assert!(empty.is_empty());
    }

    #[test]
    fn test_plugin_has_stt_default() {
        let plugin = VoicePlugin::default();
        // Without whisper/unmute features, has_stt should be false
        #[cfg(not(any(feature = "whisper", feature = "unmute")))]
        assert!(!plugin.has_stt());
    }

    #[cfg(feature = "whisper")]
    #[test]
    fn test_with_whisper() {
        let plugin = VoicePlugin::with_whisper(WhisperModel::Tiny);
        assert!(plugin.has_stt());
        assert_eq!(plugin.stt_engine_type(), SpeechEngineType::Whisper);
        
        let models = Plugin::models(&plugin);
        assert!(models.contains_key("STT"));
        assert!(models.contains_key("TRANSCRIBE"));
    }
}
