//! Whisper STT Engine
//!
//! Speech-to-text using whisper.cpp via the whisper-rs crate.
//! Supports automatic model downloading and various model sizes.
//!
//! Models are downloaded from HuggingFace on first use and cached locally.

#[cfg(feature = "whisper")]
use async_trait::async_trait;
#[cfg(feature = "whisper")]
use bytes::Bytes;
#[cfg(feature = "whisper")]
use std::path::PathBuf;
#[cfg(feature = "whisper")]
use std::sync::Arc;
#[cfg(feature = "whisper")]
use std::time::Instant;
#[cfg(feature = "whisper")]
use tokio::sync::RwLock;
#[cfg(feature = "whisper")]
use tracing::{debug, error, info, warn};
#[cfg(feature = "whisper")]
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
#[cfg(feature = "whisper")]
use zoey_core::Result;

#[cfg(feature = "whisper")]
use crate::types::*;

/// HuggingFace model repository base URL
#[cfg(feature = "whisper")]
const HF_MODEL_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Default cache directory for whisper models
#[cfg(feature = "whisper")]
fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("whisper")
}

/// Whisper speech-to-text engine
#[cfg(feature = "whisper")]
pub struct WhisperEngine {
    /// Whisper context (loaded model)
    ctx: Arc<RwLock<Option<WhisperContext>>>,
    /// Model size
    model_size: WhisperModel,
    /// Path to model file
    model_path: PathBuf,
    /// Whether the model is loaded
    loaded: Arc<RwLock<bool>>,
}

#[cfg(feature = "whisper")]
impl WhisperEngine {
    /// Create a new Whisper engine with the specified model size
    /// Model will be downloaded on first use if not present
    pub fn new(model_size: WhisperModel) -> Self {
        let cache_dir = default_cache_dir();
        let model_path = cache_dir.join(model_size.ggml_filename());

        Self {
            ctx: Arc::new(RwLock::new(None)),
            model_size,
            model_path,
            loaded: Arc::new(RwLock::new(false)),
        }
    }

    /// Create with a specific model file path
    pub fn with_model_path(model_path: PathBuf, model_size: WhisperModel) -> Self {
        Self {
            ctx: Arc::new(RwLock::new(None)),
            model_size,
            model_path,
            loaded: Arc::new(RwLock::new(false)),
        }
    }

    /// Get the model file path
    pub fn model_path(&self) -> &PathBuf {
        &self.model_path
    }

    /// Get the model size
    pub fn model_size(&self) -> WhisperModel {
        self.model_size
    }

    /// Check if model file exists locally
    pub fn model_exists(&self) -> bool {
        self.model_path.exists()
    }

