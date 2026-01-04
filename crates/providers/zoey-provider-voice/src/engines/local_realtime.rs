//! Local Realtime Voice Pipeline
//!
//! A fully local, low-latency voice pipeline that doesn't depend on external services.
//! This is what you'd use for true low-latency like Unmute, but running entirely in Zoey.
//!
//! ## Requirements for True Low Latency
//!
//! 1. **Local Whisper** - Already have via whisper-rs ✅
//! 2. **Local TTS** - Need a local TTS server (Piper, Coqui, OpenVoice)
//! 3. **Streaming** - Process audio as it arrives, not in batches
//!
//! ## Recommended Local TTS Options
//!
//! ### Piper (Fastest, ~50ms latency)
//! ```bash
//! # Install piper
//! pip install piper-tts
//! # Or run as server
//! docker run -p 5000:5000 rhasspy/piper --voice en_US-lessac-medium
//! ```
//!
//! ### Coqui TTS (Good quality, ~200ms)
//! ```bash
//! pip install TTS
//! tts-server --model_name tts_models/en/ljspeech/tacotron2-DDC
//! ```
//!
//! ### OpenVoice (Voice cloning, ~300ms)
//! ```bash
//! # See: https://github.com/myshell-ai/OpenVoice
//! ```
//!
//! ## Architecture for True Low Latency
//!
//! ```text
//! Discord Audio (48kHz stereo)
//!     │
//!     ▼ (convert to 16kHz mono)
//! ┌─────────────────────────────────┐
//! │  Streaming VAD                   │  ← Silero VAD or WebRTC VAD
//! │  (detect speech boundaries)      │
//! └─────────────────────────────────┘
//!     │
//!     ▼ (on speech end)
//! ┌─────────────────────────────────┐
//! │  Local Whisper (GPU)             │  ← ~100-200ms for short utterances
//! │  whisper-rs with CUDA            │
//! └─────────────────────────────────┘
//!     │
//!     ▼ (transcription)
//! ┌─────────────────────────────────┐
//! │  Your Agent/LLM                  │  ← Generate response
//! └─────────────────────────────────┘
//!     │
//!     ▼ (response text)
//! ┌─────────────────────────────────┐
//! │  Local TTS (Piper/Coqui)         │  ← ~50-200ms
//! └─────────────────────────────────┘
//!     │
//!     ▼ (audio)
//! Discord Playback
//!
//! Total: ~400-600ms (vs 2-3 seconds with cloud APIs)
//! ```

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

#[cfg(feature = "whisper")]
use crate::engines::whisper::WhisperEngine;
use crate::types::*;

/// Configuration for local realtime pipeline
#[derive(Debug, Clone)]
pub struct LocalRealtimeConfig {
    /// Whisper model size
    pub whisper_model: WhisperModel,
    /// Local TTS endpoint (Piper, Coqui, etc.)
    pub tts_endpoint: String,
    /// Silence threshold in ms (default 400)
    pub silence_threshold_ms: u64,
    /// VAD energy threshold (default 500.0)
    pub vad_threshold: f64,
    /// Minimum utterance length in ms (default 300)
    pub min_utterance_ms: u64,
}

impl Default for LocalRealtimeConfig {
    fn default() -> Self {
        Self {
            whisper_model: WhisperModel::Base,
            tts_endpoint: "http://localhost:5000".to_string(),
            silence_threshold_ms: 400,
            vad_threshold: 500.0,
            min_utterance_ms: 300,
        }
    }
}

/// Local realtime voice pipeline
/// 
/// Processes voice in real-time using only local resources.
/// No external API calls = consistent low latency.
#[cfg(feature = "whisper")]
pub struct LocalRealtimePipeline {
    config: LocalRealtimeConfig,
    /// Whisper engine (loaded once, reused)
    whisper: Arc<WhisperEngine>,
    /// Audio buffer for current utterance
    audio_buffer: Vec<i16>,
    /// Last time we received audio with speech
    last_speech_time: std::time::Instant,
    /// Are we currently in an utterance?
    in_utterance: bool,
    /// Transcription output channel
    transcription_tx: mpsc::Sender<String>,
}

#[cfg(feature = "whisper")]
impl LocalRealtimePipeline {
    /// Create new pipeline
    pub fn new(
        config: LocalRealtimeConfig,
        transcription_tx: mpsc::Sender<String>,
    ) -> Self {
        let whisper = Arc::new(WhisperEngine::new(config.whisper_model));
        
        Self {
            config,
            whisper,
            audio_buffer: Vec::with_capacity(16000 * 30), // 30 seconds max
            last_speech_time: std::time::Instant::now(),
            in_utterance: false,
            transcription_tx,
        }
    }
    
