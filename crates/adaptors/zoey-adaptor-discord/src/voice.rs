//! Discord Voice Support
//!
//! Handles voice channel operations and TTS integration for Discord.
//! Respects voice configuration from character XML.
//!
//! ## Auto-Starting Voice Server
//! When `engine: piper` is configured, the VoiceManager will automatically
//! spin up a local voice server. If `stt_engine: vosk` is also configured,
//! it uses the unified voice-server (Vosk STT + Piper TTS via Unmute WebSocket).
//! Otherwise, it falls back to piper-server (TTS only via HTTP).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

#[cfg(feature = "voice")]
use songbird::{
    input::{File as SongbirdFile, Input, RawAdapter, YoutubeDl},
    Call, Event, EventContext, EventHandler as SongbirdEventHandler, Songbird, TrackEvent,
};

#[cfg(feature = "voice")]
use serenity::model::id::{ChannelId, GuildId, UserId};

use std::future::Future;
use std::pin::Pin;
use std::process::Child;

/// Callback type for handling voice transcriptions
/// Takes (user_id, transcribed_text) and returns Option<response_text>
pub type TranscriptionCallback = Box<
    dyn Fn(u64, String) -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync
>;

/// Voice configuration from character XML
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    /// Whether voice is enabled
    pub enabled: bool,
    /// TTS engine to use (openai, elevenlabs, local, piper)
    pub engine: String,
    /// STT engine to use (whisper, vosk) - vosk is ~10x faster
    pub stt_engine: String,
    /// STT WebSocket endpoint (for hybrid mode with local Whisper)
    pub stt_endpoint: Option<String>,
    /// TTS model (tts-1, tts-1-hd, eleven_turbo_v2_5, etc.)
    pub model: String,
    /// Voice ID for TTS
    pub voice_id: String,
    /// Voice name (for display)
    pub voice_name: String,
    /// Speaking speed (0.25 to 4.0)
    pub speed: f32,
    /// Output audio format
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
    /// Discord-specific settings
    pub discord: DiscordVoiceSettings,
}

/// Discord-specific voice settings
#[derive(Debug, Clone)]
pub struct DiscordVoiceSettings {
    /// Auto-join voice channel when triggered
    pub auto_join_voice: bool,
    /// Leave voice channel when alone
    pub leave_when_alone: bool,
    /// Idle timeout in seconds before leaving
    pub idle_timeout_seconds: u64,
    /// Speak text responses in voice channel
    pub speak_responses: bool,
    /// Enable speech-to-text listening
    pub listen_enabled: bool,
}

impl Default for DiscordVoiceSettings {
    fn default() -> Self {
        Self {
            auto_join_voice: true,
            leave_when_alone: true,
            idle_timeout_seconds: 300,
            speak_responses: true,
            listen_enabled: false,
        }
    }
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: "openai".to_string(),
            stt_engine: "whisper".to_string(),  // whisper or vosk
            stt_endpoint: None,  // Optional WebSocket endpoint for local STT server
            model: "tts-1".to_string(),
            voice_id: "shimmer".to_string(),
            voice_name: "Shimmer".to_string(),
            speed: 1.0,
            output_format: "mp3".to_string(),
            sample_rate: 24000,
            streaming: true,
            stability: Some(0.5),
            similarity_boost: Some(0.75),
            local_endpoint: None,
            triggers: default_triggers(),
            discord: DiscordVoiceSettings::default(),
        }
    }
}

/// Default voice trigger phrases
fn default_triggers() -> Vec<String> {
    vec![
        "let's chat".to_string(),
        "lets chat".to_string(),
        "let chat".to_string(),
        "voice chat".to_string(),
        "call me".to_string(),
        "start a call".to_string(),
        "join voice".to_string(),
        "talk to me".to_string(),
        "speak to me".to_string(),
        "can we talk".to_string(),
        "want to talk".to_string(),
        "voice mode".to_string(),
        "audio mode".to_string(),
        "read this aloud".to_string(),
        "say this".to_string(),
        "speak this".to_string(),
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

        // STT engine: vosk (~100-200ms) or whisper (~2-3s)
        let stt_engine = voice
            .get("stt_engine")
            .and_then(|v| v.as_str())
            .unwrap_or("whisper")  // Default to whisper for compatibility
            .to_string();

        // Optional STT WebSocket endpoint for hybrid mode (local STT + cloud TTS)
        let stt_endpoint = voice
            .get("stt_endpoint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

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
            .unwrap_or("mp3")
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
            .unwrap_or(24000);

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

        // Parse Discord-specific settings
        let discord_settings = voice
            .get("discord")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let discord = DiscordVoiceSettings {
            auto_join_voice: discord_settings
                .get("auto_join_voice")
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    discord_settings
                        .get("auto_join_voice")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "true")
                })
                .unwrap_or(true),
            leave_when_alone: discord_settings
                .get("leave_when_alone")
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    discord_settings
                        .get("leave_when_alone")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "true")
                })
                .unwrap_or(true),
            idle_timeout_seconds: discord_settings
                .get("idle_timeout_seconds")
                .and_then(|v| v.as_u64())
                .or_else(|| {
                    discord_settings
                        .get("idle_timeout_seconds")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(300),
            speak_responses: discord_settings
                .get("speak_responses")
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    discord_settings
                        .get("speak_responses")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "true")
                })
                .unwrap_or(true),
            listen_enabled: discord_settings
                .get("listen_enabled")
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    discord_settings
                        .get("listen_enabled")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "true")
                })
                .unwrap_or(false),
        };

        Self {
            enabled,
            engine,
            stt_engine,
            stt_endpoint,
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
            discord,
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

    /// Check if a message requests TTS (read aloud)
    pub fn is_tts_request(&self, message: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let msg_lower = message.to_lowercase();
        msg_lower.contains("read this aloud")
            || msg_lower.contains("say this")
            || msg_lower.contains("speak this")
            || msg_lower.contains("read it out")
            || msg_lower.contains("read aloud")
    }
}

/// Voice session state for a guild
#[derive(Debug)]
pub struct VoiceSession {
    /// Guild ID
    pub guild_id: u64,
    /// Voice channel ID the bot is in
    pub channel_id: u64,
    /// When the bot joined
    pub joined_at: Instant,
    /// Last activity timestamp
    pub last_activity: Instant,
    /// Whether the bot is currently speaking
    pub is_speaking: bool,
    /// Users in the voice channel (for leave_when_alone detection)
    pub users_in_channel: HashSet<u64>,
}

impl VoiceSession {
    pub fn new(guild_id: u64, channel_id: u64) -> Self {
        let now = Instant::now();
        Self {
            guild_id,
            channel_id,
            joined_at: now,
            last_activity: now,
            is_speaking: false,
            users_in_channel: HashSet::new(),
        }
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Check if session has been idle too long
    pub fn is_idle(&self, timeout_secs: u64) -> bool {
        self.last_activity.elapsed() > Duration::from_secs(timeout_secs)
    }

    /// Check if bot is alone in the channel
    pub fn is_alone(&self) -> bool {
        self.users_in_channel.is_empty()
    }
}

/// Voice manager for handling Discord voice connections
pub struct VoiceManager {
    /// Voice configuration
    pub config: VoiceConfig,
    /// Active voice sessions by guild ID
    pub sessions: Arc<RwLock<std::collections::HashMap<u64, VoiceSession>>>,
    /// Songbird voice client (when voice feature is enabled)
    #[cfg(feature = "voice")]
    pub songbird: Option<Arc<Songbird>>,
    /// Lock to prevent overlapping TTS - one speak at a time per guild
    #[cfg(feature = "voice")]
    speaking_locks: Arc<RwLock<std::collections::HashMap<u64, Arc<tokio::sync::Mutex<()>>>>>,
    /// Piper server process (auto-started when engine is "piper")
    #[cfg(feature = "voice")]
    piper_server: Arc<RwLock<Option<Child>>>,
    /// Unmute dockerless manager (auto-started when engine is "unmute")
    #[cfg(all(feature = "voice", feature = "voice-unmute"))]
    unmute_manager: Arc<RwLock<Option<zoey_provider_voice::UnmuteDockerless>>>,
    /// Persistent TTS connection for low-latency Unmute TTS (avoids per-request connection overhead)
    #[cfg(all(feature = "voice", feature = "voice-unmute"))]
    unmute_tts: Arc<RwLock<Option<zoey_provider_voice::UnmuteTTS>>>,
    /// Moshi streaming client for real-time full-duplex voice (STT + TTS)
    #[cfg(all(feature = "voice", feature = "voice-moshi"))]
    moshi_client: Arc<RwLock<Option<zoey_provider_voice::MoshiStreamingClient>>>,
}

impl VoiceManager {
    /// Create a new voice manager
    pub fn new(config: VoiceConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            #[cfg(feature = "voice")]
            songbird: None,
            #[cfg(feature = "voice")]
            speaking_locks: Arc::new(RwLock::new(std::collections::HashMap::new())),
            #[cfg(feature = "voice")]
            piper_server: Arc::new(RwLock::new(None)),
            #[cfg(all(feature = "voice", feature = "voice-unmute"))]
            unmute_manager: Arc::new(RwLock::new(None)),
            #[cfg(all(feature = "voice", feature = "voice-unmute"))]
            unmute_tts: Arc::new(RwLock::new(None)),
            #[cfg(all(feature = "voice", feature = "voice-moshi"))]
            moshi_client: Arc::new(RwLock::new(None)),
        }
    }

