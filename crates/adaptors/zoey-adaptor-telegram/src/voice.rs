//! Telegram Voice Support
//!
//! Handles voice message (audio note) generation and TTS integration for Telegram.
//! Supports both TTS (text-to-speech) for sending voice messages and
//! STT (speech-to-text) for transcribing received voice messages.
//! Respects voice configuration from character XML.

#[cfg(any(feature = "voice", feature = "voice-whisper", feature = "voice-unmute"))]
use tracing::{info, warn};

#[cfg(feature = "voice")]
use zoey_provider_voice::VoicePlugin;

/// Voice configuration from character XML
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    /// Whether voice is enabled
    pub enabled: bool,
    /// TTS engine to use (openai, elevenlabs, local)
    pub engine: String,
    /// TTS model (tts-1, tts-1-hd, eleven_turbo_v2_5, etc.)
    pub model: String,
    /// Voice ID for TTS
    pub voice_id: String,
    /// Voice name (for display)
    pub voice_name: String,
    /// Speaking speed (0.25 to 4.0)
    pub speed: f32,
    /// Output audio format (ogg is preferred for Telegram voice messages)
    pub output_format: String,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Enable streaming for low latency
    pub streaming: bool,
    /// ElevenLabs stability (0.0 to 1.0)
    pub stability: Option<f32>,
    /// ElevenLabs similarity boost (0.0 to 1.0)
    pub similarity_boost: Option<f32>,
    /// Local TTS endpoint
    pub local_endpoint: Option<String>,
    /// Trigger phrases that initiate voice mode
    pub triggers: Vec<String>,
    /// Telegram-specific settings
    pub telegram: TelegramVoiceSettings,
}

/// Telegram-specific voice settings
#[derive(Debug, Clone)]
pub struct TelegramVoiceSettings {
    /// Send voice messages automatically for all responses
    pub auto_voice: bool,
    /// Maximum text length for voice synthesis (longer texts are split)
    pub max_text_length: usize,
    /// Send text alongside voice message
    pub include_text: bool,
    /// Convert received voice messages to text (STT)
    pub transcribe_voice: bool,
}

impl Default for TelegramVoiceSettings {
    fn default() -> Self {
        Self {
            auto_voice: false,
            max_text_length: 4096,
            include_text: false,
            transcribe_voice: false,
        }
    }
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: "openai".to_string(),
            model: "tts-1".to_string(),
            voice_id: "shimmer".to_string(),
            voice_name: "Shimmer".to_string(),
            speed: 1.0,
            output_format: "opus".to_string(), // Telegram prefers opus in ogg container
            sample_rate: 48000,
            streaming: true,
            stability: Some(0.5),
            similarity_boost: Some(0.75),
            local_endpoint: None,
            triggers: default_triggers(),
            telegram: TelegramVoiceSettings::default(),
        }
    }
}

/// Default voice trigger phrases
fn default_triggers() -> Vec<String> {
    vec![
        "voice".to_string(),
        "speak".to_string(),
        "say this".to_string(),
        "read aloud".to_string(),
        "read this".to_string(),
        "audio".to_string(),
        "voice message".to_string(),
        "voice note".to_string(),
        "send voice".to_string(),
        "talk to me".to_string(),
        "speak to me".to_string(),
        "/voice".to_string(),
        "/speak".to_string(),
        "/tts".to_string(),
    ]
}

impl VoiceConfig {
    /// Parse voice configuration from character settings (serde_json::Value)
    pub fn from_character_settings(settings: &serde_json::Value) -> Self {
        let voice = settings
            .get("voice")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        if voice.is_null() {
            return Self::default();
        }

        let enabled = voice
            .get("enabled")
            .and_then(|v| v.as_bool())
            .or_else(|| {
                voice
                    .get("enabled")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "true")
            })
            .unwrap_or(false);

        let engine = voice
            .get("engine")
            .and_then(|v| v.as_str())
            .unwrap_or("openai")
            .to_string();

        let model = voice
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("tts-1")
            .to_string();

        let voice_id = voice
            .get("voice_id")
            .and_then(|v| v.as_str())
            .unwrap_or("shimmer")
            .to_string();