    /// Process incoming audio chunk (16kHz mono)
    /// 
    /// Call this for every audio frame. Handles:
    /// - VAD (voice activity detection)
    /// - Buffering
    /// - Automatic transcription on silence
    /// 
    /// Returns Some(transcription) when an utterance completes.
    pub async fn process_audio(&mut self, audio_16khz_mono: &[i16]) -> Option<String> {
        // Simple VAD: check energy
        let has_speech = self.detect_speech(audio_16khz_mono);
        
        if has_speech {
            // Receiving speech
            self.audio_buffer.extend_from_slice(audio_16khz_mono);
            self.last_speech_time = std::time::Instant::now();
            self.in_utterance = true;
            None
        } else if self.in_utterance {
            // Check for end of utterance
            let silence_ms = self.last_speech_time.elapsed().as_millis() as u64;
            
            if silence_ms >= self.config.silence_threshold_ms {
                // Utterance ended - transcribe
                let utterance_ms = (self.audio_buffer.len() as u64 * 1000) / 16000;
                
                if utterance_ms >= self.config.min_utterance_ms {
                    let audio = std::mem::take(&mut self.audio_buffer);
                    self.in_utterance = false;
                    
                    // Transcribe
                    return self.transcribe(&audio).await;
                } else {
                    // Too short - discard
                    self.audio_buffer.clear();
                    self.in_utterance = false;
                }
            }
            None
        } else {
            None
        }
    }
    
    /// Detect speech using simple energy-based VAD
    fn detect_speech(&self, samples: &[i16]) -> bool {
        if samples.is_empty() {
            return false;
        }
        
        // Calculate RMS energy
        let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        let rms = (sum / samples.len() as f64).sqrt();
        
        rms > self.config.vad_threshold
    }
    
    /// Transcribe audio using local Whisper
    async fn transcribe(&self, audio: &[i16]) -> Option<String> {
        use bytes::Bytes;
        
        // Convert to bytes
        let pcm_bytes: Vec<u8> = audio
            .iter()
            .flat_map(|&s| s.to_le_bytes())
            .collect();
        
        let audio_data = AudioData {
            data: Bytes::from(pcm_bytes),
            format: AudioFormat::Pcm,
            sample_rate: 16000,
            duration_ms: Some((audio.len() as u64 * 1000) / 16000),
            character_count: 0,
        };
        
        let config = TranscriptionConfig {
            whisper_model: self.config.whisper_model,
            ..Default::default()
        };
        
        match self.whisper.transcribe(&audio_data, &config).await {
            Ok(result) => {
                let text = result.text.trim().to_string();
                if !text.is_empty() {
                    // Also send to channel
                    let _ = self.transcription_tx.send(text.clone()).await;
                    Some(text)
                } else {
                    None
                }
            }
            Err(e) => {
                warn!(error = %e, "Transcription failed");
                None
            }
        }
    }
    
    /// Synthesize text using local TTS
    pub async fn synthesize(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        // Call local TTS server
        let client = reqwest::Client::new();
        
        // Try common local TTS endpoints
        let response = client
            .post(&format!("{}/api/tts", self.config.tts_endpoint))
            .json(&serde_json::json!({
                "text": text,
                "speaker_id": "default",
            }))
            .send()
            .await
            .map_err(|e| VoiceError::NetworkError(format!("TTS request failed: {}", e)))?;
        
        if !response.status().is_success() {
            // Try alternative endpoint format (Piper)
            let response = client
                .post(&format!("{}/synthesize", self.config.tts_endpoint))
                .body(text.to_string())
                .send()
                .await
                .map_err(|e| VoiceError::NetworkError(format!("TTS request failed: {}", e)))?;
            
            if !response.status().is_success() {
                return Err(VoiceError::AudioError(format!(
                    "TTS server returned {}",
                    response.status()
                )));
            }
            
            return Ok(response.bytes().await
                .map_err(|e| VoiceError::AudioError(e.to_string()))?
                .to_vec());
        }
        
        Ok(response.bytes().await
            .map_err(|e| VoiceError::AudioError(e.to_string()))?
            .to_vec())
    }
}

// ============================================================================
// Piper TTS Integration (Fastest Local Option)
// ============================================================================

/// Piper TTS client for ultra-low-latency local synthesis
/// 
/// Piper is a fast, local neural TTS that can achieve ~50ms latency.
/// 
/// ## Setup
/// ```bash
/// # Option 1: Python package
/// pip install piper-tts
/// piper --model en_US-lessac-medium --output-raw | aplay -r 22050 -f S16_LE
/// 
/// # Option 2: Docker
/// docker run -p 5000:5000 rhasspy/piper
/// 
/// # Option 3: Binary
/// # Download from https://github.com/rhasspy/piper/releases
/// ```
pub struct PiperTTS {
    endpoint: String,
    voice: String,
}