    /// Create with Songbird client
    #[cfg(feature = "voice")]
    pub fn with_songbird(config: VoiceConfig, songbird: Arc<Songbird>) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            songbird: Some(songbird),
            speaking_locks: Arc::new(RwLock::new(std::collections::HashMap::new())),
            piper_server: Arc::new(RwLock::new(None)),
            #[cfg(all(feature = "voice", feature = "voice-unmute"))]
            unmute_manager: Arc::new(RwLock::new(None)),
            #[cfg(all(feature = "voice", feature = "voice-unmute"))]
            unmute_tts: Arc::new(RwLock::new(None)),
            #[cfg(all(feature = "voice", feature = "voice-moshi"))]
            moshi_client: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize voice manager - starts Piper server or Unmute dockerless if needed
    /// 
    /// Call this after creating the VoiceManager to auto-start the TTS/STT
    /// server when engine is "piper" or "unmute".
    /// 
    /// For Unmute: Checks if unmute is already running on the configured endpoint.
    /// If not running, attempts to start dockerless services. If already running,
    /// uses the existing instance.
    #[cfg(feature = "voice")]
    pub async fn init(&self) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }

        // Auto-start Piper server if engine is "piper"
        if self.config.engine == "piper" {
            self.start_piper_server().await?;
        }

        // For Unmute: Check if already running, otherwise start dockerless
        #[cfg(feature = "voice-unmute")]
        if self.config.engine == "unmute" {
            // Get endpoint from config
            let endpoint = self
                .config
                .local_endpoint
                .clone()
                .or_else(|| self.config.stt_endpoint.clone())
                .unwrap_or_else(|| "ws://127.0.0.1:8000".to_string());
            
            // Check if unmute is already running
            if self.check_unmute_health(&endpoint).await {
                info!(
                    endpoint = %endpoint,
                    "Unmute is already running, using existing instance"
                );
                // Don't start dockerless - use existing instance
            } else {
                // Not running, try to start dockerless
                info!("Unmute not detected, attempting to start dockerless services");
                if let Err(e) = self.start_unmute_dockerless().await {
                    warn!(
                        error = %e,
                        endpoint = %endpoint,
                        "Failed to start unmute dockerless, will try to use existing endpoint anyway"
                    );
                    // Continue anyway - might be starting up or using external instance
                }
            }
            
            // Initialize persistent TTS connection for low-latency
            // This avoids ~50-200ms connection overhead per speak() call
            info!(endpoint = %endpoint, "Initializing persistent TTS connection...");
            match zoey_provider_voice::UnmuteTTS::connect(&endpoint, Some(&self.config.voice_id)).await {
                Ok(tts) => {
                    info!("Persistent TTS connection established (saves ~100ms per request)");
                    let mut tts_lock = self.unmute_tts.write().await;
                    *tts_lock = Some(tts);
                }
                Err(e) => {
                    warn!(error = %e, "Failed to establish persistent TTS connection, will use per-request connections");
                }
            }
        }

        // Initialize Moshi if engine is "moshi" - real-time full-duplex voice
        #[cfg(feature = "voice-moshi")]
        if self.config.engine == "moshi" || self.config.stt_engine == "moshi" {
            self.start_moshi_client().await?;
        }

        Ok(())
    }

    /// Check if Unmute is already running by checking health endpoint
    #[cfg(all(feature = "voice", feature = "voice-unmute"))]
    async fn check_unmute_health(&self, endpoint: &str) -> bool {
        // Convert WebSocket URL to HTTP for health check
        let http_url = endpoint
            .replace("ws://", "http://")
            .replace("wss://", "https://")
            .trim_end_matches("/ws")
            .trim_end_matches('/')
            .to_string();
        
        let health_url = format!("{}/health", http_url);
        
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build();
        
        if let Ok(client) = client {
            if let Ok(resp) = client.get(&health_url).send().await {
                if resp.status().is_success() {
                    if let Ok(body) = resp.text().await {
                        // Check if response looks like unmute health check
                        if body.contains("status") || body.contains("ok") || body.contains("stt") || body.contains("tts") {
                            return true;
                        }
                    }
                }
            }
        }
        
        false
    }

    /// Start the embedded Piper TTS server
    #[cfg(feature = "voice")]
    pub async fn start_piper_server(&self) -> Result<(), String> {
        use std::process::{Command, Stdio};

        // Check if already running
        {
            let server = self.piper_server.read().await;
            if server.is_some() {
                info!("Piper server already running");
                return Ok(());
            }
        }

        // Parse endpoint to get port
        let endpoint = self.config.local_endpoint
            .clone()
            .unwrap_or_else(|| "http://localhost:5500".to_string());
        
        let port = endpoint
            .split(':')
            .last()
            .and_then(|s| s.trim_matches('/').parse::<u16>().ok())
            .unwrap_or(5500);

        // Find Piper binary and model
        let piper_paths = [
            std::path::PathBuf::from("voices/piper/piper"),
            std::path::PathBuf::from("crates/providers/zoey-provider-voice/voices/piper/piper"),
            std::path::PathBuf::from("/root/zoey-rust/crates/providers/zoey-provider-voice/voices/piper/piper"),
        ];
        
        // Use voice_id from config or default to amy-low
        let voice_id = if self.config.voice_id.is_empty() {
            "en_US-amy-low".to_string()
        } else {
            self.config.voice_id.clone()
        };
        let model_filename = format!("{}.onnx", voice_id);
        
        let model_paths = [
            std::path::PathBuf::from(format!("voices/models/{}", model_filename)),
            std::path::PathBuf::from(format!("crates/providers/zoey-provider-voice/voices/models/{}", model_filename)),
            std::path::PathBuf::from(format!("/root/zoey-rust/crates/providers/zoey-provider-voice/voices/models/{}", model_filename)),
        ];

        let piper_binary = piper_paths.iter()
            .find(|p| p.exists())
            .cloned();

        let model_file = model_paths.iter()
            .find(|p| p.exists())
            .cloned();

        // Check for voice-server binary (unified Vosk STT + Piper TTS)
        let voice_server_paths = [
            std::path::PathBuf::from("target/release/voice-server"),
            std::path::PathBuf::from("/root/zoey-rust/target/release/voice-server"),
        ];
        
        // Check for piper-server binary (TTS only)
        let piper_server_paths = [
            std::path::PathBuf::from("target/release/piper-server"),
            std::path::PathBuf::from("/root/zoey-rust/target/release/piper-server"),
        ];

        let voice_server_binary = voice_server_paths.iter()
            .find(|p| p.exists())
            .cloned();
        
        let piper_server_binary = piper_server_paths.iter()
            .find(|p| p.exists())
            .cloned();

        // Prefer voice-server when using Vosk STT (unified streaming solution)
        let use_voice_server = self.config.stt_engine.to_lowercase() == "vosk" 
            && voice_server_binary.is_some();
        
        info!(
            stt_engine = %self.config.stt_engine,
            voice_server_exists = voice_server_binary.is_some(),
            piper_server_exists = piper_server_binary.is_some(),
            use_voice_server = %use_voice_server,
            "Voice server selection"
        );
        
        if use_voice_server {
            let server = voice_server_binary.as_ref().unwrap();
            let piper = piper_binary.as_ref();
            let model = model_file.as_ref();
            
            info!(
                server = %server.display(),
                piper = ?piper.map(|p| p.display().to_string()),
                model = ?model.map(|m| m.display().to_string()),
                port = %port,
                "Starting unified voice server (Vosk STT + Piper TTS)"
            );

            let mut cmd = Command::new(server);
            cmd.arg("--port").arg(port.to_string())
               .arg("--host").arg("127.0.0.1");
            
            // Add Piper paths if available
            if let Some(piper) = piper {
                cmd.arg("--piper").arg(piper);
            }
            if let Some(model) = model {
                cmd.arg("--model").arg(model);
            }
            
            let child = cmd
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .env("LD_LIBRARY_PATH", "/usr/local/lib")
                .spawn()
                .map_err(|e| format!("Failed to start voice server: {}", e))?;

            // Store the process handle
            {
                let mut server_lock = self.piper_server.write().await;
                *server_lock = Some(child);
            }

            // Wait for server to be ready
            tokio::time::sleep(Duration::from_millis(1000)).await;

            // Health check
            let client = reqwest::Client::new();
            for _ in 0..15 {
                if let Ok(resp) = client.get(&format!("http://127.0.0.1:{}/health", port)).send().await {
                    if resp.status().is_success() {
                        info!(port = %port, "Unified voice server is ready (Vosk STT + Piper TTS)");
                        return Ok(());
                    }
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }

            warn!("Voice server started but health check failed - it may still be loading");
            return Ok(());
        }

        // Fallback to piper-server (TTS only)
        if let (Some(server), Some(piper), Some(model)) = (piper_server_binary.as_ref(), piper_binary.as_ref(), model_file.as_ref()) {
            info!(
                server = %server.display(),
                piper = %piper.display(),
                model = %model.display(),
                port = %port,
                "Starting Piper TTS server (Rust)"
            );

            let child = Command::new(server)
                .arg("--piper")
                .arg(piper)
                .arg("--model")
                .arg(model)
                .arg("--port")
                .arg(port.to_string())
                .arg("--host")
                .arg("127.0.0.1")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Failed to start Piper server: {}", e))?;

            // Store the process handle
            {
                let mut server_lock = self.piper_server.write().await;
                *server_lock = Some(child);
            }

            // Wait for server to be ready
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Health check
            let client = reqwest::Client::new();
            for _ in 0..10 {
                if let Ok(resp) = client.get(&format!("http://127.0.0.1:{}/health", port)).send().await {
                    if resp.status().is_success() {
                        info!(port = %port, "Piper TTS server is ready");
                        return Ok(());
                    }
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            warn!("Piper server started but health check failed - it may still be loading");
            return Ok(());
        }

        // Fallback: Use local Piper engine directly (no server)
        if let (Some(piper), Some(model)) = (piper_binary, model_file) {
            info!(
                piper = %piper.display(),
                model = %model.display(),
                "Piper server binary not found, will use direct execution"
            );
            // The speak() method will use LocalPiperEngine as fallback
            return Ok(());
        }

        Err("Piper not installed. Run: ./scripts/setup-piper-native.sh".to_string())
    }

    /// Stop the Piper server if running
    #[cfg(feature = "voice")]
    pub async fn stop_piper_server(&self) {
        let mut server = self.piper_server.write().await;
        if let Some(mut child) = server.take() {
            info!("Stopping Piper TTS server");
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Start Unmute dockerless services
    #[cfg(all(feature = "voice", feature = "voice-unmute"))]
    pub async fn start_unmute_dockerless(&self) -> Result<(), String> {
        use zoey_provider_voice::UnmuteDockerless;

        // Check if already running
        {
            let manager = self.unmute_manager.read().await;
            if manager.is_some() {
                info!("Unmute dockerless already running");
                return Ok(());
            }
        }

        // Get unmute directory from config or environment
        let unmute_dir = std::env::var("UNMUTE_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("./unmute"));

        info!(
            unmute_dir = %unmute_dir.display(),
            "Starting Unmute dockerless services..."
        );

        // Build and start manager
        let mut manager = UnmuteDockerless::builder()
            .unmute_dir(&unmute_dir)
            .build()
            .await
            .map_err(|e| format!("Failed to build Unmute dockerless: {}", e))?;

        manager.start_all().await
            .map_err(|e| format!("Failed to start Unmute services: {}", e))?;

        // Store manager
        {
            let mut mgr = self.unmute_manager.write().await;
            *mgr = Some(manager);
        }

        info!("Unmute dockerless services started successfully");
        Ok(())
    }

    /// Stop Unmute dockerless services
    #[cfg(all(feature = "voice", feature = "voice-unmute"))]
    pub async fn stop_unmute_dockerless(&self) {
        let mut manager = self.unmute_manager.write().await;
        if let Some(mut mgr) = manager.take() {
            info!("Stopping Unmute dockerless services");
            if let Err(e) = mgr.stop_all().await {
                warn!(error = %e, "Error stopping Unmute services");
            }
        }
    }

    /// Get Unmute endpoint (from manager if running, or from config)
    /// 
    /// If unmute dockerless manager is running, uses its endpoint.
    /// Otherwise, falls back to config endpoint (which may be an external unmute instance).
    #[cfg(all(feature = "voice", feature = "voice-unmute"))]
    pub async fn get_unmute_endpoint(&self) -> String {
        let manager = self.unmute_manager.read().await;
        if let Some(ref m) = *manager {
            return m.endpoint();
        }
        // Fallback to config (use 127.0.0.1 for consistency)
        self.config.local_endpoint
            .clone()
            .or_else(|| self.config.stt_endpoint.clone())
            .unwrap_or_else(|| "ws://127.0.0.1:8000".to_string())
    }

    /// Start Moshi streaming client for real-time full-duplex voice
    /// 
    /// Moshi provides both STT and TTS in a single connection with ultra-low
    /// latency (~200ms round-trip) using the Mimi neural audio codec.
    #[cfg(all(feature = "voice", feature = "voice-moshi"))]
    pub async fn start_moshi_client(&self) -> Result<(), String> {
        use zoey_provider_voice::{MoshiConfig, MoshiStreamingClient};

        // Check if already connected
        {
            let client = self.moshi_client.read().await;
            if client.is_some() {
                info!("Moshi client already connected");
                return Ok(());
            }
        }

        // Get endpoint from config - Moshi uses wss:// by default on port 8998
        let endpoint = self
            .config
            .local_endpoint
            .clone()
            .or_else(|| self.config.stt_endpoint.clone())
            .unwrap_or_else(|| "localhost:8998".to_string());

        // Clean up endpoint - remove protocol prefix if present
        let endpoint = endpoint
            .trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string();

        info!(
            endpoint = %endpoint,
            "Connecting to Moshi server for full-duplex voice..."
        );

        // Create Moshi config
        let config = MoshiConfig {
            endpoint: endpoint.clone(),
            use_tls: true, // Moshi typically uses HTTPS/WSS
            asr_only: false, // Enable both STT and TTS
            ..Default::default()
        };

        // Create and connect streaming client
        let client = MoshiStreamingClient::new(config);
        
        // Store client (connection happens on first use)
        {
            let mut client_lock = self.moshi_client.write().await;
            *client_lock = Some(client);
        }

        info!(endpoint = %endpoint, "Moshi client initialized (connection on first use)");
        Ok(())
    }

    /// Get Moshi endpoint from config
    #[cfg(all(feature = "voice", feature = "voice-moshi"))]
    pub fn get_moshi_endpoint(&self) -> String {
        self.config
            .local_endpoint
            .clone()
            .or_else(|| self.config.stt_endpoint.clone())
            .unwrap_or_else(|| "localhost:8998".to_string())
    }

    /// Check if Moshi client is connected
    #[cfg(all(feature = "voice", feature = "voice-moshi"))]
    pub async fn is_moshi_connected(&self) -> bool {
        let client = self.moshi_client.read().await;
        client.is_some()
    }

    /// Check if Piper server is running
    #[cfg(feature = "voice")]
    pub async fn is_piper_running(&self) -> bool {
        let server = self.piper_server.read().await;
        server.is_some()
    }
    
    /// Get or create a speaking lock for a guild
    #[cfg(feature = "voice")]
    async fn get_speaking_lock(&self, guild_id: u64) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.speaking_locks.write().await;
        locks.entry(guild_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Check if voice is enabled and configured
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Join a voice channel
    #[cfg(feature = "voice")]
    pub async fn join_channel(self: Arc<Self>, guild_id: u64, channel_id: u64) -> Result<(), String> {
        self.join_channel_with_callback(guild_id, channel_id, None).await
    }

    /// Join a voice channel with optional transcription callback for voice conversations
    #[cfg(feature = "voice")]
    pub async fn join_channel_with_callback(
        self: Arc<Self>,
        guild_id: u64, 
        channel_id: u64,
        transcription_callback: Option<TranscriptionCallback>,
    ) -> Result<(), String> {
        let songbird = self
            .songbird
            .as_ref()
            .ok_or_else(|| "Songbird not initialized".to_string())?;

        let guild = GuildId::new(guild_id);
        let channel = ChannelId::new(channel_id);

        match songbird.join(guild, channel).await {
            Ok(call_lock) => {
                info!(guild_id = %guild_id, channel_id = %channel_id, "Joined voice channel");

                // Register voice receiver for STT if listen_enabled
                #[cfg(any(feature = "voice-whisper", feature = "voice-unmute", feature = "voice-vosk", feature = "voice-moshi"))]
                if self.config.discord.listen_enabled {
                    use songbird::events::Event;
                    
                    let (tx, mut rx) = tokio::sync::mpsc::channel::<(u64, String)>(32);
                    let engine = self.config.engine.as_str();
                    let stt_engine = self.config.stt_engine.as_str();
                    
                    // Determine which STT system to use based on engine/stt_engine config
                    let use_moshi = stt_engine == "moshi" || engine == "moshi";
                    
                    // Use WebSocket STT if stt_endpoint is configured (hybrid mode) or engine is unmute
                    #[cfg(feature = "voice-unmute")]
                    let stt_endpoint = if engine == "unmute" {
                        self.get_unmute_endpoint().await
                    } else {
                        self.config.stt_endpoint.as_deref()
                            .or(self.config.local_endpoint.as_deref())
                            .unwrap_or("ws://127.0.0.1:8765")
                            .to_string()
                    };
                    #[cfg(not(feature = "voice-unmute"))]
                    let stt_endpoint = self.config.stt_endpoint.as_deref()
                        .or(self.config.local_endpoint.as_deref())
                        .unwrap_or("ws://127.0.0.1:8765");
                    
                    // Use RealtimeVoiceReceiver for WebSocket STT (unmute protocol)
                    #[cfg(feature = "voice-unmute")]
                    let use_realtime = !use_moshi && (self.config.stt_endpoint.is_some() || engine == "unmute");
                    #[cfg(not(feature = "voice-unmute"))]
                    let use_realtime = false;
                    
                    let mut call = call_lock.lock().await;
                    
                    // MOSHI: Full-duplex real-time STT/TTS via Moshi protocol
                    #[cfg(feature = "voice-moshi")]
                    if use_moshi {
                        let moshi_endpoint = self.get_moshi_endpoint();
                        let moshi_receiver = std::sync::Arc::new(MoshiVoiceReceiver::new(guild_id, &moshi_endpoint, tx.clone()));
                        let handler = MoshiVoiceHandler { receiver: moshi_receiver.clone() };
                        
                        call.add_global_event(Event::Core(songbird::CoreEvent::VoiceTick), handler);
                        
                        info!(guild_id = %guild_id, endpoint = %moshi_endpoint, "Moshi full-duplex STT/TTS registered");
                    }
                    
                    #[cfg(feature = "voice-unmute")]
                    if use_realtime && !use_moshi {
                        // REALTIME: Stream directly to WebSocket STT server (Whisper/Vosk)
                        let stt_endpoint_str = stt_endpoint.as_str();
                        let realtime_receiver = std::sync::Arc::new(RealtimeVoiceReceiver::new(guild_id, stt_endpoint_str, tx.clone()));
                        let handler = RealtimeVoiceHandler { receiver: realtime_receiver.clone() };
                        
                        call.add_global_event(Event::Core(songbird::CoreEvent::VoiceTick), handler);
                        
                        info!(guild_id = %guild_id, endpoint = %stt_endpoint, "Realtime STT via WebSocket (Whisper server)");
                    }
                    
                    // BUFFERED: Local STT with Vosk/Whisper (only when these features are available)
                    #[cfg(any(feature = "voice-whisper", feature = "voice-vosk", feature = "voice-unmute"))]
                    {
                        // Skip buffered mode if we're using realtime (unmute) or moshi
                        #[allow(unused_mut)]
                        let mut skip_buffered = use_realtime;
                        #[cfg(feature = "voice-moshi")]
                        {
                            skip_buffered = skip_buffered || use_moshi;
                        }
                        
                        if !skip_buffered {
                            let stt_engine = self.config.stt_engine.clone();
                            let receiver = std::sync::Arc::new(VoiceReceiver::new(guild_id, tx.clone(), stt_engine));
                            let handler = VoiceReceiverHandler { receiver: receiver.clone() };
                            
                            call.add_global_event(Event::Core(songbird::CoreEvent::VoiceTick), handler);
                            
                            let receiver_for_speaking = receiver.clone();
                            let speaking_handler = VoiceReceiverHandler { receiver: receiver_for_speaking };
                            call.add_global_event(Event::Core(songbird::CoreEvent::SpeakingStateUpdate), speaking_handler);
                            
                            info!(guild_id = %guild_id, "Voice receiver registered for STT (VoiceTick + SpeakingStateUpdate)");
                            
                            // Spawn task to periodically check for completed utterances
                            let receiver_clone = receiver.clone();
                            tokio::spawn(async move {
                                loop {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                    receiver_clone.check_and_transcribe().await;
                                }
                            });
                        }
                    }
                    
                    drop(call);
                    
                    // Spawn task to handle transcriptions and route to agent
                    if let Some(callback) = transcription_callback {
                        let voice_mgr = Arc::clone(&self);
                        tokio::spawn(async move {
                            while let Some((user_id, text)) = rx.recv().await {
                                info!(user_id = %user_id, text = %text, "Received transcription from voice - routing to agent");
                                
                                // Call the transcription callback to process and respond
                                if let Some(response) = (callback)(user_id, text.clone()).await {
                                    // Speak the response
                                    if let Err(e) = voice_mgr.speak(guild_id, &response).await {
                                        warn!(error = %e, "Failed to speak response");
                                    }
                                }
                            }
                        });
                    } else {
                        // No callback - just log transcriptions
                        tokio::spawn(async move {
                            while let Some((user_id, text)) = rx.recv().await {
                                info!(user_id = %user_id, text = %text, "Received transcription from voice (no callback configured)");
                            }
                        });
                    }
                }

                // Create session
                let mut sessions = self.sessions.write().await;
                sessions.insert(guild_id, VoiceSession::new(guild_id, channel_id));

                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Failed to join voice channel");
                Err(format!("Failed to join voice channel: {}", e))
            }
        }
    }


    /// Leave a voice channel
    #[cfg(feature = "voice")]
    pub async fn leave_channel(&self, guild_id: u64) -> Result<(), String> {
        let songbird = self
            .songbird
            .as_ref()
            .ok_or_else(|| "Songbird not initialized".to_string())?;

        let guild = GuildId::new(guild_id);

        if let Err(e) = songbird.remove(guild).await {
            warn!(error = %e, "Error leaving voice channel");
        }

        // Remove session
        let mut sessions = self.sessions.write().await;
        sessions.remove(&guild_id);

        info!(guild_id = %guild_id, "Left voice channel");
        Ok(())
    }

    /// Speak text in a voice channel using TTS
    /// Uses a per-guild lock to ensure responses are played sequentially
    #[cfg(feature = "voice")]
    pub async fn speak(&self, guild_id: u64, text: &str) -> Result<(), String> {
        use zoey_provider_voice::{AudioFormat, Voice, VoiceConfig as TTSConfig, VoicePlugin};

        // Acquire speaking lock for this guild - ensures sequential playback
        let speaking_lock = self.get_speaking_lock(guild_id).await;
        let _guard = speaking_lock.lock().await;
        
        info!(guild_id = %guild_id, "Acquired speaking lock, starting TTS");

        let songbird = self
            .songbird
            .as_ref()
            .ok_or_else(|| "Songbird not initialized".to_string())?;

        let guild = GuildId::new(guild_id);

        // Get the call handler
        let call_lock = songbird
            .get(guild)
            .ok_or_else(|| "Not in a voice channel".to_string())?;

        // Update session activity
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(&guild_id) {
                session.touch();
                session.is_speaking = true;
            }
        }

        // Create TTS plugin based on config
        let tts = match self.config.engine.as_str() {
            "elevenlabs" => VoicePlugin::with_elevenlabs(None),
            "piper" => {
                // Piper TTS - ultra low latency (~50ms)
                let endpoint = self
                    .config
                    .local_endpoint
                    .clone()
                    .unwrap_or_else(|| "http://localhost:5500".to_string());
                VoicePlugin::with_piper(&endpoint)
            }
            #[cfg(feature = "voice-unmute")]
            "unmute" => {
                // Unmute TTS - GPU-accelerated, streaming WebSocket protocol
                // Use endpoint from dockerless manager if running, otherwise from config
                let endpoint = self.get_unmute_endpoint().await;
                VoicePlugin::with_unmute(&endpoint)
            }
            "supertonic" => {
                // Supertonic TTS - Ultra-fast on-device TTS (~10-50ms latency)
                // Uses ONNX models, runs via HTTP server on port 5080
                let endpoint = self
                    .config
                    .local_endpoint
                    .clone()
                    .unwrap_or_else(|| "http://127.0.0.1:5080".to_string());
                info!(guild_id = %guild_id, endpoint = %endpoint, "Using Supertonic TTS");
                VoicePlugin::with_supertonic(&endpoint)
            }
            #[cfg(feature = "voice-moshi")]
            "moshi" => {
                // Moshi TTS - Full-duplex real-time voice model (~200ms latency)
                // Uses Kyutai's Moshi model with Mimi audio codec
                let endpoint = self
                    .config
                    .local_endpoint
                    .clone()
                    .or_else(|| self.config.stt_endpoint.clone())
                    .unwrap_or_else(|| "localhost:8998".to_string());
                info!(guild_id = %guild_id, endpoint = %endpoint, "Using Moshi TTS");
                VoicePlugin::with_moshi(&endpoint)
            }
            "local" => {
                let endpoint = self
                    .config
                    .local_endpoint
                    .clone()
                    .unwrap_or_else(|| "http://localhost:5000".to_string());
                VoicePlugin::with_local(endpoint)
            }
            _ => VoicePlugin::with_openai(None), // Default to OpenAI
        };

        // For Unmute, use native WebSocket streaming TTS for realtime playback
        // Unmute streams audio chunks as they're generated - play them immediately!
        #[cfg(feature = "voice-unmute")]
        let audio = if self.config.engine == "unmute" {
            use zoey_provider_voice::{AudioData, AudioFormat};
            use bytes::Bytes;
            
            let tts_start = std::time::Instant::now();
            let mut audio_data = Vec::new();
            
            // Try persistent connection first (saves ~100ms connection overhead)
            let use_persistent = {
                let tts_lock = self.unmute_tts.read().await;
                if let Some(ref tts_conn) = *tts_lock {
                    if tts_conn.is_connected().await {
                        true
                    } else {
                        warn!("Persistent TTS connection disconnected");
                        false
                    }
                } else {
                    false
                }
            };
            
            if use_persistent {
                // Use persistent connection - no connection overhead!
                let tts_lock = self.unmute_tts.read().await;
                if let Some(ref tts_conn) = *tts_lock {
                    info!(guild_id = %guild_id, "Using persistent TTS connection (fast path)");
                    
                    match tts_conn.synthesize(text).await {
                        Ok(mut rx) => {
                            let mut chunk_count = 0;
                            let mut first_chunk_time: Option<std::time::Instant> = None;
                            
                            while let Some(chunk_result) = rx.recv().await {
                                match chunk_result {
                                    Ok(data) if data.is_empty() => break, // End marker
                                    Ok(data) => {
                                        if first_chunk_time.is_none() {
                                            first_chunk_time = Some(std::time::Instant::now());
                                            let latency_ms = tts_start.elapsed().as_millis();
                                            info!(
                                                guild_id = %guild_id,
                                                latency_ms = %latency_ms,
                                                "First TTS chunk (persistent connection)"
                                            );
                                        }
                                        audio_data.extend_from_slice(&data);
                                        chunk_count += 1;
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "Persistent TTS chunk error");
                                        break;
                                    }
                                }
                            }
                            
                            info!(
                                guild_id = %guild_id,
                                chunks = %chunk_count,
                                total_bytes = %audio_data.len(),
                                total_latency_ms = %tts_start.elapsed().as_millis(),
                                "TTS complete (persistent connection)"
                            );
                        }
                        Err(e) => {
                            warn!(error = %e, "Persistent TTS request failed, falling back");
                        }
                    }
                }
            }
            
            // Fall back to per-request connection if persistent failed or no audio
            if audio_data.is_empty() {
                info!(guild_id = %guild_id, "Using per-request TTS connection (fallback)");
                
                // Use streaming TTS - chunks arrive via WebSocket as they're generated
                let mut stream = tts
                    .synthesize_stream(text)
                    .await
                    .map_err(|e| format!("Unmute TTS streaming failed: {}", e))?;
                
                let mut chunk_count = 0;
                let mut first_chunk_time: Option<std::time::Instant> = None;
                
                while let Some(chunk_result) = stream.recv().await {
                    match chunk_result {
                        Ok(chunk) => {
                            if first_chunk_time.is_none() && !chunk.data.is_empty() {
                                first_chunk_time = Some(std::time::Instant::now());
                                let latency_ms = chunk.timestamp_ms.unwrap_or(0);
                                info!(
                                    guild_id = %guild_id,
                                    latency_ms = %latency_ms,
                                    "First TTS chunk received from Unmute (new connection)"
                                );
                            }
                            
                            if !chunk.data.is_empty() {
                                audio_data.extend_from_slice(&chunk.data);
                                chunk_count += 1;
                            }
                            
                            if chunk.is_final {
                                break;
                            }
                        }
                        Err(e) => {
                            return Err(format!("TTS stream error: {}", e));
                        }
                    }
                }
                
                if let Some(start) = first_chunk_time {
                    info!(
                        guild_id = %guild_id,
                        chunks = %chunk_count,
                        total_bytes = %audio_data.len(),
                        total_latency_ms = %start.elapsed().as_millis(),
                        "TTS complete (new connection)"
                    );
                }
            }
            
            // Convert to AudioData
            let format = match self.config.output_format.to_lowercase().as_str() {
                "mp3" => AudioFormat::Mp3,
                "opus" => AudioFormat::Opus,
                "aac" => AudioFormat::Aac,
                "flac" => AudioFormat::Flac,
                "wav" => AudioFormat::Wav,
                "pcm" | "pcm16" => AudioFormat::Pcm,
                _ => AudioFormat::Pcm, // Default to PCM for Unmute
            };
            
            AudioData {
                data: Bytes::from(audio_data),
                format,
                sample_rate: self.config.sample_rate,
                duration_ms: None,
                character_count: text.len(),
            }
        } else {
            // Non-Unmute engines
            tts
                .synthesize(text)
                .await
                .map_err(|e| format!("TTS synthesis failed: {}", e))?
        };
        
        #[cfg(not(feature = "voice-unmute"))]
        let audio = if self.config.engine == "unmute" {
            return Err("Unmute feature not enabled".to_string());
        } else {
            // Synthesize speech (non-Unmute engines: OpenAI, ElevenLabs, Piper, etc.)
            tts
                .synthesize(text)
                .await
                .map_err(|e| format!("TTS synthesis failed: {}", e))?
        };

        info!(
            guild_id = %guild_id,
            text_len = %text.len(),
            audio_size = %audio.data.len(),
            audio_format = ?audio.format,
            "Synthesized speech"
        );

        // Play audio in voice channel
        let mut call = call_lock.lock().await;

        info!(audio_size = %audio.data.len(), format = ?audio.format, "Creating audio input from bytes");

        // Handle different audio formats
        let input: Input = match audio.format {
            AudioFormat::Pcm => {
                // Raw PCM audio - wrap in WAV header for symphonia to decode
                // Piper/Unmute outputs 16-bit signed PCM at the configured sample rate (mono)
                let pcm_data = audio.data.to_vec();
                let sample_rate = audio.sample_rate;
                let wav_data = wrap_pcm_in_wav(&pcm_data, sample_rate, 1, 16);
                
                info!(
                    pcm_size = %pcm_data.len(),
                    wav_size = %wav_data.len(),
                    sample_rate = %sample_rate,
                    "Wrapped PCM in WAV header"
                );
                
                let audio_bytes: &'static [u8] = Box::leak(wav_data.into_boxed_slice());
                audio_bytes.into()
            }
            _ => {
                // For encoded formats (MP3, WAV, etc), let symphonia auto-detect
                let audio_bytes: &'static [u8] = Box::leak(audio.data.to_vec().into_boxed_slice());
                audio_bytes.into()
            }
        };

        let _track_handle = call.play_input(input);

        info!(guild_id = %guild_id, "Started playing audio in voice channel");

        // Estimate playback duration and wait for it to complete
        // This keeps the lock held until audio finishes, preventing overlaps
        let audio_size = audio.data.len();
        let bytes_per_second = match audio.format {
            // PCM 16-bit mono at sample_rate
            AudioFormat::Pcm => (audio.sample_rate * 2) as f64,  // 24000Hz * 2 bytes = 48000 bytes/sec
            // MP3/encoded formats are ~3-4KB/s for speech
            _ => 4000.0,
        };
        let duration_secs = (audio_size as f64 / bytes_per_second).max(1.0).ceil() as u64;
        
        info!(guild_id = %guild_id, duration_secs = %duration_secs, audio_bytes = %audio_size, "Waiting for audio playback to complete");
        tokio::time::sleep(Duration::from_secs(duration_secs)).await;

        // Mark as not speaking
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(&guild_id) {
                session.is_speaking = false;
            }
        }
        
        info!(guild_id = %guild_id, "Releasing speaking lock");

        Ok(())
    }

    /// Update user presence in voice channel
    pub async fn update_user_presence(&self, guild_id: u64, user_id: u64, joined: bool) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&guild_id) {
            if joined {
                session.users_in_channel.insert(user_id);
            } else {
                session.users_in_channel.remove(&user_id);
            }
            session.touch();
        }
    }

    /// Check for idle/alone sessions and leave if needed
    pub async fn check_and_cleanup(&self) {
        if !self.config.enabled {
            return;
        }

        let sessions_to_leave: Vec<u64> = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .filter(|(_, session)| {
                    // Leave if alone and configured to do so
                    if self.config.discord.leave_when_alone && session.is_alone() {
                        return true;
                    }
                    // Leave if idle too long
                    if session.is_idle(self.config.discord.idle_timeout_seconds) {
                        return true;
                    }
                    false
                })
                .map(|(guild_id, _)| *guild_id)
                .collect()
        };

        #[cfg(feature = "voice")]
        for guild_id in sessions_to_leave {
            info!(guild_id = %guild_id, "Leaving voice channel due to idle/alone");
            let _ = self.leave_channel(guild_id).await;
        }
    }

    /// Check if listening (STT) is available
    #[cfg(any(feature = "voice-whisper", feature = "voice-unmute", feature = "voice-moshi"))]
    pub fn can_listen(&self) -> bool {
        self.config.enabled && self.config.discord.listen_enabled
    }

    /// Check if listening is available (stub when no STT features)
    #[cfg(not(any(feature = "voice-whisper", feature = "voice-unmute", feature = "voice-moshi")))]
    pub fn can_listen(&self) -> bool {
        false
    }
}

