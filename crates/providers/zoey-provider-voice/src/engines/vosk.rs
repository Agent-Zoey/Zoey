//! Vosk STT Engine
//!
//! Fast, local speech-to-text using Vosk.
//! ~100-200ms latency - much faster than Whisper!
//!
//! Models are downloaded from alphacephei.com on first use.

#[cfg(feature = "vosk-stt")]
use async_trait::async_trait;
#[cfg(feature = "vosk-stt")]
use std::path::PathBuf;
#[cfg(feature = "vosk-stt")]
use std::sync::Arc;
#[cfg(feature = "vosk-stt")]
use std::time::Instant;
#[cfg(feature = "vosk-stt")]
use tokio::sync::RwLock;
#[cfg(feature = "vosk-stt")]
use tracing::{debug, info};
#[cfg(feature = "vosk-stt")]
use vosk::{Model, Recognizer};
#[cfg(feature = "vosk-stt")]
use zoey_core::Result;

#[cfg(feature = "vosk-stt")]
use crate::types::{
    AudioData, AudioFormat, TranscriptionConfig, TranscriptionResult,
    TranscriptionStream, TranscriptionChunk, SpeechEngineType, VoiceError,
};

/// Vosk model options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoskModel {
    /// Small English model (~40MB, fastest)
    EnSmall,
    /// Large English model (~1.8GB, most accurate)
    EnLarge,
    /// Generic small model (~40MB)
    Small,
}

#[cfg(feature = "vosk-stt")]
impl VoskModel {
    /// Get the download URL for this model
    pub fn download_url(&self) -> &'static str {
        match self {
            Self::EnSmall => "https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip",
            Self::EnLarge => "https://alphacephei.com/vosk/models/vosk-model-en-us-0.22.zip",
            Self::Small => "https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip",
        }
    }

    /// Get the model directory name after extraction
    pub fn dir_name(&self) -> &'static str {
        match self {
            Self::EnSmall => "vosk-model-small-en-us-0.15",
            Self::EnLarge => "vosk-model-en-us-0.22",
            Self::Small => "vosk-model-small-en-us-0.15",
        }
    }
}

impl Default for VoskModel {
    fn default() -> Self {
        Self::EnSmall // Fastest by default
    }
}

/// Default cache directory for Vosk models
#[cfg(feature = "vosk-stt")]
fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vosk")
}

/// Vosk speech-to-text engine - FAST local recognition
#[cfg(feature = "vosk-stt")]
pub struct VoskEngine {
    /// Vosk model
    model: Arc<RwLock<Option<Model>>>,
    /// Model type
    model_type: VoskModel,
    /// Path to model directory
    model_path: PathBuf,
    /// Sample rate (16000 for Vosk)
    sample_rate: f32,
    /// Whether model is loaded
    loaded: Arc<RwLock<bool>>,
}

#[cfg(feature = "vosk-stt")]
impl VoskEngine {
    /// Create a new Vosk engine with the specified model
    /// Model will be downloaded on first use if not present
    pub fn new(model_type: VoskModel) -> Self {
        let cache_dir = default_cache_dir();
        let model_path = cache_dir.join(model_type.dir_name());

        Self {
            model: Arc::new(RwLock::new(None)),
            model_type,
            model_path,
            sample_rate: 16000.0,
            loaded: Arc::new(RwLock::new(false)),
        }
    }

    /// Create with a specific model path
    pub fn with_model_path(model_path: PathBuf) -> Self {
        Self {
            model: Arc::new(RwLock::new(None)),
            model_type: VoskModel::EnSmall,
            model_path,
            sample_rate: 16000.0,
            loaded: Arc::new(RwLock::new(false)),
        }
    }

    /// Check if model exists locally
    pub fn model_exists(&self) -> bool {
        self.model_path.exists() && self.model_path.is_dir()
    }