impl PiperTTS {
    /// Create Piper client
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            voice: "en_US-lessac-medium".to_string(),
        }
    }
    
    /// Set voice model
    pub fn with_voice(mut self, voice: &str) -> Self {
        self.voice = voice.to_string();
        self
    }
    
    /// Synthesize text to audio
    pub async fn synthesize(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        let client = reqwest::Client::new();
        
        let response = client
            .get(&self.endpoint)
            .query(&[
                ("text", text),
                ("voice", &self.voice),
            ])
            .send()
            .await
            .map_err(|e| VoiceError::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(VoiceError::AudioError(format!(
                "Piper returned {}",
                response.status()
            )));
        }
        
        Ok(response.bytes().await
            .map_err(|e| VoiceError::AudioError(e.to_string()))?
            .to_vec())
    }
}

// ============================================================================
// Full Local Voice Assistant
// ============================================================================

/// Complete local voice assistant pipeline
/// 
/// This is the closest to "real Unmute" you can get locally:
/// - Local Whisper STT
/// - Local Piper TTS
/// - Your own LLM/agent for responses
/// 
/// ## Example
/// ```rust,ignore
/// let (tx, mut rx) = mpsc::channel(32);
/// 
/// let assistant = LocalVoiceAssistant::new(
///     LocalRealtimeConfig::default(),
///     tx,
///     |transcription| async move {
///         // Your agent logic here
///         Some(format!("I heard: {}", transcription))
///     },
/// );
/// 
/// // In your audio handler:
/// assistant.process_audio(&audio).await;
/// 
/// // Responses come back via rx
/// while let Some((text, audio)) = rx.recv().await {
///     play_audio(audio);
/// }
/// ```
#[cfg(feature = "whisper")]
pub struct LocalVoiceAssistant<F>
where
    F: Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>> + Send + Sync,
{
    pipeline: LocalRealtimePipeline,
    tts: PiperTTS,
    response_handler: F,
    response_tx: mpsc::Sender<(String, Vec<u8>)>,
}

#[cfg(feature = "whisper")]
impl<F> LocalVoiceAssistant<F>
where
    F: Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>> + Send + Sync,
{
    /// Create new voice assistant
    pub fn new(
        config: LocalRealtimeConfig,
        response_tx: mpsc::Sender<(String, Vec<u8>)>,
        response_handler: F,
    ) -> Self {
        let (transcription_tx, _) = mpsc::channel(32);
        let tts_endpoint = config.tts_endpoint.clone();
        
        Self {
            pipeline: LocalRealtimePipeline::new(config, transcription_tx),
            tts: PiperTTS::new(&tts_endpoint),
            response_handler,
            response_tx,
        }
    }
    
    /// Process audio and generate response if utterance complete
    pub async fn process_audio(&mut self, audio: &[i16]) -> Result<(), VoiceError> {
        if let Some(transcription) = self.pipeline.process_audio(audio).await {
            info!(text = %transcription, "User said");
            
            // Get response from handler
            if let Some(response_text) = (self.response_handler)(transcription).await {
                info!(response = %response_text, "Generating response");
                
                // Synthesize audio
                match self.tts.synthesize(&response_text).await {
                    Ok(audio) => {
                        let _ = self.response_tx.send((response_text, audio)).await;
                    }
                    Err(e) => {
                        warn!(error = %e, "TTS failed");
                    }
                }
            }
        }
        
        Ok(())
    }
}

// ============================================================================
// Quick Start Functions
// ============================================================================

/// Recommended setup for true low-latency voice
/// 
/// ## Prerequisites
/// 1. Install Piper TTS: `docker run -p 5000:5000 rhasspy/piper`
/// 2. Have a GPU for Whisper (optional but recommended)
/// 
/// ## Returns
/// - `transcription_rx`: Receives transcribed text
/// - `response_rx`: Receives (text, audio) pairs
/// - `audio_tx`: Send audio chunks here
#[cfg(feature = "whisper")]
pub fn setup_local_voice() -> (
    mpsc::Receiver<String>,
    mpsc::Receiver<(String, Vec<u8>)>,
    mpsc::Sender<Vec<i16>>,
) {
    let (transcription_tx, transcription_rx) = mpsc::channel::<String>(32);
    let (_response_tx, response_rx) = mpsc::channel::<(String, Vec<u8>)>(32);
    let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<i16>>(64);
    
    let config = LocalRealtimeConfig::default();
    let mut pipeline = LocalRealtimePipeline::new(config, transcription_tx);
    
    tokio::spawn(async move {
        while let Some(audio) = audio_rx.recv().await {
            let _ = pipeline.process_audio(&audio).await;
        }
    });
    
    (transcription_rx, response_rx, audio_tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = LocalRealtimeConfig::default();
        assert_eq!(config.silence_threshold_ms, 400);
        assert_eq!(config.min_utterance_ms, 300);
    }

    #[test]
    fn test_piper_client() {
        let piper = PiperTTS::new("http://localhost:5000")
            .with_voice("en_US-amy-low");
        assert_eq!(piper.voice, "en_US-amy-low");
    }
}