    /// Download the model file if not present
    pub async fn ensure_model(&self) -> Result<()> {
        if self.model_exists() {
            debug!("Whisper model already exists at {:?}", self.model_path);
            return Ok(());
        }

        info!(
            "Downloading Whisper {} model (~{}MB)...",
            self.model_size.as_str(),
            self.model_size.size_mb()
        );

        // Create cache directory if it doesn't exist
        if let Some(parent) = self.model_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                VoiceError::ModelDownloadError(format!("Failed to create cache dir: {}", e))
            })?;
        }

        let url = format!("{}/{}", HF_MODEL_BASE, self.model_size.ggml_filename());
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600)) // 10 min timeout for large models
            .build()
            .map_err(|e| VoiceError::ModelDownloadError(e.to_string()))?;

        let response = client.get(&url).send().await.map_err(|e| {
            VoiceError::ModelDownloadError(format!("Failed to download from {}: {}", url, e))
        })?;

        if !response.status().is_success() {
            return Err(VoiceError::ModelDownloadError(format!(
                "Download failed with status: {}",
                response.status()
            ))
            .into());
        }

        // Download with progress logging
        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;
        let mut last_log = Instant::now();

        let bytes = response.bytes().await.map_err(|e| {
            VoiceError::ModelDownloadError(format!("Failed to read response: {}", e))
        })?;

        downloaded = bytes.len() as u64;
        
        if total_size > 0 {
            info!(
                "Downloaded {:.1}MB / {:.1}MB (100%)",
                downloaded as f64 / 1_000_000.0,
                total_size as f64 / 1_000_000.0
            );
        }

        // Write to file
        tokio::fs::write(&self.model_path, &bytes).await.map_err(|e| {
            VoiceError::ModelDownloadError(format!("Failed to write model file: {}", e))
        })?;

        info!("Whisper model downloaded successfully to {:?}", self.model_path);
        Ok(())
    }

    /// Load the model into memory
    pub async fn load_model(&self) -> Result<()> {
        // Ensure model is downloaded
        self.ensure_model().await?;

        let mut loaded = self.loaded.write().await;
        if *loaded {
            return Ok(());
        }

        info!("Loading Whisper {} model...", self.model_size.as_str());
        let start = Instant::now();

        let model_path = self.model_path.clone();
        
        // Load model in blocking task (whisper-rs is sync)
        let ctx = tokio::task::spawn_blocking(move || {
            let params = WhisperContextParameters::default();
            WhisperContext::new_with_params(model_path.to_str().unwrap(), params)
        })
        .await
        .map_err(|e| VoiceError::ModelError(format!("Task join error: {}", e)))?
        .map_err(|e| VoiceError::ModelError(format!("Failed to load model: {}", e)))?;

        *self.ctx.write().await = Some(ctx);
        *loaded = true;

        info!(
            "Whisper model loaded in {:.2}s",
            start.elapsed().as_secs_f64()
        );
        Ok(())
    }

    /// Convert audio bytes to f32 samples at 16kHz mono
    /// Whisper requires 16kHz mono f32 samples
    fn convert_audio_to_samples(audio: &AudioData) -> Result<Vec<f32>> {
        match audio.format {
            AudioFormat::Wav => Self::decode_wav(&audio.data),
            AudioFormat::Pcm => Self::decode_pcm(&audio.data, audio.sample_rate),
            _ => Err(VoiceError::UnsupportedFormat(format!(
                "Whisper requires WAV or PCM input, got {}",
                audio.format.as_str()
            ))
            .into()),
        }
    }

    /// Decode WAV file to f32 samples
    #[cfg(feature = "whisper")]
    fn decode_wav(data: &Bytes) -> Result<Vec<f32>> {
        use std::io::Cursor;
        
        let cursor = Cursor::new(data.as_ref());
        let mut reader = hound::WavReader::new(cursor)
            .map_err(|e| VoiceError::AudioError(format!("Failed to read WAV: {}", e)))?;

        let spec = reader.spec();
        let sample_rate = spec.sample_rate;
        let channels = spec.channels as usize;

        // Read samples based on bit depth
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .filter_map(|s| s.ok())
                .collect(),
            hound::SampleFormat::Int => {
                let max_val = (1i32 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / max_val)
                    .collect()
            }
        };

        // Convert to mono if stereo
        let mono_samples: Vec<f32> = if channels > 1 {
            samples
                .chunks(channels)
                .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                .collect()
        } else {
            samples
        };

        // Resample to 16kHz if needed
        if sample_rate != 16000 {
            Ok(Self::resample(&mono_samples, sample_rate, 16000))
        } else {
            Ok(mono_samples)
        }
    }

    /// Decode raw PCM to f32 samples
    fn decode_pcm(data: &Bytes, sample_rate: u32) -> Result<Vec<f32>> {
        // Assume 16-bit signed PCM mono
        let samples: Vec<f32> = data
            .chunks_exact(2)
            .map(|chunk| {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                sample as f32 / 32768.0
            })
            .collect();

        // Resample to 16kHz if needed
        if sample_rate != 16000 {
            Ok(Self::resample(&samples, sample_rate, 16000))
        } else {
            Ok(samples)
        }
    }

    /// Simple linear interpolation resampling
    fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
        if from_rate == to_rate {
            return samples.to_vec();
        }

        let ratio = from_rate as f64 / to_rate as f64;
        let new_len = (samples.len() as f64 / ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);

        for i in 0..new_len {
            let src_idx = i as f64 * ratio;
            let idx = src_idx as usize;
            let frac = src_idx - idx as f64;

            if idx + 1 < samples.len() {
                let sample = samples[idx] * (1.0 - frac as f32) + samples[idx + 1] * frac as f32;
                resampled.push(sample);
            } else if idx < samples.len() {
                resampled.push(samples[idx]);
            }
        }

        resampled
    }

    /// Run transcription on samples
    async fn transcribe_samples(
        &self,
        samples: Vec<f32>,
        config: &TranscriptionConfig,
    ) -> Result<TranscriptionResult> {
        // Ensure model is loaded
        self.load_model().await?;

        let ctx_guard = self.ctx.read().await;
        let ctx = ctx_guard.as_ref().ok_or_else(|| {
            VoiceError::NotReady("Whisper model not loaded".to_string())
        })?;

        let language = config.language.clone();
        let timestamps = config.timestamps;
        let temperature = config.temperature;
        let beam_size = config.beam_size;

        // Clone context reference for blocking task
        let ctx_ptr = ctx as *const WhisperContext as usize;
        
        // Capture samples length before move
        let samples_len = samples.len();
        
        // Run transcription in blocking task
        let start = Instant::now();
        
        let result = tokio::task::spawn_blocking(move || {
            // SAFETY: We hold the read lock, so ctx is valid
            let ctx = unsafe { &*(ctx_ptr as *const WhisperContext) };
            
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            
            // Set language if specified
            if let Some(ref lang) = language {
                params.set_language(Some(lang));
            } else {
                params.set_language(None); // Auto-detect
            }

            // Configure parameters
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(timestamps);
            params.set_token_timestamps(timestamps);
            params.set_temperature(temperature);
            
            // Suppress non-speech tokens
            params.set_suppress_blank(true);
            params.set_suppress_non_speech_tokens(true);

            // Create state and run
            let mut state = ctx.create_state()
                .map_err(|e| VoiceError::TranscriptionError(format!("Failed to create state: {}", e)))?;

            state.full(params, &samples)
                .map_err(|e| VoiceError::TranscriptionError(format!("Transcription failed: {}", e)))?;

            // Extract results
            let num_segments = state.full_n_segments()
                .map_err(|e| VoiceError::TranscriptionError(format!("Failed to get segments: {}", e)))?;

            let mut text = String::new();
            let mut segments = Vec::new();

            for i in 0..num_segments {
                let segment_text = state.full_get_segment_text(i)
                    .map_err(|e| VoiceError::TranscriptionError(format!("Failed to get segment {}: {}", i, e)))?;
                
                text.push_str(&segment_text);

                if timestamps {
                    let start_ts = state.full_get_segment_t0(i)
                        .map_err(|e| VoiceError::TranscriptionError(e.to_string()))?;
                    let end_ts = state.full_get_segment_t1(i)
                        .map_err(|e| VoiceError::TranscriptionError(e.to_string()))?;

                    segments.push(TranscriptionSegment {
                        text: segment_text,
                        start_ms: (start_ts * 10) as u64, // Whisper uses centiseconds
                        end_ms: (end_ts * 10) as u64,
                        confidence: None,
                        speaker_id: None,
                    });
                }
            }

            Ok::<_, VoiceError>((text.trim().to_string(), segments))
        })
        .await
        .map_err(|e| VoiceError::TranscriptionError(format!("Task join error: {}", e)))??;

        let processing_time = start.elapsed().as_millis() as u64;
        let audio_duration = (samples_len as f64 / 16000.0 * 1000.0) as u64;

        debug!(
            "Transcription completed in {}ms for {}ms of audio ({}x realtime)",
            processing_time,
            audio_duration,
            audio_duration as f64 / processing_time as f64
        );

        Ok(TranscriptionResult {
            text: result.0,
            language: config.language.clone(),
            language_confidence: None,
            confidence: None,
            duration_ms: Some(audio_duration),
            segments: result.1,
            processing_time_ms: Some(processing_time),
        })
    }
}