// ============================================================================
// Voice Receiver for STT (Speech-to-Text)
// ============================================================================

/// Audio buffer for accumulating voice data from a user
#[cfg(any(feature = "voice-whisper", feature = "voice-unmute"))]
#[derive(Debug)]
pub struct UserAudioBuffer {
    /// User ID
    pub user_id: u64,
    /// Accumulated PCM audio samples (16-bit signed, 48kHz stereo from Discord)
    pub samples: Vec<i16>,
    /// Last time audio was received
    pub last_audio: Instant,
    /// Whether we're currently detecting speech
    pub is_speaking: bool,
    /// Silence duration threshold (ms) to consider speech ended
    pub silence_threshold_ms: u64,
}

#[cfg(any(feature = "voice-whisper", feature = "voice-unmute"))]
impl UserAudioBuffer {
    pub fn new(user_id: u64) -> Self {
        Self {
            user_id,
            samples: Vec::new(),
            last_audio: Instant::now(),
            is_speaking: false,
            silence_threshold_ms: 500, // 500ms of silence = end of utterance (faster response, still captures sentences)
        }
    }

    /// Add audio samples to buffer
    pub fn push_samples(&mut self, samples: &[i16]) {
        self.samples.extend_from_slice(samples);
        self.last_audio = Instant::now();
        
        // Simple VAD: check if samples are above noise threshold
        let rms = Self::calculate_rms(samples);
        self.is_speaking = rms > 500.0; // Adjust threshold as needed
    }

