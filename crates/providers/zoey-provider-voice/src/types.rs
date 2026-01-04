//! Core types for the voice provider

use async_trait::async_trait;
use bytes::Bytes;
use zoey_core::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Voice engine type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceEngineType {
    /// OpenAI TTS (tts-1, tts-1-hd)
    OpenAI,
    /// ElevenLabs
    ElevenLabs,
    /// Local HTTP TTS server (OpenVoice, Coqui, etc.)
    Local,
}

impl VoiceEngineType {
    /// Get engine type as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::ElevenLabs => "elevenlabs",
            Self::Local => "local",
        }
    }
}

impl Default for VoiceEngineType {
    fn default() -> Self {
        Self::OpenAI
    }
}

/// Voice gender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceGender {
    /// Female voice
    Female,
    /// Male voice
    Male,
    /// Neutral/unspecified
    Neutral,
}

impl Default for VoiceGender {
    fn default() -> Self {
        Self::Female
    }
}

/// Voice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    /// Voice identifier (engine-specific)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Voice gender
    pub gender: VoiceGender,
    /// Language code (e.g., "en-US")
    pub language: String,
    /// Voice description
    pub description: Option<String>,
    /// Preview URL (if available)
    pub preview_url: Option<String>,
}

impl Voice {
    /// Default female voice (OpenAI shimmer - clear, female, great for assistants)
    pub fn default_female() -> Self {
        Self {
            id: "shimmer".to_string(),
            name: "shimmer".to_string(),
            gender: VoiceGender::Female,
            language: "en-US".to_string(),
            description: Some("Clear, warm female voice - ideal for assistants".to_string()),
            preview_url: None,
        }
    }

    /// OpenAI Alloy voice (female, versatile)
    pub fn openai_alloy() -> Self {
        Self {
            id: "alloy".to_string(),
            name: "alloy".to_string(),
            gender: VoiceGender::Female,
            language: "en-US".to_string(),
            description: Some("Versatile, expressive female voice".to_string()),
            preview_url: None,
        }
    }

    /// OpenAI Nova voice (female, warm)
    pub fn openai_nova() -> Self {
        Self {
            id: "nova".to_string(),
            name: "nova".to_string(),
            gender: VoiceGender::Female,
            language: "en-US".to_string(),
            description: Some("Warm, engaging female voice".to_string()),
            preview_url: None,
        }
    }

    /// OpenAI Shimmer voice (female, clear)
    pub fn openai_shimmer() -> Self {
        Self::default_female()
    }

    /// OpenAI Echo voice (male)
    pub fn openai_echo() -> Self {
        Self {
            id: "echo".to_string(),
            name: "echo".to_string(),
            gender: VoiceGender::Male,
            language: "en-US".to_string(),
            description: Some("Clear, authoritative male voice".to_string()),
            preview_url: None,
        }
    }

    /// OpenAI Fable voice (male, expressive)
    pub fn openai_fable() -> Self {
        Self {
            id: "fable".to_string(),
            name: "fable".to_string(),
            gender: VoiceGender::Male,
            language: "en-US".to_string(),
            description: Some("Expressive, storytelling male voice".to_string()),
            preview_url: None,
        }
    }

    /// OpenAI Onyx voice (male, deep)
    pub fn openai_onyx() -> Self {
        Self {
            id: "onyx".to_string(),
            name: "onyx".to_string(),
            gender: VoiceGender::Male,
            language: "en-US".to_string(),
            description: Some("Deep, resonant male voice".to_string()),
            preview_url: None,
        }
    }

    /// ElevenLabs Rachel voice (female, conversational)
    pub fn elevenlabs_rachel() -> Self {
        Self {
            id: "21m00Tcm4TlvDq8ikWAM".to_string(),
            name: "Rachel".to_string(),
            gender: VoiceGender::Female,
            language: "en-US".to_string(),
            description: Some("Natural, conversational female voice".to_string()),
            preview_url: None,
        }
    }

    /// ElevenLabs Domi voice (female, strong)
    pub fn elevenlabs_domi() -> Self {
        Self {
            id: "AZnzlk1XvdvUeBnXmlld".to_string(),
            name: "Domi".to_string(),
            gender: VoiceGender::Female,
            language: "en-US".to_string(),
            description: Some("Strong, confident female voice".to_string()),
            preview_url: None,
        }
    }

    /// ElevenLabs Bella voice (female, soft)
    pub fn elevenlabs_bella() -> Self {
        Self {
            id: "EXAVITQu4vr4xnSDxMaL".to_string(),
            name: "Bella".to_string(),
            gender: VoiceGender::Female,
            language: "en-US".to_string(),
            description: Some("Soft, gentle female voice".to_string()),
            preview_url: None,
        }
    }