        let voice_name = voice
            .get("voice_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Shimmer")
            .to_string();

        let speed = voice
            .get("speed")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .or_else(|| {
                voice
                    .get("speed")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(1.0);

        let output_format = voice
            .get("output_format")
            .and_then(|v| v.as_str())
            .unwrap_or("opus")
            .to_string();

        let sample_rate = voice
            .get("sample_rate")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .or_else(|| {
                voice
                    .get("sample_rate")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(48000);

        let streaming = voice
            .get("streaming")
            .and_then(|v| v.as_bool())
            .or_else(|| {
                voice
                    .get("streaming")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "true")
            })
            .unwrap_or(true);

        let stability = voice
            .get("stability")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .or_else(|| {
                voice
                    .get("stability")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            });

        let similarity_boost = voice
            .get("similarity_boost")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .or_else(|| {
                voice
                    .get("similarity_boost")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            });

        let local_endpoint = voice
            .get("local_endpoint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Parse triggers
        let triggers = voice
            .get("triggers")
            .and_then(|t| t.get("trigger"))
            .and_then(|arr| {
                if arr.is_array() {
                    arr.as_array().map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                            .collect()
                    })
                } else if arr.is_string() {
                    arr.as_str().map(|s| vec![s.to_lowercase()])
                } else {
                    None
                }
            })
            .unwrap_or_else(default_triggers);

        // Parse Telegram-specific settings
        let telegram_settings = voice
            .get("telegram")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let telegram = TelegramVoiceSettings {
            auto_voice: telegram_settings
                .get("auto_voice")
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    telegram_settings
                        .get("auto_voice")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "true")
                })
                .unwrap_or(false),
            max_text_length: telegram_settings
                .get("max_text_length")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .or_else(|| {
                    telegram_settings
                        .get("max_text_length")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(4096),
            include_text: telegram_settings
                .get("include_text")
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    telegram_settings
                        .get("include_text")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "true")
                })
                .unwrap_or(false),
            transcribe_voice: telegram_settings
                .get("transcribe_voice")
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    telegram_settings
                        .get("transcribe_voice")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "true")
                })
                .unwrap_or(false),
        };

        Self {
            enabled,
            engine,
            model,
            voice_id,
            voice_name,
            speed,
            output_format,
            sample_rate,
            streaming,
            stability,
            similarity_boost,
            local_endpoint,
            triggers,
            telegram,
        }
    }

    /// Check if a message contains a voice trigger phrase
    pub fn is_voice_trigger(&self, message: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let msg_lower = message.to_lowercase();
        self.triggers
            .iter()
            .any(|trigger| msg_lower.contains(trigger))
    }

    /// Check if a message is a voice command
    pub fn is_voice_command(&self, message: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let msg_lower = message.to_lowercase().trim().to_string();
        msg_lower.starts_with("/voice")
            || msg_lower.starts_with("/speak")
            || msg_lower.starts_with("/tts")
    }

    /// Extract text to synthesize from a voice command
    /// e.g., "/voice Hello world" -> "Hello world"
    pub fn extract_voice_text(&self, message: &str) -> Option<String> {
        let msg = message.trim();
        for prefix in &["/voice ", "/speak ", "/tts "] {
            if msg.to_lowercase().starts_with(prefix) {
                let text = msg[prefix.len()..].trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }
}

/// Voice manager for handling Telegram voice messages
pub struct VoiceManager {
    /// Voice configuration
    pub config: VoiceConfig,
    /// TTS plugin instance (when voice feature is enabled)
    #[cfg(feature = "voice")]
    tts: Option<VoicePlugin>,
}

impl VoiceManager {
    /// Create a new voice manager
    pub fn new(config: VoiceConfig) -> Self {
        #[cfg(feature = "voice")]
        let tts = if config.enabled {
            Some(Self::create_tts_plugin(&config))
        } else {
            None
        };

        Self {
            config,
            #[cfg(feature = "voice")]
            tts,
        }
    }

    #[cfg(feature = "voice")]
    fn create_tts_plugin(config: &VoiceConfig) -> VoicePlugin {
        match config.engine.as_str() {
            "elevenlabs" => VoicePlugin::with_elevenlabs(None),
            "local" => {
                let endpoint = config
                    .local_endpoint
                    .clone()
                    .unwrap_or_else(|| "http://localhost:5000".to_string());
                VoicePlugin::with_local(endpoint)
            }
            _ => VoicePlugin::with_openai(None), // Default to OpenAI
        }
    }

    /// Check if voice is enabled and configured
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Synthesize text to speech audio
    #[cfg(feature = "voice")]
    pub async fn synthesize(&self, text: &str) -> Result<Vec<u8>, String> {
        let tts = self
            .tts
            .as_ref()
            .ok_or_else(|| "TTS not initialized".to_string())?;

        // Truncate text if too long
        let text = if text.len() > self.config.telegram.max_text_length {
            warn!(
                "Text too long for TTS ({} chars), truncating to {}",
                text.len(),
                self.config.telegram.max_text_length
            );
            &text[..self.config.telegram.max_text_length]
        } else {
            text
        };

        info!(
            engine = %self.config.engine,
            voice = %self.config.voice_id,
            text_len = %text.len(),
            "Synthesizing speech"
        );

        let audio = tts
            .synthesize(text)
            .await
            .map_err(|e| format!("TTS synthesis failed: {}", e))?;

        info!(
            audio_size = %audio.data.len(),
            format = ?audio.format,
            "Speech synthesis complete"
        );

        Ok(audio.data.to_vec())
    }

    /// Synthesize text and return audio data suitable for Telegram voice message
    /// Returns the audio bytes and estimated duration in seconds
    #[cfg(feature = "voice")]
    pub async fn synthesize_for_telegram(&self, text: &str) -> Result<(Vec<u8>, u32), String> {
        let audio_data = self.synthesize(text).await?;

        // Estimate duration based on audio size
        // For MP3 at typical speech bitrate (~20-32 kbps): ~3KB per second
        let estimated_duration = (audio_data.len() as f64 / 3000.0).ceil() as u32;

        Ok((audio_data, estimated_duration.max(1)))
    }

    /// Check if STT (transcription) is available
    #[cfg(any(feature = "voice-whisper", feature = "voice-unmute"))]
    pub fn can_transcribe(&self) -> bool {
        self.config.enabled && self.config.telegram.transcribe_voice
    }

    /// Check if STT is available (stub when no STT features)
    #[cfg(not(any(feature = "voice-whisper", feature = "voice-unmute")))]
    pub fn can_transcribe(&self) -> bool {
        false
    }

    /// Download a voice message from Telegram
    /// Returns the audio bytes
    #[cfg(any(feature = "voice-whisper", feature = "voice-unmute"))]
    pub async fn download_voice_message(
        bot: &teloxide::Bot,
        file_id: &str,
    ) -> Result<Vec<u8>, String> {
        use teloxide::prelude::*;

        // Get file info from Telegram
        let file = bot
            .get_file(file_id)
            .await
            .map_err(|e| format!("Failed to get file info: {}", e))?;

        // Build the download URL
        let token = bot.token();
        let url = format!(
            "https://api.telegram.org/file/bot{}/{}",
            token, file.path
        );

        // Download using reqwest
        let response = reqwest::get(&url)
            .await
            .map_err(|e| format!("Failed to download file: {}", e))?;

        let data = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read file bytes: {}", e))?
            .to_vec();

        info!(
            file_id = %file_id,
            size = %data.len(),
            "Downloaded voice message"
        );

        Ok(data)
    }

    /// Transcribe a voice message from Telegram
    /// Takes OGG/Opus audio data and returns transcribed text
    #[cfg(feature = "voice-whisper")]
    pub async fn transcribe_voice_message(&self, audio_data: &[u8]) -> Result<String, String> {
        use zoey_provider_voice::{AudioData, AudioFormat, VoicePlugin, WhisperModel};
        use bytes::Bytes;

        if !self.can_transcribe() {
            return Err("Voice transcription not enabled".to_string());
        }

        info!(
            audio_size = %audio_data.len(),
            "Transcribing voice message"
        );

        // Convert OGG/Opus to PCM samples
        let pcm_samples = Self::decode_ogg_opus(audio_data)?;
        
        if pcm_samples.is_empty() {
            return Err("No audio data after decoding".to_string());
        }

        // Convert i16 samples to bytes
        let pcm_bytes: Vec<u8> = pcm_samples
            .iter()
            .flat_map(|&s| s.to_le_bytes())
            .collect();

        let audio = AudioData {
            data: Bytes::from(pcm_bytes),
            format: AudioFormat::Pcm,
            sample_rate: 16000, // After resampling
            duration_ms: Some((pcm_samples.len() as u64 * 1000) / 16000),
            character_count: 0,
        };

        // Create Whisper plugin and transcribe
        let plugin = VoicePlugin::with_whisper(WhisperModel::Base);
        
        let result = plugin
            .transcribe(&audio)
            .await
            .map_err(|e| format!("Transcription failed: {}", e))?;

        info!(
            text_len = %result.text.len(),
            "Voice transcription complete"
        );

        Ok(result.text)
    }

    /// Transcribe using Unmute engine
    #[cfg(all(feature = "voice-unmute", not(feature = "voice-whisper")))]
    pub async fn transcribe_voice_message(&self, audio_data: &[u8]) -> Result<String, String> {
        use zoey_provider_voice::{AudioData, AudioFormat, VoicePlugin};
        use bytes::Bytes;

        if !self.can_transcribe() {
            return Err("Voice transcription not enabled".to_string());
        }

        // Convert OGG/Opus to PCM
        let pcm_samples = Self::decode_ogg_opus(audio_data)?;
        
        let pcm_bytes: Vec<u8> = pcm_samples
            .iter()
            .flat_map(|&s| s.to_le_bytes())
            .collect();

        let audio = AudioData {
            data: Bytes::from(pcm_bytes),
            format: AudioFormat::Pcm,
            sample_rate: 16000,
            duration_ms: Some((pcm_samples.len() as u64 * 1000) / 16000),
            character_count: 0,
        };

        // Use Unmute for transcription
        let endpoint = self.config.local_endpoint
            .as_deref()
            .unwrap_or("ws://localhost:8000");
        let plugin = VoicePlugin::with_unmute(endpoint);
        
        let result = plugin
            .transcribe(&audio)
            .await
            .map_err(|e| format!("Transcription failed: {}", e))?;

        Ok(result.text)
    }

    /// Decode OGG/Opus audio to PCM samples at 16kHz mono
    #[cfg(any(feature = "voice-whisper", feature = "voice-unmute"))]
    fn decode_ogg_opus(data: &[u8]) -> Result<Vec<i16>, String> {
        use std::io::Cursor;

        // Use lewton for OGG/Vorbis or opus decoder
        // Telegram voice messages are typically OGG with Opus codec
        let cursor = Cursor::new(data.to_vec());
        
        // Try to decode as OGG/Vorbis first (lewton)
        match lewton::inside_ogg::OggStreamReader::new(cursor) {
            Ok(mut reader) => {
                let sample_rate = reader.ident_hdr.audio_sample_rate;
                let channels = reader.ident_hdr.audio_channels as usize;
                
                let mut all_samples = Vec::new();
                
                while let Ok(Some(packet)) = reader.read_dec_packet_itl() {
                    all_samples.extend(packet);
                }
                
                // Convert to mono if stereo
                let mono_samples: Vec<i16> = if channels > 1 {
                    all_samples
                        .chunks(channels)
                        .map(|chunk| {
                            let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                            (sum / channels as i32) as i16
                        })
                        .collect()
                } else {
                    all_samples
                };
                
                // Resample to 16kHz if needed
                if sample_rate != 16000 {
                    Ok(Self::resample(&mono_samples, sample_rate, 16000))
                } else {
                    Ok(mono_samples)
                }
            }
            Err(e) => {
                // If lewton fails, the file might be Opus-encoded
                // For now, return an error - a proper Opus decoder would be needed
                warn!(error = %e, "Failed to decode OGG audio, may be Opus-encoded");
                Err(format!("Failed to decode OGG audio: {}. Opus codec not yet supported.", e))
            }
        }
    }

    /// Simple linear resampling
    #[cfg(any(feature = "voice-whisper", feature = "voice-unmute"))]
    fn resample(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
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
                let sample = samples[idx] as f64 * (1.0 - frac) + samples[idx + 1] as f64 * frac;
                resampled.push(sample as i16);
            } else if idx < samples.len() {
                resampled.push(samples[idx]);
            }
        }

        resampled
    }
}