    /// Calculate RMS (root mean square) of samples for VAD
    fn calculate_rms(samples: &[i16]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum / samples.len() as f64).sqrt()
    }

    /// Check if user has stopped speaking (silence detected)
    pub fn has_silence(&self) -> bool {
        self.last_audio.elapsed().as_millis() as u64 > self.silence_threshold_ms
    }

    /// Get duration of buffered audio in milliseconds
    /// Discord audio is 48kHz stereo (2 channels)
    pub fn duration_ms(&self) -> u64 {
        // samples / (sample_rate * channels) * 1000
        (self.samples.len() as u64 * 1000) / (48000 * 2)
    }

    /// Convert stereo 48kHz to mono 16kHz for Whisper
    pub fn to_mono_16khz(&self) -> Vec<i16> {
        // First convert stereo to mono by averaging channels
        let mono: Vec<i16> = self.samples
            .chunks(2)
            .map(|chunk| {
                if chunk.len() == 2 {
                    ((chunk[0] as i32 + chunk[1] as i32) / 2) as i16
                } else {
                    chunk[0]
                }
            })
            .collect();

        // Then downsample from 48kHz to 16kHz (factor of 3)
        mono.iter()
            .step_by(3)
            .copied()
            .collect()
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.samples.clear();
        self.is_speaking = false;
    }

    /// Check if buffer has enough audio for transcription (min 0.5 seconds)
    pub fn has_enough_audio(&self) -> bool {
        self.duration_ms() >= 500
    }
}