    /// Create a custom voice
    pub fn custom(id: String, name: String, gender: VoiceGender, language: String) -> Self {
        Self {
            id,
            name,
            gender,
            language,
            description: None,
            preview_url: None,
        }
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::default_female()
    }
}

/// Audio output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioFormat {
    /// MP3 (most compatible, good compression)
    Mp3,
    /// Opus (best quality/size ratio, low latency)
    Opus,
    /// AAC (good for iOS/Safari)
    Aac,
    /// FLAC (lossless, larger files)
    Flac,
    /// WAV (uncompressed, largest)
    Wav,
    /// PCM raw audio
    Pcm,
}

impl AudioFormat {
    /// Get format as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Aac => "aac",
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Pcm => "pcm",
        }
    }

    /// Get MIME type
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Mp3 => "audio/mpeg",
            Self::Opus => "audio/opus",
            Self::Aac => "audio/aac",
            Self::Flac => "audio/flac",
            Self::Wav => "audio/wav",
            Self::Pcm => "audio/pcm",
        }
    }

    /// Get file extension
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Aac => "aac",
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Pcm => "pcm",
        }
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self::Mp3
    }
}

/// Voice synthesis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    /// Engine type
    pub engine_type: VoiceEngineType,
    /// Selected voice
    pub voice: Voice,
    /// Speaking speed (0.25 to 4.0, default 1.0)
    pub speed: f32,
    /// Output audio format
    pub output_format: AudioFormat,
    /// Enable streaming for low latency
    pub streaming: bool,
    /// Model to use (engine-specific)
    pub model: Option<String>,
    /// Stability (ElevenLabs: 0.0 to 1.0)
    pub stability: Option<f32>,
    /// Similarity boost (ElevenLabs: 0.0 to 1.0)
    pub similarity_boost: Option<f32>,
    /// Style (ElevenLabs: 0.0 to 1.0)
    pub style: Option<f32>,
    /// API endpoint override (for local/custom servers)
    pub endpoint: Option<String>,
    /// Sample rate (Hz)
    pub sample_rate: u32,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            engine_type: VoiceEngineType::OpenAI,
            voice: Voice::default_female(),
            speed: 1.0,
            output_format: AudioFormat::Mp3,
            streaming: true,
            model: None,
            stability: None,
            similarity_boost: None,
            style: None,
            endpoint: None,
            sample_rate: 24000,
        }
    }
}

/// Synthesized audio data
#[derive(Debug, Clone)]
pub struct AudioData {
    /// Raw audio bytes
    pub data: Bytes,
    /// Audio format
    pub format: AudioFormat,
    /// Sample rate (Hz)
    pub sample_rate: u32,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Character count of input text
    pub character_count: usize,
}

impl AudioData {
    /// Create new audio data
    pub fn new(data: Bytes, format: AudioFormat, sample_rate: u32) -> Self {
        Self {
            data,
            format,
            sample_rate,
            duration_ms: None,
            character_count: 0,
        }
    }

    /// Get size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Check if audio data is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Audio stream chunk
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// Chunk data
    pub data: Bytes,
    /// Chunk index
    pub index: usize,
    /// Is this the final chunk?
    pub is_final: bool,
    /// Timestamp offset in milliseconds
    pub timestamp_ms: Option<u64>,
}

/// Audio stream receiver type
pub type AudioStream = mpsc::Receiver<Result<AudioChunk>>;

/// Audio stream sender type
pub type AudioStreamSender = mpsc::Sender<Result<AudioChunk>>;

/// Create an audio stream channel
pub fn create_audio_stream(buffer_size: usize) -> (AudioStreamSender, AudioStream) {
    mpsc::channel(buffer_size)
}

/// Voice engine trait - implemented by each TTS backend
#[async_trait]
pub trait VoiceEngine: Send + Sync {
    /// Engine name
    fn name(&self) -> &str;

    /// Synthesize text to audio
    async fn synthesize(&self, text: &str, config: &VoiceConfig) -> Result<AudioData>;

    /// Synthesize text to audio stream (for low latency)
    async fn synthesize_stream(&self, text: &str, config: &VoiceConfig) -> Result<AudioStream>;

    /// Get available voices
    async fn available_voices(&self) -> Result<Vec<Voice>>;

    /// Check if engine is ready
    async fn is_ready(&self) -> bool;

    /// Get supported audio formats
    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Mp3]
    }

    /// Get maximum text length
    fn max_text_length(&self) -> usize {
        4096
    }
}

// ============================================================================
// Speech-to-Text (STT) Types
// ============================================================================