// Stub implementations when voice feature is disabled
#[cfg(not(feature = "voice"))]
impl VoiceManager {
    pub async fn synthesize(&self, _text: &str) -> Result<Vec<u8>, String> {
        Err("Voice feature not enabled. Compile with --features voice".to_string())
    }

    pub async fn synthesize_for_telegram(&self, _text: &str) -> Result<(Vec<u8>, u32), String> {
        Err("Voice feature not enabled. Compile with --features voice".to_string())
    }
}

// STT stub when no STT features
#[cfg(all(feature = "voice", not(any(feature = "voice-whisper", feature = "voice-unmute"))))]
impl VoiceManager {
    pub async fn transcribe_voice_message(&self, _audio_data: &[u8]) -> Result<String, String> {
        Err("STT not available. Compile with --features voice-whisper or voice-unmute".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_triggers() {
        let triggers = default_triggers();
        assert!(triggers.contains(&"voice".to_string()));
        assert!(triggers.contains(&"/voice".to_string()));
        assert!(triggers.contains(&"/tts".to_string()));
    }

    #[test]
    fn test_voice_trigger_detection() {
        let mut config = VoiceConfig::default();
        config.enabled = true;

        assert!(config.is_voice_trigger("Please read this aloud"));
        assert!(config.is_voice_trigger("Send me a voice message"));
        assert!(config.is_voice_trigger("/voice hello"));
        assert!(!config.is_voice_trigger("Hello, how are you?"));
    }

    #[test]
    fn test_voice_command_detection() {
        let mut config = VoiceConfig::default();
        config.enabled = true;

        assert!(config.is_voice_command("/voice hello"));
        assert!(config.is_voice_command("/speak test"));
        assert!(config.is_voice_command("/tts message"));
        assert!(!config.is_voice_command("voice hello")); // needs slash
        assert!(!config.is_voice_command("hello /voice")); // needs to start with it
    }

    #[test]
    fn test_extract_voice_text() {
        let config = VoiceConfig::default();

        assert_eq!(
            config.extract_voice_text("/voice Hello world"),
            Some("Hello world".to_string())
        );
        assert_eq!(
            config.extract_voice_text("/speak Test message"),
            Some("Test message".to_string())
        );
        assert_eq!(
            config.extract_voice_text("/tts  Some text  "),
            Some("Some text".to_string())
        );
        assert_eq!(config.extract_voice_text("/voice"), None); // No text after command
        assert_eq!(config.extract_voice_text("Hello"), None); // Not a command
    }

    #[test]
    fn test_voice_config_parsing() {
        let settings = serde_json::json!({
            "voice": {
                "enabled": "true",
                "engine": "elevenlabs",
                "model": "eleven_turbo_v2_5",
                "voice_id": "21m00Tcm4TlvDq8ikWAM",
                "voice_name": "Rachel",
                "speed": "1.0",
                "telegram": {
                    "auto_voice": "false",
                    "max_text_length": "2048",
                    "include_text": "true"
                },
                "triggers": {
                    "trigger": ["hello voice", "start talking"]
                }
            }
        });

        let config = VoiceConfig::from_character_settings(&settings);
        assert!(config.enabled);
        assert_eq!(config.engine, "elevenlabs");
        assert_eq!(config.model, "eleven_turbo_v2_5");
        assert_eq!(config.voice_id, "21m00Tcm4TlvDq8ikWAM");
        assert!(!config.telegram.auto_voice);
        assert_eq!(config.telegram.max_text_length, 2048);
        assert!(config.telegram.include_text);
    }
}