    /// Download and extract the model if not present
    pub async fn ensure_model(&self) -> Result<()> {
        if self.model_exists() {
            debug!("Vosk model already exists at {:?}", self.model_path);
            return Ok(());
        }

        info!("Downloading Vosk model: {:?}", self.model_type);
        
        let url = self.model_type.download_url();
        let cache_dir = self.model_path.parent().unwrap_or(&self.model_path);
        
        // Create cache directory
        tokio::fs::create_dir_all(cache_dir).await?;
        
        // Download the zip file
        let zip_path = cache_dir.join("model.zip");
        let response = reqwest::get(url).await
            .map_err(|e| VoiceError::ModelDownloadError(e.to_string()))?;
        let bytes = response.bytes().await
            .map_err(|e| VoiceError::ModelDownloadError(e.to_string()))?;
        tokio::fs::write(&zip_path, &bytes).await?;
        
        // Extract using system unzip (simpler than adding zip crate)
        let output = tokio::process::Command::new("unzip")
            .arg("-o")
            .arg(&zip_path)
            .arg("-d")
            .arg(cache_dir)
            .output()
            .await?;
        
        if !output.status.success() {
            return Err(VoiceError::ModelDownloadError(format!(
                "Failed to extract: {}",
                String::from_utf8_lossy(&output.stderr)
            )).into());
        }
        
        // Clean up zip file
        let _ = tokio::fs::remove_file(&zip_path).await;
        
        info!("Vosk model downloaded and extracted to {:?}", self.model_path);
        Ok(())
    }

    /// Load the model into memory
    pub async fn load_model(&self) -> Result<()> {
        // Check if already loaded
        if *self.loaded.read().await {
            return Ok(());
        }
        
        // Ensure model is downloaded
        self.ensure_model().await?;
        
        let mut model_guard = self.model.write().await;
        if model_guard.is_some() {
            return Ok(());
        }

        info!("Loading Vosk model from {:?}", self.model_path);
        let start = Instant::now();
        
        let model_path = self.model_path.clone();
        let model = tokio::task::spawn_blocking(move || {
            Model::new(model_path.to_str().unwrap())
        })
        .await
        .map_err(|e| VoiceError::ModelError(e.to_string()))?
        .ok_or_else(|| VoiceError::ModelError("Failed to load Vosk model".to_string()))?;
        
        *model_guard = Some(model);
        *self.loaded.write().await = true;
        
        info!("Vosk model loaded in {:?}", start.elapsed());
        Ok(())
    }

    /// Transcribe audio data to text (internal method)
    async fn transcribe_audio(&self, audio: &AudioData) -> Result<TranscriptionResult> {
        // Ensure model is loaded
        self.load_model().await?;
        
        let start = Instant::now();
        
        // Convert audio to samples
        let samples = self.prepare_audio(audio)?;
        let sample_rate = self.sample_rate;
        let model_path = self.model_path.clone();
        
        // Do everything in a blocking task since Vosk isn't thread-safe
        let text = tokio::task::spawn_blocking(move || {
            // Create a new model instance for this transcription
            // This is less efficient but thread-safe
            let model = Model::new(model_path.to_str().unwrap())
                .ok_or_else(|| VoiceError::NotReady("Failed to load model".to_string()))?;
            
            let mut recognizer = Recognizer::new(&model, sample_rate)
                .ok_or_else(|| VoiceError::TranscriptionError("Failed to create recognizer".to_string()))?;
            
            recognizer.set_words(true);
            
            // Feed audio in chunks for streaming-like behavior
            for chunk in samples.chunks(4096) {
                recognizer.accept_waveform(chunk);
            }
            
            // Get final result - use single() since it's the common case
            let result = recognizer.final_result();
            let text = result.single()
                .map(|r| r.text.to_string())
                .unwrap_or_default();
            
            Ok::<String, VoiceError>(text)
        })
        .await
        .map_err(|e| VoiceError::TranscriptionError(e.to_string()))??;
        
        let elapsed = start.elapsed();
        info!(
            latency_ms = elapsed.as_millis(),
            text_len = text.len(),
            "Vosk transcription complete"
        );
        
        Ok(TranscriptionResult {
            text: text.trim().to_string(),
            language: Some("en".to_string()),
            language_confidence: None,
            confidence: None,
            duration_ms: Some(elapsed.as_millis() as u64),
            segments: Vec::new(),
            processing_time_ms: Some(elapsed.as_millis() as u64),
        })
    }