/// Speech engine type enum for STT backends
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeechEngineType {
    /// Whisper.cpp via whisper-rs (default, CPU/GPU)
    Whisper,
    /// Vosk (fast local recognition, ~100-200ms latency)
    Vosk,
    /// Unmute (premium, GPU-accelerated real-time)
    Unmute,
    /// Moshi (real-time speech-text foundation model, full-duplex)
    /// Based on Kyutai's Moshi: https://github.com/kyutai-labs/moshi
    Moshi,
}

impl SpeechEngineType {
    /// Get engine type as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Whisper => "whisper",
            Self::Vosk => "vosk",
            Self::Unmute => "unmute",
            Self::Moshi => "moshi",
        }
    }
}

impl Default for SpeechEngineType {
    fn default() -> Self {
        Self::Whisper
    }
}

/// Whisper model size options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WhisperModel {
    /// Tiny model (~75MB, fastest, lower accuracy)
    Tiny,
    /// Base model (~142MB, good balance for testing)
    Base,
    /// Small model (~466MB, good accuracy)
    Small,
    /// Medium model (~1.5GB, high accuracy)
    Medium,
    /// Large model (~3GB, best accuracy)
    Large,
    /// Large-v2 model (improved large)
    LargeV2,
    /// Large-v3 model (latest large)
    LargeV3,
}

impl WhisperModel {
    /// Get model name as string (for file naming)
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
            Self::LargeV2 => "large-v2",
            Self::LargeV3 => "large-v3",
        }
    }

    /// Get approximate model file size in MB
    pub fn size_mb(&self) -> u32 {
        match self {
            Self::Tiny => 75,
            Self::Base => 142,
            Self::Small => 466,
            Self::Medium => 1500,
            Self::Large => 3000,
            Self::LargeV2 => 3000,
            Self::LargeV3 => 3000,
        }
    }

    /// Get approximate RAM/VRAM required in MB
    pub fn memory_mb(&self) -> u32 {
        match self {
            Self::Tiny => 400,
            Self::Base => 500,
            Self::Small => 1000,
            Self::Medium => 2500,
            Self::Large => 5000,
            Self::LargeV2 => 5000,
            Self::LargeV3 => 5000,
        }
    }

    /// Get HuggingFace model filename
    pub fn ggml_filename(&self) -> &'static str {
        match self {
            Self::Tiny => "ggml-tiny.bin",
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
            Self::Medium => "ggml-medium.bin",
            Self::Large => "ggml-large.bin",
            Self::LargeV2 => "ggml-large-v2.bin",
            Self::LargeV3 => "ggml-large-v3.bin",
        }
    }
}

impl Default for WhisperModel {
    fn default() -> Self {
        Self::Base // Good balance of speed and accuracy
    }
}

impl std::str::FromStr for WhisperModel {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tiny" => Ok(Self::Tiny),
            "base" => Ok(Self::Base),
            "small" => Ok(Self::Small),
            "medium" => Ok(Self::Medium),
            "large" => Ok(Self::Large),
            "large-v2" | "largev2" => Ok(Self::LargeV2),
            "large-v3" | "largev3" => Ok(Self::LargeV3),
            _ => Err(format!("Unknown whisper model: {}", s)),
        }
    }
}

/// A segment of transcribed speech with timing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    /// Transcribed text for this segment
    pub text: String,
    /// Start time in milliseconds
    pub start_ms: u64,
    /// End time in milliseconds
    pub end_ms: u64,
    /// Confidence score (0.0 to 1.0)
    pub confidence: Option<f32>,
    /// Speaker ID (if speaker diarization is enabled)
    pub speaker_id: Option<String>,
}

/// Result of speech-to-text transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    /// Full transcribed text
    pub text: String,
    /// Detected language code (e.g., "en", "es", "fr")
    pub language: Option<String>,
    /// Language detection confidence (0.0 to 1.0)
    pub language_confidence: Option<f32>,
    /// Overall transcription confidence (0.0 to 1.0)
    pub confidence: Option<f32>,
    /// Audio duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Word/segment-level timestamps
    pub segments: Vec<TranscriptionSegment>,
    /// Processing time in milliseconds
    pub processing_time_ms: Option<u64>,
}

impl TranscriptionResult {
    /// Create a simple transcription result with just text
    pub fn new(text: String) -> Self {
        Self {
            text,
            language: None,
            language_confidence: None,
            confidence: None,
            duration_ms: None,
            segments: Vec::new(),
            processing_time_ms: None,
        }
    }

    /// Create with detected language
    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }

    /// Check if transcription is empty
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

impl Default for TranscriptionResult {
    fn default() -> Self {
        Self::new(String::new())
    }
}