/// Voice receiver event handler for capturing audio from users
#[cfg(any(feature = "voice-whisper", feature = "voice-unmute", feature = "voice-vosk"))]
pub struct VoiceReceiver {
    /// Guild ID this receiver is for
    pub guild_id: u64,
    /// Audio buffers per user
    pub buffers: Arc<parking_lot::RwLock<std::collections::HashMap<u64, UserAudioBuffer>>>,
    /// Channel to send transcribed text
    pub transcription_tx: tokio::sync::mpsc::Sender<(u64, String)>,
    /// STT engine to use (whisper, vosk)
    pub stt_engine: String,
}

// ============================================================================
// Low-Latency Realtime Voice Receiver (Unmute only)
// ============================================================================

/// Realtime voice receiver using persistent Unmute connection
/// 
/// This is the LOW-LATENCY version that:
/// - Maintains a persistent WebSocket to Unmute
/// - Streams audio chunks as they arrive (no buffering!)
/// - Commits on silence detection for instant transcription
#[cfg(feature = "voice-unmute")]
pub struct RealtimeVoiceReceiver {
    /// Guild ID
    pub guild_id: u64,
    /// Persistent Unmute conversation (per-user)
    pub conversations: Arc<parking_lot::RwLock<std::collections::HashMap<u64, RealtimeUserState>>>,
    /// Channel to send transcribed text
    pub transcription_tx: tokio::sync::mpsc::Sender<(u64, String)>,
    /// Unmute endpoint
    pub endpoint: String,
}