#[cfg(feature = "whisper")]
#[async_trait]
impl SpeechEngine for WhisperEngine {
    fn name(&self) -> &str {
        "whisper"
    }

    async fn transcribe(
        &self,
        audio: &AudioData,
        config: &TranscriptionConfig,
    ) -> Result<TranscriptionResult> {
        // Check audio duration
        let duration_secs = audio.duration_ms.unwrap_or(0) / 1000;
        if let Some(max_duration) = config.max_duration_secs {
            if duration_secs > max_duration as u64 {
                return Err(VoiceError::AudioTooLong {
                    duration_secs: duration_secs as u32,
                    max_secs: max_duration,
                }
                .into());
            }
        }

        // Convert audio to samples
        let samples = Self::convert_audio_to_samples(audio)?;

        if samples.is_empty() {
            return Ok(TranscriptionResult::new(String::new()));
        }

        // Run transcription
        self.transcribe_samples(samples, config).await
    }

    async fn transcribe_stream(
        &self,
        audio: &AudioData,
        config: &TranscriptionConfig,
    ) -> Result<TranscriptionStream> {
        // For Whisper, we don't have true streaming - process whole audio
        // and stream back results segment by segment
        let (tx, rx) = create_transcription_stream(32);

        let result = self.transcribe(audio, config).await;

        tokio::spawn(async move {
            match result {
                Ok(transcription) => {
                    if transcription.segments.is_empty() {
                        // No segments, send full text as one chunk
                        let _ = tx
                            .send(Ok(TranscriptionChunk {
                                text: transcription.text,
                                is_final: true,
                                timestamp_ms: None,
                                confidence: transcription.confidence,
                            }))
                            .await;
                    } else {
                        // Stream segments
                        for (i, segment) in transcription.segments.iter().enumerate() {
                            let is_final = i == transcription.segments.len() - 1;
                            let _ = tx
                                .send(Ok(TranscriptionChunk {
                                    text: segment.text.clone(),
                                    is_final,
                                    timestamp_ms: Some(segment.start_ms),
                                    confidence: segment.confidence,
                                }))
                                .await;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        });

        Ok(rx)
    }

    async fn is_ready(&self) -> bool {
        *self.loaded.read().await
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Wav, AudioFormat::Pcm]
    }

    fn max_duration_secs(&self) -> u32 {
        300 // 5 minutes
    }

    fn supported_languages(&self) -> Vec<&'static str> {
        // Whisper supports 99 languages
        vec![
            "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar",
            "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu",
            "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa",
            "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn",
            "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc",
            "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn",
            "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw",
            "su",
        ]
    }
}

// Stub implementation when whisper feature is disabled
#[cfg(not(feature = "whisper"))]
pub struct WhisperEngine;

#[cfg(not(feature = "whisper"))]
impl WhisperEngine {
    pub fn new(_model_size: super::super::WhisperModel) -> Self {
        Self
    }
}

#[cfg(all(test, feature = "whisper"))]
mod tests {
    use super::*;

    #[test]
    fn test_whisper_model_sizes() {
        assert_eq!(WhisperModel::Tiny.as_str(), "tiny");
        assert_eq!(WhisperModel::Base.size_mb(), 142);
        assert_eq!(WhisperModel::Large.memory_mb(), 5000);
    }

    #[test]
    fn test_whisper_model_from_str() {
        assert_eq!("tiny".parse::<WhisperModel>().unwrap(), WhisperModel::Tiny);
        assert_eq!("large-v2".parse::<WhisperModel>().unwrap(), WhisperModel::LargeV2);
        assert!("invalid".parse::<WhisperModel>().is_err());
    }

    #[test]
    fn test_resample() {
        // Test upsampling from 8kHz to 16kHz
        let samples = vec![0.0, 1.0, 0.0, -1.0];
        let resampled = WhisperEngine::resample(&samples, 8000, 16000);
        assert!(resampled.len() > samples.len());
    }
}