/// Configuration for speech-to-text transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    /// STT engine type
    pub engine_type: SpeechEngineType,
    /// Whisper model size (for Whisper engine)
    pub whisper_model: WhisperModel,
    /// Language hint (ISO 639-1 code, e.g., "en", "es")
    /// If None, language will be auto-detected
    pub language: Option<String>,
    /// Enable word-level timestamps
    pub timestamps: bool,
    /// Enable speaker diarization (identify different speakers)
    pub diarization: bool,
    /// Maximum audio duration to process (in seconds)
    pub max_duration_secs: Option<u32>,
    /// Unmute endpoint URL (for Unmute engine)
    pub unmute_endpoint: Option<String>,
    /// Moshi endpoint URL (for Moshi engine)
    /// Format: "host:port" (e.g., "localhost:8998")
    pub moshi_endpoint: Option<String>,
    /// Temperature for sampling (0.0 = deterministic, higher = more random)
    pub temperature: f32,
    /// Beam size for beam search decoding
    pub beam_size: u32,
    /// Suppress non-speech tokens
    pub suppress_non_speech: bool,
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            engine_type: SpeechEngineType::Whisper,
            whisper_model: WhisperModel::Base,
            language: None, // Auto-detect
            timestamps: false,
            diarization: false,
            max_duration_secs: Some(300), // 5 minutes max
            unmute_endpoint: None,
            moshi_endpoint: None,
            temperature: 0.0,
            beam_size: 5,
            suppress_non_speech: true,
        }
    }
}

/// Streaming transcription chunk
#[derive(Debug, Clone)]
pub struct TranscriptionChunk {
    /// Partial transcription text
    pub text: String,
    /// Is this the final chunk?
    pub is_final: bool,
    /// Timestamp offset in milliseconds
    pub timestamp_ms: Option<u64>,
    /// Confidence for this chunk
    pub confidence: Option<f32>,
}

/// Transcription stream receiver type
pub type TranscriptionStream = mpsc::Receiver<Result<TranscriptionChunk>>;

/// Transcription stream sender type
pub type TranscriptionStreamSender = mpsc::Sender<Result<TranscriptionChunk>>;

/// Create a transcription stream channel
pub fn create_transcription_stream(buffer_size: usize) -> (TranscriptionStreamSender, TranscriptionStream) {
    mpsc::channel(buffer_size)
}

/// Speech engine trait - implemented by each STT backend
#[async_trait]
pub trait SpeechEngine: Send + Sync {
    /// Engine name
    fn name(&self) -> &str;

    /// Transcribe audio to text
    async fn transcribe(&self, audio: &AudioData, config: &TranscriptionConfig) -> Result<TranscriptionResult>;

    /// Transcribe audio to text with streaming results
    async fn transcribe_stream(&self, audio: &AudioData, config: &TranscriptionConfig) -> Result<TranscriptionStream>;

    /// Check if engine is ready (model loaded, service available)
    async fn is_ready(&self) -> bool;

    /// Get supported input audio formats
    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Wav, AudioFormat::Mp3, AudioFormat::Pcm]
    }

    /// Get maximum audio duration in seconds
    fn max_duration_secs(&self) -> u32 {
        300 // 5 minutes default
    }

    /// Get supported languages (ISO 639-1 codes)
    fn supported_languages(&self) -> Vec<&'static str> {
        vec!["en"] // English by default
    }
}

/// Voice synthesis and transcription error types
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    /// API authentication error
    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimitError(String),

    /// Invalid voice
    #[error("Invalid voice: {0}")]
    InvalidVoice(String),

    /// Text too long
    #[error("Text exceeds maximum length: {length} > {max}")]
    TextTooLong {
        /// Actual text length
        length: usize,
        /// Maximum allowed length
        max: usize,
    },

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Audio processing error
    #[error("Audio processing error: {0}")]
    AudioError(String),

    /// Engine not ready
    #[error("Voice engine not ready: {0}")]
    NotReady(String),

    /// Unsupported format
    #[error("Unsupported audio format: {0}")]
    UnsupportedFormat(String),

    // STT-specific errors
    /// Audio duration too long
    #[error("Audio duration exceeds maximum: {duration_secs}s > {max_secs}s")]
    AudioTooLong {
        /// Actual duration in seconds
        duration_secs: u32,
        /// Maximum allowed duration
        max_secs: u32,
    },

    /// Model not found or failed to load
    #[error("Model error: {0}")]
    ModelError(String),

    /// Model download failed
    #[error("Failed to download model: {0}")]
    ModelDownloadError(String),

    /// Transcription failed
    #[error("Transcription failed: {0}")]
    TranscriptionError(String),

    /// Unsupported language
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    /// WebSocket connection error (for Unmute)
    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    /// Generic error
    #[error("Voice error: {0}")]
    Other(String),
}

impl From<VoiceError> for zoey_core::ZoeyError {
    fn from(err: VoiceError) -> Self {
        zoey_core::ZoeyError::other(err.to_string())
    }
}