/// Per-user realtime state
#[cfg(feature = "voice-unmute")]
pub struct RealtimeUserState {
    /// Last audio timestamp (for silence detection)
    pub last_audio: std::time::Instant,
    /// Is user currently speaking?
    pub is_speaking: bool,
    /// Pending audio sender (to stream to Unmute)
    pub audio_tx: Option<tokio::sync::mpsc::Sender<Vec<i16>>>,
}

#[cfg(feature = "voice-unmute")]
impl RealtimeVoiceReceiver {
    /// Create new realtime receiver
    pub fn new(
        guild_id: u64,
        endpoint: &str,
        transcription_tx: tokio::sync::mpsc::Sender<(u64, String)>,
    ) -> Self {
        Self {
            guild_id,
            conversations: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            transcription_tx,
            endpoint: endpoint.to_string(),
        }
    }
    
    /// Process incoming audio - streams directly to Unmute
    /// 
    /// This is called for every VoiceTick (~20ms of audio).
    /// Audio is immediately forwarded to Unmute - no local buffering!
    pub fn process_audio(&self, user_id: u64, audio_48k_stereo: &[i16]) {
        // Convert to 16kHz mono (Unmute format)
        let mono_16k = Self::convert_48k_stereo_to_16k_mono(audio_48k_stereo);
        
        if mono_16k.is_empty() {
            return;
        }
        
        // Get or create user state
        let mut conversations = self.conversations.write();
        let state = conversations.entry(user_id).or_insert_with(|| {
            // Spawn streaming task for this user
            let (audio_tx, audio_rx) = tokio::sync::mpsc::channel::<Vec<i16>>(64);
            let endpoint = self.endpoint.clone();
            let tx = self.transcription_tx.clone();
            let guild_id = self.guild_id;
            
            tokio::spawn(async move {
                Self::stream_to_unmute(user_id, guild_id, endpoint, audio_rx, tx).await;
            });
            
            RealtimeUserState {
                last_audio: std::time::Instant::now(),
                is_speaking: false,
                audio_tx: Some(audio_tx),
            }
        });
        
        // Check if audio has energy (simple VAD)
        let rms = Self::calculate_rms(&mono_16k);
        let has_speech = rms > 300.0;
        
        if has_speech {
            state.last_audio = std::time::Instant::now();
            state.is_speaking = true;
            
            // Send audio to streaming task (non-blocking)
            if let Some(ref tx) = state.audio_tx {
                let _ = tx.try_send(mono_16k);
            }
        } else {
            // Check for silence timeout (trigger transcription)
            // Use 500ms silence threshold - balances complete thoughts vs responsiveness
            // Natural speech has pauses, but we can be more aggressive here
            let silence_ms = state.last_audio.elapsed().as_millis() as u64;
            if state.is_speaking && silence_ms > 500 {
                state.is_speaking = false;
                // Send empty to signal end of utterance
                if let Some(ref tx) = state.audio_tx {
                    let _ = tx.try_send(Vec::new());
                }
            }
        }
    }
    
    /// Streaming task - maintains persistent connection to Unmute
    async fn stream_to_unmute(
        user_id: u64,
        guild_id: u64,
        endpoint: String,
        mut audio_rx: tokio::sync::mpsc::Receiver<Vec<i16>>,
        transcription_tx: tokio::sync::mpsc::Sender<(u64, String)>,
    ) {
        use zoey_provider_voice::UnmuteRealtime;
        
        info!(user_id = %user_id, guild_id = %guild_id, "Starting realtime stream to Unmute");
        
        // Connect once - explicit type annotation needed
        let mut realtime: UnmuteRealtime = match UnmuteRealtime::connect(&endpoint).await {
            Ok(rt) => rt,
            Err(e) => {
                error!(error = %e, "Failed to connect to Unmute");
                return;
            }
        };
        
        // Main processing loop - handles both audio streaming and transcription receipt
        loop {
            tokio::select! {
                // Handle incoming audio
                audio = audio_rx.recv() => {
                    match audio {
                        Some(samples) if samples.is_empty() => {
                            // End of utterance - commit for transcription
                            if let Err(e) = realtime.commit().await {
                                warn!(error = %e, "Failed to commit audio");
                            }
                        }
                        Some(samples) => {
                            // Stream audio immediately
                            if let Err(e) = realtime.send_audio(&samples).await {
                                warn!(error = %e, "Failed to send audio");
                            }
                        }
                        None => {
                            // Channel closed - exit
                            break;
                        }
                    }
                }
                
                // Handle transcriptions (non-blocking check)
                text = realtime.recv() => {
                    if let Some(text) = text {
                        if !text.trim().is_empty() {
                            info!(user_id = %user_id, text = %text, "Realtime transcription");
                            let _ = transcription_tx.send((user_id, text)).await;
                        }
                    }
                }
            }
        }
        
        info!(user_id = %user_id, "Realtime stream ended");
    }
    
    /// Convert 48kHz stereo to 16kHz mono
    fn convert_48k_stereo_to_16k_mono(stereo_48k: &[i16]) -> Vec<i16> {
        // Average stereo channels
        let mono: Vec<i16> = stereo_48k
            .chunks(2)
            .map(|chunk| {
                if chunk.len() == 2 {
                    ((chunk[0] as i32 + chunk[1] as i32) / 2) as i16
                } else {
                    chunk[0]
                }
            })
            .collect();
        
        // Downsample 48kHz -> 16kHz (factor of 3)
        mono.iter().step_by(3).copied().collect()
    }
    
    /// Calculate RMS for VAD
    fn calculate_rms(samples: &[i16]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum / samples.len() as f64).sqrt()
    }
}

#[cfg(any(feature = "voice-whisper", feature = "voice-unmute", feature = "voice-vosk"))]
impl VoiceReceiver {
    pub fn new(
        guild_id: u64,
        transcription_tx: tokio::sync::mpsc::Sender<(u64, String)>,
        stt_engine: String,
    ) -> Self {
        Self {
            guild_id,
            buffers: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            transcription_tx,
            stt_engine,
        }
    }