    /// Prepare audio for Vosk (convert to i16 samples at 16kHz)
    fn prepare_audio(&self, audio: &AudioData) -> Result<Vec<i16>> {
        // If it's already raw PCM, use directly
        if audio.format == AudioFormat::Pcm {
            // Convert bytes to i16 samples
            let samples: Vec<i16> = audio.data
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
            
            // Resample if needed (Discord is 48kHz, Vosk needs 16kHz)
            if audio.sample_rate != 16000 {
                return Ok(self.resample(&samples, audio.sample_rate, 16000));
            }
            return Ok(samples);
        }
        
        // For WAV files, parse the header and extract samples
        if audio.format == AudioFormat::Wav {
            return self.parse_wav(&audio.data);
        }
        
        Err(VoiceError::UnsupportedFormat(format!("{:?}", audio.format)).into())
    }

    /// Parse WAV file and extract i16 samples
    fn parse_wav(&self, data: &[u8]) -> Result<Vec<i16>> {
        use std::io::Cursor;
        let cursor = Cursor::new(data);
        let mut reader = hound::WavReader::new(cursor)
            .map_err(|e| VoiceError::AudioError(format!("Failed to parse WAV: {}", e)))?;
        
        let spec = reader.spec();
        let samples: Vec<i16> = reader.samples::<i16>()
            .filter_map(|s| s.ok())
            .collect();
        
        // Convert to mono if stereo
        let mono = if spec.channels == 2 {
            samples.chunks(2).map(|c| ((c[0] as i32 + c[1] as i32) / 2) as i16).collect()
        } else {
            samples
        };
        
        // Resample if needed
        if spec.sample_rate != 16000 {
            Ok(self.resample(&mono, spec.sample_rate, 16000))
        } else {
            Ok(mono)
        }
    }

    /// Simple linear resampling
    fn resample(&self, samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
        let ratio = from_rate as f64 / to_rate as f64;
        let new_len = (samples.len() as f64 / ratio) as usize;
        
        (0..new_len)
            .map(|i| {
                let src_idx = (i as f64 * ratio) as usize;
                samples.get(src_idx).copied().unwrap_or(0)
            })
            .collect()
    }
}

#[cfg(feature = "vosk-stt")]
#[async_trait]
impl crate::SpeechEngine for VoskEngine {
    fn name(&self) -> &str {
        "vosk"
    }

    async fn transcribe(&self, audio: &AudioData, _config: &TranscriptionConfig) -> Result<TranscriptionResult> {
        self.transcribe_audio(audio).await
    }

    async fn transcribe_stream(&self, audio: &AudioData, config: &TranscriptionConfig) -> Result<TranscriptionStream> {
        // For now, just do a single transcription (streaming would require more work)
        let (tx, rx) = crate::types::create_transcription_stream(1);
        let result = self.transcribe(audio, config).await;
        
        tokio::spawn(async move {
            match result {
                Ok(transcription) => {
                    let _ = tx.send(Ok(TranscriptionChunk {
                        text: transcription.text,
                        is_final: true,
                        timestamp_ms: None,
                        confidence: transcription.confidence,
                    })).await;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vosk_model_urls() {
        assert!(VoskModel::EnSmall.download_url().contains("alphacephei"));
        assert!(VoskModel::EnLarge.download_url().contains("alphacephei"));
    }
}