    /// Get or create buffer for user
    fn get_or_create_buffer(&self, user_id: u64) -> parking_lot::RwLockWriteGuard<'_, std::collections::HashMap<u64, UserAudioBuffer>> {
        let mut buffers = self.buffers.write();
        if !buffers.contains_key(&user_id) {
            buffers.insert(user_id, UserAudioBuffer::new(user_id));
        }
        buffers
    }

    /// Process received audio from a user
    pub fn process_audio(&self, user_id: u64, audio: &[i16]) {
        let mut buffers = self.buffers.write();
        let buffer = buffers.entry(user_id).or_insert_with(|| UserAudioBuffer::new(user_id));
        buffer.push_samples(audio);
    }

    /// Check for completed utterances and trigger transcription
    pub async fn check_and_transcribe(&self) {
        let users_to_transcribe: Vec<(u64, Vec<i16>)> = {
            let mut buffers = self.buffers.write();
            let mut to_transcribe = Vec::new();
            
            for (user_id, buffer) in buffers.iter_mut() {
                // If user has stopped speaking and we have enough audio
                if buffer.has_silence() && buffer.has_enough_audio() {
                    let mono_16k = buffer.to_mono_16khz();
                    to_transcribe.push((*user_id, mono_16k));
                    buffer.clear();
                }
            }
            
            to_transcribe
        };

        // Transcribe each completed utterance
        for (user_id, audio_samples) in users_to_transcribe {
            if let Some(text) = self.transcribe_audio(&audio_samples).await {
                if !text.trim().is_empty() {
                    info!(user_id = %user_id, text = %text, "Transcribed user speech");
                    let _ = self.transcription_tx.send((user_id, text)).await;
                }
            }
        }
    }

    /// Transcribe audio samples using configured STT engine
    #[cfg(any(feature = "voice-whisper", feature = "voice-vosk"))]
    async fn transcribe_audio(&self, samples: &[i16]) -> Option<String> {
        use std::time::Instant;
        
        let start = Instant::now();

        // Use configured STT engine
        #[cfg(feature = "voice-vosk")]
        if self.stt_engine == "vosk" {
            // Use fast Vosk transcription directly on samples
            return match Self::transcribe_with_vosk(samples).await {
                Ok(text) => {
                    let elapsed = start.elapsed().as_millis();
                    if !text.trim().is_empty() {
                        info!(latency_ms = %elapsed, text = %text, "Vosk STT complete");
                    }
                    Some(text)
                }
                Err(e) => {
                    warn!(error = %e, "Vosk transcription failed");
                    None
                }
            };
        }
        
        // Fall back to Whisper
        #[cfg(feature = "voice-whisper")]
        {
            use zoey_provider_voice::{AudioData, AudioFormat, VoicePlugin, WhisperModel};
            use bytes::Bytes;
            
            let pcm_bytes: Vec<u8> = samples
                .iter()
                .flat_map(|&s| s.to_le_bytes())
                .collect();

            let audio = AudioData {
                data: Bytes::from(pcm_bytes),
                format: AudioFormat::Pcm,
                sample_rate: 16000,
                duration_ms: Some((samples.len() as u64 * 1000) / 16000),
                character_count: 0,
            };
            
            let plugin = VoicePlugin::with_whisper(WhisperModel::Tiny);
            return match plugin.transcribe(&audio).await {
                Ok(result) => {
                    let elapsed = start.elapsed().as_millis();
                    info!(latency_ms = %elapsed, text = %result.text, "Whisper STT complete");
                    Some(result.text)
                }
                Err(e) => {
                    warn!(error = %e, "Whisper transcription failed");
                    None
                }
            };
        }
        
        #[cfg(not(any(feature = "voice-whisper", feature = "voice-vosk")))]
        {
            warn!(engine = %self.stt_engine, "No STT engine available");
            None
        }
    }

    /// Fast Vosk transcription - uses cached model
    #[cfg(feature = "voice-vosk")]
    async fn transcribe_with_vosk(samples: &[i16]) -> Result<String, String> {
        use once_cell::sync::Lazy;
        use std::sync::Mutex;
        use vosk::{Model, Recognizer};
        
        // Cached model - loaded once, reused forever
        static VOSK_MODEL: Lazy<Mutex<Option<Model>>> = Lazy::new(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            let model_paths = [
                format!("{}/.cache/vosk/vosk-model-small-en-us-0.15", home),
                "/root/.cache/vosk/vosk-model-small-en-us-0.15".to_string(),
                "vosk-model-small-en-us-0.15".to_string(),
            ];
            
            for path in &model_paths {
                if std::path::Path::new(path).exists() {
                    info!(path = %path, "Loading Vosk model (one-time)");
                    if let Some(model) = Model::new(path) {
                        info!("Vosk model loaded successfully");
                        return Mutex::new(Some(model));
                    }
                }
            }
            
            warn!("Vosk model not found");
            Mutex::new(None)
        });
        
        let samples = samples.to_vec();
        
        // Run blocking transcription in thread pool
        tokio::task::spawn_blocking(move || {
            let model_guard = VOSK_MODEL.lock().map_err(|e| e.to_string())?;
            let model = model_guard.as_ref().ok_or("Vosk model not loaded")?;
            
            let mut recognizer = Recognizer::new(model, 16000.0)
                .ok_or("Failed to create recognizer")?;
            
            // Process audio
            recognizer.accept_waveform(&samples);
            
            // Get final result
            let result = recognizer.final_result();
            let text = result.single()
                .map(|r| r.text.to_string())
                .unwrap_or_default();
            
            Ok(text)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Transcribe audio (stub when using unmute only)
    #[cfg(all(feature = "voice-unmute", not(any(feature = "voice-whisper", feature = "voice-vosk"))))]
    async fn transcribe_audio(&self, samples: &[i16]) -> Option<String> {
        use zoey_provider_voice::{AudioData, AudioFormat, VoicePlugin};
        use bytes::Bytes;

        let pcm_bytes: Vec<u8> = samples
            .iter()
            .flat_map(|&s| s.to_le_bytes())
            .collect();

        let audio = AudioData {
            data: Bytes::from(pcm_bytes),
            format: AudioFormat::Pcm,
            sample_rate: 16000,
            duration_ms: Some((samples.len() as u64 * 1000) / 16000),
            character_count: 0,
        };

        // Use Unmute for transcription
        let plugin = VoicePlugin::with_unmute("ws://localhost:8000");
        
        match plugin.transcribe(&audio).await {
            Ok(result) => Some(result.text),
            Err(e) => {
                warn!(error = %e, "Transcription failed");
                None
            }
        }
    }
}

/// Songbird event handler for receiving voice data
#[cfg(any(feature = "voice-whisper", feature = "voice-unmute", feature = "voice-vosk", feature = "voice-moshi"))]
use songbird::events::{Event as SongbirdEvent, EventContext as SongbirdEventContext, EventHandler};

#[cfg(any(feature = "voice-whisper", feature = "voice-unmute", feature = "voice-vosk"))]
pub struct VoiceReceiverHandler {
    pub receiver: Arc<VoiceReceiver>,
}

#[cfg(any(feature = "voice-whisper", feature = "voice-unmute", feature = "voice-vosk"))]
#[async_trait::async_trait]
impl EventHandler for VoiceReceiverHandler {
    async fn act(&self, ctx: &SongbirdEventContext<'_>) -> Option<SongbirdEvent> {
        use songbird::events::context_data::VoiceTick;
        
        match ctx {
            SongbirdEventContext::VoiceTick(VoiceTick { speaking, silent, .. }) => {
                // Log voice tick activity
                if !speaking.is_empty() {
                    debug!(
                        guild_id = %self.receiver.guild_id,
                        speaking_count = %speaking.len(),
                        "VoiceTick: users speaking"
                    );
                }
                
                // Process audio from each speaking user
                for (&ssrc, data) in speaking.iter() {
                    if let Some(audio) = &data.decoded_voice {
                        let sample_count = audio.len();
                        if sample_count > 0 {
                            info!(
                                guild_id = %self.receiver.guild_id,
                                ssrc = %ssrc,
                                samples = %sample_count,
                                "Received voice audio from user"
                            );
                            // Use SSRC as user ID placeholder
                            self.receiver.process_audio(ssrc as u64, audio);
                        }
                    } else {
                        // Audio not decoded - this means DecodeMode might not be set
                        debug!(
                            ssrc = %ssrc,
                            "VoiceTick: no decoded audio (raw packet only)"
                        );
                    }
                }
            }
            SongbirdEventContext::SpeakingStateUpdate(state) => {
                info!(
                    guild_id = %self.receiver.guild_id,
                    ssrc = %state.ssrc,
                    speaking = ?state.speaking,
                    "User speaking state changed"
                );
            }
            _ => {}
        }
        
        None
    }
}

/// Realtime voice handler for Unmute streaming (lowest latency)
#[cfg(feature = "voice-unmute")]
pub struct RealtimeVoiceHandler {
    pub receiver: Arc<RealtimeVoiceReceiver>,
}

#[cfg(feature = "voice-unmute")]
#[async_trait::async_trait]
impl EventHandler for RealtimeVoiceHandler {
    async fn act(&self, ctx: &SongbirdEventContext<'_>) -> Option<SongbirdEvent> {
        use songbird::events::context_data::VoiceTick;
        
        match ctx {
            SongbirdEventContext::VoiceTick(VoiceTick { speaking, .. }) => {
                // Stream audio immediately to Unmute - no buffering!
                for (&ssrc, data) in speaking.iter() {
                    if let Some(audio) = &data.decoded_voice {
                        if !audio.is_empty() {
                            // Forward to realtime receiver (streams to Unmute WebSocket)
                            self.receiver.process_audio(ssrc as u64, audio);
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }
}

// ============================================================================
// Moshi Voice Receiver (Full-duplex real-time STT/TTS)
// ============================================================================

/// Moshi voice receiver for real-time full-duplex speech processing
/// 
/// This receiver uses Kyutai's Moshi model for ultra-low latency speech-to-text
/// and text-to-speech. Moshi can handle both directions simultaneously, enabling
/// natural conversation with ~200ms round-trip latency.
#[cfg(feature = "voice-moshi")]
pub struct MoshiVoiceReceiver {
    /// Guild ID
    pub guild_id: u64,
    /// Per-user state for Moshi streaming
    pub user_states: Arc<parking_lot::RwLock<std::collections::HashMap<u64, MoshiUserState>>>,
    /// Channel to send transcribed text
    pub transcription_tx: tokio::sync::mpsc::Sender<(u64, String)>,
    /// Moshi endpoint
    pub endpoint: String,
}

/// Per-user state for Moshi streaming
#[cfg(feature = "voice-moshi")]
pub struct MoshiUserState {
    /// Last audio timestamp (for silence detection)
    pub last_audio: std::time::Instant,
    /// Is user currently speaking?
    pub is_speaking: bool,
    /// Audio sender channel to stream to Moshi
    pub audio_tx: Option<tokio::sync::mpsc::Sender<Vec<f32>>>,
    /// Accumulated PCM buffer (Discord is 48kHz stereo, Moshi is 24kHz mono)
    pub pcm_buffer: Vec<i16>,
}

#[cfg(feature = "voice-moshi")]
impl MoshiVoiceReceiver {
    /// Create new Moshi voice receiver
    pub fn new(
        guild_id: u64,
        endpoint: &str,
        transcription_tx: tokio::sync::mpsc::Sender<(u64, String)>,
    ) -> Self {
        Self {
            guild_id,
            user_states: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            transcription_tx,
            endpoint: endpoint.to_string(),
        }
    }
    
    /// Process incoming audio - converts and streams to Moshi
    /// 
    /// Discord provides 48kHz stereo i16 PCM
    /// Moshi expects 24kHz mono f32 PCM
    pub fn process_audio(&self, user_id: u64, audio_48k_stereo: &[i16]) {
        // Convert Discord audio (48kHz stereo i16) to Moshi format (24kHz mono f32)
        let mono_24k_f32 = Self::convert_discord_to_moshi(audio_48k_stereo);
        
        if mono_24k_f32.is_empty() {
            return;
        }
        
        // Get or create user state
        let mut states = self.user_states.write();
        let state = states.entry(user_id).or_insert_with(|| {
            // Spawn streaming task for this user
            let (audio_tx, audio_rx) = tokio::sync::mpsc::channel::<Vec<f32>>(64);
            let endpoint = self.endpoint.clone();
            let tx = self.transcription_tx.clone();
            let guild_id = self.guild_id;
            
            tokio::spawn(async move {
                Self::stream_to_moshi(user_id, guild_id, endpoint, audio_rx, tx).await;
            });
            
            MoshiUserState {
                last_audio: std::time::Instant::now(),
                is_speaking: false,
                audio_tx: Some(audio_tx),
                pcm_buffer: Vec::with_capacity(4800), // 100ms at 48kHz
            }
        });
        
        // Check if audio has energy (simple VAD)
        let rms = Self::calculate_rms(&mono_24k_f32);
        let has_speech = rms > 0.01; // Threshold for f32 normalized audio
        
        if has_speech {
            state.last_audio = std::time::Instant::now();
            state.is_speaking = true;
            
            // Send audio to streaming task (non-blocking)
            if let Some(ref tx) = state.audio_tx {
                let _ = tx.try_send(mono_24k_f32);
            }
        } else {
            // Check for silence timeout (trigger end of utterance)
            let silence_ms = state.last_audio.elapsed().as_millis() as u64;
            if state.is_speaking && silence_ms > 500 {
                state.is_speaking = false;
                // Send empty to signal end of utterance
                if let Some(ref tx) = state.audio_tx {
                    let _ = tx.try_send(Vec::new());
                }
            }
        }
    }
    
    /// Streaming task - maintains connection to Moshi and processes events
    async fn stream_to_moshi(
        user_id: u64,
        guild_id: u64,
        endpoint: String,
        mut audio_rx: tokio::sync::mpsc::Receiver<Vec<f32>>,
        transcription_tx: tokio::sync::mpsc::Sender<(u64, String)>,
    ) {
        use zoey_provider_voice::{MoshiConfig, MoshiStreamingClient, MoshiEvent, MoshiControl};
        
        info!(user_id = %user_id, guild_id = %guild_id, endpoint = %endpoint, "Starting Moshi stream");
        
        // Clean endpoint and create config
        let clean_endpoint = endpoint
            .trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string();
        
        let config = MoshiConfig {
            endpoint: clean_endpoint,
            use_tls: true,
            asr_only: false, // We want both STT and TTS
            ..Default::default()
        };
        
        let mut client = MoshiStreamingClient::new(config);
        
        // Connect to Moshi - returns event receiver
        let mut event_rx = match client.connect().await {
            Ok(rx) => rx,
            Err(e) => {
                error!(error = %e, "Failed to connect to Moshi");
                return;
            }
        };
        
        info!(user_id = %user_id, "Connected to Moshi, starting audio stream");
        
        // Send start control
        if let Err(e) = client.send_control(MoshiControl::Start) {
            error!(error = %e, "Failed to send start command to Moshi");
            return;
        }
        
        // Main processing loop
        loop {
            tokio::select! {
                // Handle incoming audio from Discord
                audio = audio_rx.recv() => {
                    match audio {
                        Some(samples) if samples.is_empty() => {
                            // End of utterance - send end turn
                            if let Err(e) = client.send_control(MoshiControl::EndTurn) {
                                warn!(error = %e, "Failed to send end turn to Moshi");
                            }
                        }
                        Some(samples) => {
                            // Send audio to Moshi
                            if let Err(e) = client.send_audio(samples) {
                                warn!(error = %e, "Failed to send audio to Moshi");
                            }
                        }
                        None => {
                            // Channel closed - exit
                            info!(user_id = %user_id, "Audio channel closed, ending Moshi stream");
                            break;
                        }
                    }
                }
                
                // Handle events from Moshi
                event = event_rx.recv() => {
                    match event {
                        Some(MoshiEvent::Transcription { text, is_final }) => {
                            if !text.trim().is_empty() {
                                info!(
                                    user_id = %user_id,
                                    text = %text,
                                    is_final = %is_final,
                                    "Moshi transcription"
                                );
                                if is_final {
                                    // Send final transcription to callback
                                    let _ = transcription_tx.send((user_id, text)).await;
                                }
                            }
                        }
                        Some(MoshiEvent::AudioResponse { data }) => {
                            // Moshi generated audio response
                            // This would be handled by the TTS playback system
                            debug!(
                                user_id = %user_id,
                                audio_bytes = %data.len(),
                                "Moshi audio response (for TTS playback)"
                            );
                        }
                        Some(MoshiEvent::Ready { protocol_version, model_version }) => {
                            info!(
                                user_id = %user_id,
                                protocol_version = %protocol_version,
                                model_version = %model_version,
                                "Moshi ready"
                            );
                        }
                        Some(MoshiEvent::Error { message }) => {
                            error!(user_id = %user_id, error = %message, "Moshi error");
                        }
                        Some(MoshiEvent::Disconnected) => {
                            info!(user_id = %user_id, "Moshi disconnected");
                            break;
                        }
                        Some(MoshiEvent::Metadata { json }) => {
                            debug!(user_id = %user_id, metadata = %json, "Moshi metadata");
                        }
                        None => {
                            info!(user_id = %user_id, "Moshi event channel closed");
                            break;
                        }
                    }
                }
            }
        }
        
        // Send close command
        let _ = client.close();
        info!(user_id = %user_id, "Moshi stream ended");
    }
    
    /// Convert Discord audio format to Moshi format
    /// Discord: 48kHz stereo i16 PCM
    /// Moshi: 24kHz mono f32 PCM (normalized to -1.0 to 1.0)
    fn convert_discord_to_moshi(stereo_48k: &[i16]) -> Vec<f32> {
        // First: stereo to mono by averaging channels
        let mono_48k: Vec<i16> = stereo_48k
            .chunks(2)
            .map(|chunk| {
                if chunk.len() == 2 {
                    ((chunk[0] as i32 + chunk[1] as i32) / 2) as i16
                } else {
                    chunk[0]
                }
            })
            .collect();
        
        // Second: downsample 48kHz -> 24kHz (factor of 2)
        let mono_24k: Vec<i16> = mono_48k.iter().step_by(2).copied().collect();
        
        // Third: convert i16 to f32 normalized
        mono_24k
            .iter()
            .map(|&s| s as f32 / 32768.0)
            .collect()
    }
    
    /// Calculate RMS for VAD
    fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f32 = samples.iter().map(|&s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }
}

/// Songbird event handler for Moshi voice receiver
#[cfg(feature = "voice-moshi")]
pub struct MoshiVoiceHandler {
    pub receiver: Arc<MoshiVoiceReceiver>,
}

#[cfg(feature = "voice-moshi")]
#[async_trait::async_trait]
impl EventHandler for MoshiVoiceHandler {
    async fn act(&self, ctx: &SongbirdEventContext<'_>) -> Option<SongbirdEvent> {
        use songbird::events::context_data::VoiceTick;
        
        match ctx {
            SongbirdEventContext::VoiceTick(VoiceTick { speaking, .. }) => {
                // Stream audio immediately to Moshi - no buffering!
                for (&ssrc, data) in speaking.iter() {
                    if let Some(audio) = &data.decoded_voice {
                        if !audio.is_empty() {
                            // Forward to Moshi receiver
                            self.receiver.process_audio(ssrc as u64, audio);
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }
}

// Stub implementations when voice feature is disabled
#[cfg(not(feature = "voice"))]
impl VoiceManager {
    pub async fn join_channel(&self, _guild_id: u64, _channel_id: u64) -> Result<(), String> {
        Err("Voice feature not enabled. Compile with --features voice".to_string())
    }

    pub async fn leave_channel(&self, _guild_id: u64) -> Result<(), String> {
        Err("Voice feature not enabled. Compile with --features voice".to_string())
    }

    pub async fn speak(&self, _guild_id: u64, _text: &str) -> Result<(), String> {
        Err("Voice feature not enabled. Compile with --features voice".to_string())
    }
}

/// Wrap raw PCM audio data in a WAV header for symphonia to decode
/// 
/// Creates a minimal WAV file header for the given PCM parameters
#[cfg(feature = "voice")]
fn wrap_pcm_in_wav(pcm_data: &[u8], sample_rate: u32, channels: u16, bits_per_sample: u16) -> Vec<u8> {
    let byte_rate = sample_rate * (channels as u32) * (bits_per_sample as u32) / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = pcm_data.len() as u32;
    let file_size = 36 + data_size;
    
    let mut wav = Vec::with_capacity(44 + pcm_data.len());
    
    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    
    // fmt subchunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size (16 for PCM)
    wav.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat (1 = PCM)
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    
    // data subchunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm_data);
    
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_triggers() {
        let triggers = default_triggers();
        assert!(triggers.contains(&"let's chat".to_string()));
        assert!(triggers.contains(&"voice chat".to_string()));
        assert!(triggers.contains(&"join voice".to_string()));
    }

    #[test]
    fn test_voice_trigger_detection() {
        let mut config = VoiceConfig::default();
        config.enabled = true;

        assert!(config.is_voice_trigger("Hey, let's chat!"));
        assert!(config.is_voice_trigger("Can you join voice please?"));
        assert!(config.is_voice_trigger("VOICE CHAT now!"));
        assert!(!config.is_voice_trigger("Hello, how are you?"));
    }

    #[test]
    fn test_tts_request_detection() {
        let mut config = VoiceConfig::default();
        config.enabled = true;

        assert!(config.is_tts_request("Please read this aloud"));
        assert!(config.is_tts_request("Say this for me"));
        assert!(config.is_tts_request("Can you speak this?"));
        assert!(!config.is_tts_request("Hello, how are you?"));
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
                "discord": {
                    "auto_join_voice": "true",
                    "idle_timeout_seconds": "600"
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
        assert!(config.discord.auto_join_voice);
        assert_eq!(config.discord.idle_timeout_seconds, 600);
    }

    #[test]
    fn test_session_idle_detection() {
        let session = VoiceSession::new(123, 456);
        // Fresh session should not be idle
        assert!(!session.is_idle(300));
    }
}
