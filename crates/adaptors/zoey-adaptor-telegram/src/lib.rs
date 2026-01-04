use async_trait::async_trait;
use zoey_core::{
    types::{service::Service, ChannelType, Content, Memory, Room},
    validate_input, AgentRuntime, RateLimiter, Result,
};
use reqwest::Client as HttpClient;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use teloxide::prelude::*;
#[cfg(feature = "voice")]
use teloxide::types::InputFile;
use teloxide::types::{ChatId, Message as TelegramMessage, MessageId};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

pub mod voice;
pub use voice::{TelegramVoiceSettings, VoiceConfig, VoiceManager};

static TELEGRAM_DISPATCHER_HANDLE: OnceLock<JoinHandle<()>> = OnceLock::new();

pub fn shutdown_telegram() {
    if let Some(h) = TELEGRAM_DISPATCHER_HANDLE.get() {
        h.abort();
    }
}

/// Extract text content from a fully assembled XML response
fn extract_final_text_from_xml(content: &str) -> String {
    // Try to extract content from <text>...</text> tags
    if let Some(start) = content.find("<text>") {
        let after_tag = &content[start + 6..];
        if let Some(end) = after_tag.find("</text>") {
            return after_tag[..end].trim().to_string();
        }
        // Text tag opened but not closed - return what's after it
        return after_tag.trim().to_string();
    }

    // No text tag found - return the original content trimmed
    // This handles cases where the LLM didn't follow XML format
    content.trim().to_string()
}

#[derive(Clone)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub token: String,
    pub allowed_chats: Option<Vec<i64>>,
    pub allowed_users: Option<Vec<u64>>,
    /// Bot username (without @), used for detecting mentions in groups
    pub bot_username: Option<String>,
    /// Voice configuration for TTS
    pub voice: VoiceConfig,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            allowed_chats: None,
            allowed_users: None,
            bot_username: None,
            voice: VoiceConfig::default(),
        }
    }
}

pub struct TelegramAdapterService {
    config: TelegramConfig,
    runtime: Arc<RwLock<AgentRuntime>>,
    running: bool,
    limiter: Arc<RateLimiter>,
}

impl TelegramAdapterService {
    pub fn new(config: TelegramConfig, runtime: Arc<RwLock<AgentRuntime>>) -> Self {
        let limiter = Arc::new(RateLimiter::new(Duration::from_secs(60), 30));
        Self {
            config,
            runtime,
            running: false,
            limiter,
        }
    }
}

struct TelegramHandler {
    runtime: Arc<RwLock<AgentRuntime>>,
    limiter: Arc<RateLimiter>,
    allowed_chats: Option<HashSet<i64>>,
    allowed_users: Option<HashSet<u64>>,
    bot_username: Option<String>,
    bot_id: u64,
    voice_manager: Arc<VoiceManager>,
}

impl TelegramHandler {
    /// Send a voice message to a chat using TTS
    #[cfg(feature = "voice")]
    async fn send_voice_message(
        bot: &Bot,
        chat_id: i64,
        text: &str,
        voice_manager: &VoiceManager,
        include_text: bool,
    ) -> std::result::Result<(), String> {
        // Show recording audio action
        let _ = bot
            .send_chat_action(ChatId(chat_id), teloxide::types::ChatAction::RecordVoice)
            .await;

        // Synthesize speech
        let (audio_data, duration) = voice_manager.synthesize_for_telegram(text).await?;

        info!(
            chat_id = %chat_id,
            text_len = %text.len(),
            audio_size = %audio_data.len(),
            duration = %duration,
            "Sending voice message"
        );

        // Choose sending method based on configured output format
        let is_opus = voice_manager
            .config
            .output_format
            .eq_ignore_ascii_case("opus");
        let file_name = if is_opus { "voice.ogg" } else { "voice.mp3" };
        let input_file = InputFile::memory(audio_data.clone()).file_name(file_name);

        // Prefer native voice messages for Opus-in-OGG; otherwise use audio
        let send_result = if is_opus {
            bot.send_voice(ChatId(chat_id), input_file.clone())
                .duration(duration)
                .await
        } else {
            bot.send_audio(ChatId(chat_id), input_file.clone())
                .duration(duration)
                .await
        };

        // Fallback: if sending as voice failed, try as regular audio
        let send_result = match send_result {
            Ok(r) => Ok(r),
            Err(e) => {
                warn!(error = %e, "Primary send failed, attempting audio fallback");
                bot.send_audio(ChatId(chat_id), input_file)
                    .duration(duration)
                    .await
            }
        };

        match send_result {
            Ok(_) => {
                info!(chat_id = %chat_id, "Voice message sent successfully");
                // Optionally send text alongside
                if include_text {
                    let _ = bot.send_message(ChatId(chat_id), text).await;
                }
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Failed to send voice message");
                Err(format!("Failed to send voice message: {}", e))
            }
        }
    }

    async fn handle_message(&self, bot: Bot, msg: TelegramMessage) {
        // Check for voice message first (if STT is enabled)
        #[cfg(any(feature = "voice-whisper", feature = "voice-unmute"))]
        let is_voice_message = msg.voice().is_some();
        #[cfg(not(any(feature = "voice-whisper", feature = "voice-unmute")))]
        let is_voice_message = false;

        // Get text from message OR transcribe voice message
        let text: String;
        let from_voice: bool;

        #[cfg(any(feature = "voice-whisper", feature = "voice-unmute"))]
        {
            if is_voice_message && self.voice_manager.can_transcribe() {
                // Handle voice message - transcribe it
                let voice = msg.voice().unwrap();
                let file_id = &voice.file.id;
                
                match VoiceManager::download_voice_message(&bot, file_id).await {
                    Ok(audio_data) => {
                        match self.voice_manager.transcribe_voice_message(&audio_data).await {
                            Ok(transcribed) => {
                                if transcribed.trim().is_empty() {
                                    info!("Voice message transcribed but empty, ignoring");
                                    return;
                                }
                                info!(
                                    text_len = %transcribed.len(),
                                    "Voice message transcribed successfully"
                                );
                                text = transcribed;
                                from_voice = true;
                            }
                            Err(e) => {
                                warn!(error = %e, "Failed to transcribe voice message");
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to download voice message");
                        return;
                    }
                }
            } else if let Some(t) = msg.text() {
                text = t.to_string();
                from_voice = false;
            } else {
                return; // Ignore non-text, non-voice messages
            }
        }

        #[cfg(not(any(feature = "voice-whisper", feature = "voice-unmute")))]
        {
            text = match msg.text() {
                Some(t) => t.to_string(),
                None => return, // Ignore non-text messages
            };
            from_voice = false;
        }

        let from = match msg.from {
            Some(ref u) => u,
            None => return,
        };

        // Don't respond to bots
        if from.is_bot {
            return;
        }

        if validate_input(&text, 4096).is_err() {
            return;
        }

        // Capture minimal data needed for processing
        let msg_id = msg.id.0;
        let chat_id = msg.chat.id.0;
        let user_id = from.id.0;
        let is_private = msg.chat.is_private();

        let runtime = self.runtime.clone();
        let limiter = self.limiter.clone();
        let allowed_chats = self.allowed_chats.clone();
        let allowed_users = self.allowed_users.clone();
        let bot_username = self.bot_username.clone();
        let bot_id = self.bot_id;
        #[allow(unused_variables)]
        let voice_manager = self.voice_manager.clone();
        #[allow(unused_variables)]
        let respond_with_voice = from_voice; // Respond with voice if input was voice

        // Spawn worker thread with large stack - all heavy work happens here
        std::thread::Builder::new()
            .name("telegram_msg_worker".to_string())
            .stack_size(32 * 1024 * 1024) // 32MB stack
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                rt.block_on(async move {
                    let trimmed = text.trim();
                    if trimmed.starts_with('/') {
                        if trimmed == "/start" {
                            let _ = bot
                                .send_message(ChatId(chat_id), "Hi! I’m Zoey. Ask me anything.")
                                .await;
                            return;
                        }
                    }

                    // Voice command: request spoken AI response rather than reading user text
                    let mut force_voice = false;
                    let mut user_query_text = text.clone();
                    #[cfg(feature = "voice")]
                    if voice_manager.is_enabled() {
                        if voice_manager.config.is_voice_command(&text) {
                            if let Some(q) = voice_manager.config.extract_voice_text(&text) {
                                user_query_text = q;
                                force_voice = true;
                            } else {
                                let _ = bot
                                    .send_message(
                                        ChatId(chat_id),
                                        "Usage: /voice <question>\nExample: /voice What can you do?",
                                    )
                                    .await;
                                return;
                            }
                        }
                    }

                    // Dedup check
                    static ACTIVE: OnceLock<Arc<RwLock<HashSet<String>>>> = OnceLock::new();
                    let active_store = ACTIVE
                        .get_or_init(|| Arc::new(RwLock::new(HashSet::new())))
                        .clone();
                    let active_key = format!("{}:{}:{}", chat_id, user_id, msg_id);
                    {
                        let mut guard = active_store.write().unwrap();
                        if guard.contains(&active_key) {
                            return;
                        }
                        guard.insert(active_key.clone());
                    }

                    // Rate limiting
                    let key = format!("{}:{}", chat_id, user_id);
                    if !limiter.check(&key) {
                        return;
                    }

                    // Chat/user filters
                    if let Some(ref set) = allowed_chats {
                        if !set.contains(&chat_id) {
                            return;
                        }
                    }
                    if let Some(ref set) = allowed_users {
                        if !set.contains(&user_id) {
                            return;
                        }
                    }

                    // Check if bot is mentioned or addressed
                    let mentioned = if let Some(ref username) = bot_username {
                        text.contains(&format!("@{}", username))
                    } else {
                        false
                    };

                    // Check if this is a reply to the bot
                    let is_reply_to_bot = msg
                        .reply_to_message()
                        .and_then(|reply| reply.from.as_ref())
                        .map(|from| from.id.0 == bot_id)
                        .unwrap_or(false);

                    let in_allowed_chat = allowed_chats
                        .as_ref()
                        .map(|set| set.contains(&chat_id))
                        .unwrap_or(false);

                    let addressed_to_me =
                        is_private || mentioned || is_reply_to_bot || in_allowed_chat;

                    // Get agent info
                    let (agent_id, world_id, char_name) = {
                        let rt_guard = runtime.read().unwrap();
                        let world_id =
                            zoey_core::string_to_uuid(&format!("telegram-chat-{}", chat_id));
                        (rt_guard.agent_id, world_id, rt_guard.character.name.clone())
                    };

                    info!(
                        "[{}][telegram] incoming chat={} user={} len={}",
                        char_name,
                        chat_id,
                        user_id,
                        text.len()
                    );

                    // Show typing indicator
                    let _ = bot.send_chat_action(ChatId(chat_id), teloxide::types::ChatAction::Typing).await;

                    // Build room and memory
                    // Use deterministic room ID based on chat for consistent conversation history
                    let room_id =
                        zoey_core::string_to_uuid(&format!("telegram-room-{}", chat_id));
                    let room = Room {
                        id: room_id,
                        agent_id: Some(agent_id),
                        name: format!("telegram-{}", chat_id),
                        source: "telegram".to_string(),
                        channel_type: if is_private {
                            ChannelType::Dm
                        } else {
                            ChannelType::GroupDm
                        },
                        channel_id: Some(chat_id.to_string()),
                        server_id: None,
                        world_id,
                        metadata: Default::default(),
                        created_at: Some(chrono::Utc::now().timestamp()),
                    };

                    // Deterministic entity ID based on Telegram user ID for consistent message attribution
                    let entity_id =
                        zoey_core::string_to_uuid(&format!("telegram-user-{}", user_id));

                    let mut content = Content {
                        text: user_query_text.clone(),
                        source: Some("telegram".to_string()),
                        channel_type: Some(if is_private {
                            "PRIVATE".to_string()
                        } else {
                            "GROUP".to_string()
                        }),
                        ..Default::default()
                    };
                    content.metadata.insert(
                        "addressed_to_me".to_string(),
                        serde_json::Value::Bool(addressed_to_me),
                    );

                    let memory = Memory {
                        id: uuid::Uuid::new_v4(),
                        entity_id,
                        agent_id,
                        room_id: room.id,
                        content,
                        embedding: None,
                        metadata: None,
                        created_at: chrono::Utc::now().timestamp(),
                        unique: Some(false),
                        similarity: None,
                    };

                    // Send placeholder message
                    let placeholder_id: Option<i32> = if addressed_to_me || is_private {
                        match bot.send_message(ChatId(chat_id), "...").await {
                            Ok(m) => Some(m.id.0),
                            Err(_) => None,
                        }
                    } else {
                        None
                    };

                    // Setup HTTP client and API base
                    let api_base = std::env::var("AGENT_API_URL")
                        .ok()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| "http://127.0.0.1:9090/agent".to_string());

                    static TELEGRAM_SSE_CLIENT: OnceLock<HttpClient> = OnceLock::new();
                    let client = TELEGRAM_SSE_CLIENT
                        .get_or_init(|| {
                            HttpClient::builder()
                                .pool_max_idle_per_host(50)
                                .pool_idle_timeout(std::time::Duration::from_secs(30))
                                .build()
                                .unwrap_or_else(|_| HttpClient::new())
                        })
                        .clone();

                    // Memory persistence is handled by Agent API's /chat/stream endpoint
                    let _ = &runtime; // Keep runtime in scope
                    let body = serde_json::json!({
                        "text": user_query_text.clone(),
                        "roomId": room.id,
                        "entityId": memory.entity_id,
                        "stream": true
                    });
                    let resp = tokio::time::timeout(
                        std::time::Duration::from_secs(
                            std::env::var("TELEGRAM_STREAM_REQUEST_TIMEOUT_SECS")
                                .ok()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(20),
                        ),
                        client
                            .post(format!("{}/chat/stream", api_base))
                            .header("accept", "text/event-stream")
                            .json(&body)
                            .send(),
                    )
                    .await;
                    match resp {
                        Ok(Ok(mut r)) => {
                            let mut buffer = String::new();
                            let mut assembled = String::new();
                            let mut last_edit = std::time::Instant::now();
                            let edit_interval = std::time::Duration::from_millis(
                                std::env::var("TELEGRAM_EDIT_INTERVAL_MS")
                                    .ok()
                                    .and_then(|s| s.parse::<u64>().ok())
                                    .unwrap_or(500), // Telegram has stricter rate limits, use longer interval
                            );
                            let mut replaced_ack = false;
                            let mut finalized = false;
                            let inactivity_ms = std::env::var("TELEGRAM_STREAM_INACTIVITY_MS")
                                .ok()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(2000);
                            let inactivity_limit = std::time::Duration::from_millis(inactivity_ms);
                            #[allow(unused_assignments)]
                            let mut last_chunk_at = std::time::Instant::now();
                            while let Ok(opt) = r.chunk().await {
                                last_chunk_at = std::time::Instant::now();
                                let chunk = match opt {
                                    Some(c) => c,
                                    None => break,
                                };
                                let s = String::from_utf8_lossy(&chunk);
                                buffer.push_str(&s);
                                let mut parts: Vec<&str> = buffer.split('\n').collect();
                                let tail = parts.pop().unwrap_or("");
                                for line in parts {
                                    let l = line.trim();
                                    if !l.starts_with("data:") {
                                        continue;
                                    }
                                    let payload = l.trim_start_matches("data:").trim();
                                    if payload.is_empty() {
                                        continue;
                                    }
                                    if let Ok(json) =
                                        serde_json::from_str::<serde_json::Value>(payload)
                                    {
                                        if json.get("error").is_some() {
                                            continue;
                                        }
                                        let is_final = json
                                            .get("final")
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(false);
                                        let text =
                                            json.get("text").and_then(|v| v.as_str()).unwrap_or("");
                                        if !text.is_empty() {
                                            assembled.push_str(text);
                                        }
                                        let now = std::time::Instant::now();
                                        if now.duration_since(last_edit) >= edit_interval {
                                            if let Some(pid) = placeholder_id {
                                                if !replaced_ack && !assembled.is_empty() {
                                                    replaced_ack = true;
                                                }
                                                // Extract text content from XML format for display
                                                let display_text =
                                                    extract_final_text_from_xml(&assembled);
                                                if !display_text.is_empty() {
                                                    let _ = bot
                                                        .edit_message_text(
                                                            ChatId(chat_id),
                                                            MessageId(pid),
                                                            &display_text,
                                                        )
                                                        .await;
                                                }
                                            }
                                            last_edit = now;
                                        }
                                        if is_final && !finalized {
                                            finalized = true;
                                            // Extract text content from XML format for final display
                                            let display_text =
                                                extract_final_text_from_xml(&assembled);
                                            let final_content = if display_text.is_empty() {
                                                assembled.clone()
                                            } else {
                                                display_text.clone()
                                            };

                                            // Check if we should send as voice message
                                            #[cfg(feature = "voice")]
                                            let send_as_voice = voice_manager.is_enabled()
                                                && (force_voice
                                                    || voice_manager.config.telegram.auto_voice
                                                    || voice_manager.config.is_voice_trigger(&user_query_text));

                                            #[cfg(not(feature = "voice"))]
                                            let send_as_voice = false;

                                            if send_as_voice {
                                                #[cfg(feature = "voice")]
                                                {
                                                    // Delete placeholder if exists
                                                    if let Some(pid) = placeholder_id {
                                                        let _ = bot
                                                            .delete_message(ChatId(chat_id), MessageId(pid))
                                                            .await;
                                                    }
                                                    // Send as voice message
                                                    match Self::send_voice_message(
                                                        &bot,
                                                        chat_id,
                                                        &final_content,
                                                        &voice_manager,
                                                        voice_manager.config.telegram.include_text,
                                                    )
                                                    .await
                                                    {
                                                        Ok(_) => {}
                                                        Err(e) => {
                                                            warn!(error = %e, "Voice synthesis failed, sending as text");
                                                            let _ = bot
                                                                .send_message(ChatId(chat_id), &final_content)
                                                                .await;
                                                        }
                                                    }
                                                }
                                            } else {
                                                // Send final text message to Telegram
                                                if let Some(pid) = placeholder_id {
                                                    let _ = bot
                                                        .edit_message_text(
                                                            ChatId(chat_id),
                                                            MessageId(pid),
                                                            &final_content,
                                                        )
                                                        .await;
                                                } else if !final_content.is_empty() {
                                                    let _ = bot
                                                        .send_message(ChatId(chat_id), &final_content)
                                                        .await;
                                                }
                                            }
                                            break;
                                        }
                                    }
                                }
                                buffer = tail.to_string();
                                // Inactivity watchdog: finalize if no chunks for configured period
                                if !finalized && last_chunk_at.elapsed() >= inactivity_limit {
                                    finalized = true;
                                    // Extract text content from XML format
                                    let display_text = extract_final_text_from_xml(&assembled);
                                    let final_content = if display_text.is_empty() {
                                        assembled.clone()
                                    } else {
                                        display_text.clone()
                                    };

                                    // Check if we should send as voice message
                                    #[cfg(feature = "voice")]
                                    let send_as_voice = voice_manager.is_enabled()
                                        && (force_voice
                                            || voice_manager.config.telegram.auto_voice
                                            || voice_manager.config.is_voice_trigger(&user_query_text));

                                    #[cfg(not(feature = "voice"))]
                                    let send_as_voice = false;

                                    if send_as_voice {
                                        #[cfg(feature = "voice")]
                                        {
                                            if let Some(pid) = placeholder_id {
                                                let _ = bot
                                                    .delete_message(ChatId(chat_id), MessageId(pid))
                                                    .await;
                                            }
                                            match Self::send_voice_message(
                                                &bot,
                                                chat_id,
                                                &final_content,
                                                &voice_manager,
                                                voice_manager.config.telegram.include_text,
                                            )
                                            .await
                                            {
                                                Ok(_) => {}
                                                Err(e) => {
                                                    warn!(error = %e, "Voice synthesis failed, sending as text");
                                                    let _ = bot
                                                        .send_message(ChatId(chat_id), &final_content)
                                                        .await;
                                                }
                                            }
                                        }
                                    } else {
                                        if let Some(pid) = placeholder_id {
                                            let _ = bot
                                                .edit_message_text(
                                                    ChatId(chat_id),
                                                    MessageId(pid),
                                                    &final_content,
                                                )
                                                .await;
                                        } else if !final_content.is_empty() {
                                            let _ =
                                                bot.send_message(ChatId(chat_id), &final_content).await;
                                        }
                                    }
                                    break;
                                }
                            }
                            // Ensure finalization after stream ends without explicit final
                            if !finalized {
                                // Extract text content from XML format
                                let display_text = extract_final_text_from_xml(&assembled);
                                let final_content = if display_text.is_empty() {
                                    assembled.clone()
                                } else {
                                    display_text.clone()
                                };

                                // Check if we should send as voice message
                                #[cfg(feature = "voice")]
                                let send_as_voice = voice_manager.is_enabled()
                                    && (force_voice
                                        || voice_manager.config.telegram.auto_voice
                                        || voice_manager.config.is_voice_trigger(&user_query_text));

                                #[cfg(not(feature = "voice"))]
                                let send_as_voice = false;

                                if send_as_voice {
                                    #[cfg(feature = "voice")]
                                    {
                                        if let Some(pid) = placeholder_id {
                                            let _ = bot
                                                .delete_message(ChatId(chat_id), MessageId(pid))
                                                .await;
                                        }
                                        match Self::send_voice_message(
                                            &bot,
                                            chat_id,
                                            &final_content,
                                            &voice_manager,
                                            voice_manager.config.telegram.include_text,
                                        )
                                        .await
                                        {
                                            Ok(_) => {}
                                            Err(e) => {
                                                warn!(error = %e, "Voice synthesis failed, sending as text");
                                                let _ = bot
                                                    .send_message(ChatId(chat_id), &final_content)
                                                    .await;
                                            }
                                        }
                                    }
                                } else {
                                    if let Some(pid) = placeholder_id {
                                        let _ = bot
                                            .edit_message_text(
                                                ChatId(chat_id),
                                                MessageId(pid),
                                                &final_content,
                                            )
                                            .await;
                                    } else if !final_content.is_empty() {
                                        let _ = bot
                                            .send_message(ChatId(chat_id), &final_content)
                                            .await;
                                    }
                                }
                            }
                        }
                        _ => {
                            error!(
                                error = %"stream send timeout or error",
                                "Streaming request failed"
                            );
                            if let Some(pid) = placeholder_id {
                                let _ = bot
                                    .edit_message_text(ChatId(chat_id), MessageId(pid), "Error")
                                    .await;
                            }
                        }
                    }
                    {
                        let mut guard = active_store.write().unwrap();
                        guard.remove(&active_key);
                    }
                });
            })
            .ok();
    }
}

#[async_trait]
impl Service for TelegramAdapterService {
    fn service_type(&self) -> &str {
        "telegram-adapter"
    }

    async fn initialize(
        &mut self,
        _runtime_any: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        if !self.config.enabled || self.running {
            return Ok(());
        }
        let token = self.config.token.clone();

        let bot = Bot::new(&token);

        // Get bot info
        let me = bot.get_me().await.map_err(|e| {
            zoey_core::ZoeyError::other(format!("Failed to get bot info: {:?}", e))
        })?;

        let bot_id = me.id.0;
        let bot_username = me.username.clone();

        info!(
            "Telegram bot started: @{} (ID: {})",
            bot_username
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("unknown"),
            bot_id
        );

        // Initialize voice manager
        let voice_manager = Arc::new(VoiceManager::new(self.config.voice.clone()));
        if voice_manager.is_enabled() {
            info!(
                engine = %self.config.voice.engine,
                voice = %self.config.voice.voice_id,
                "Voice support enabled for Telegram"
            );
        }

        let handler = TelegramHandler {
            runtime: self.runtime.clone(),
            limiter: self.limiter.clone(),
            allowed_chats: self
                .config
                .allowed_chats
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            allowed_users: self
                .config
                .allowed_users
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            bot_username: bot_username.or(self.config.bot_username.clone()),
            bot_id,
            voice_manager,
        };

        let handler = Arc::new(handler);

        let handle = tokio::spawn(async move {
            let update_handler =
                Update::filter_message().endpoint(move |bot: Bot, msg: TelegramMessage| {
                    let handler = handler.clone();
                    async move {
                        handler.handle_message(bot, msg).await;
                        Ok::<(), std::convert::Infallible>(())
                    }
                });

            Dispatcher::builder(bot, update_handler)
                .enable_ctrlc_handler()
                .build()
                .dispatch()
                .await;
        });
        let _ = TELEGRAM_DISPATCHER_HANDLE.set(handle);

        self.running = true;
        info!("Telegram adapter started");
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.running = false;
        shutdown_telegram();
        Ok(())
    }
    fn is_running(&self) -> bool {
        self.running
    }
    async fn health_check(&self) -> Result<zoey_core::types::service::ServiceHealth> {
        Ok(zoey_core::types::service::ServiceHealth::Healthy)
    }
}

pub struct TelegramPlugin {
    config: TelegramConfig,
    runtime: Arc<RwLock<AgentRuntime>>,
}

impl TelegramPlugin {
    pub fn new(config: TelegramConfig, runtime: Arc<RwLock<AgentRuntime>>) -> Self {
        Self { config, runtime }
    }
}

#[async_trait]
impl zoey_core::types::Plugin for TelegramPlugin {
    fn name(&self) -> &str {
        "telegram"
    }
    fn description(&self) -> &str {
        "Telegram adapter using teloxide"
    }

    async fn init(
        &self,
        _config: std::collections::HashMap<String, String>,
        _runtime_any: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        {
            let mut rt = self.runtime.write().unwrap();
            let token = self.config.token.clone();
            let allowed_chats = self.config.allowed_chats.as_ref().map(|v| {
                v.iter()
                    .cloned()
                    .collect::<std::collections::HashSet<i64>>()
            });
            let handler: zoey_core::types::messaging::SendHandlerFunction =
                Arc::new(move |params| {
                    let token = token.clone();
                    let allowed_chats = allowed_chats.clone();
                    Box::pin(async move {
                        let bot = Bot::new(&token);
                        if let Some(cid) = params
                            .target
                            .metadata
                            .get("chat_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse::<i64>().ok())
                        {
                            if let Some(ref set) = allowed_chats {
                                if !set.contains(&cid) {
                                    return Ok(());
                                }
                            }
                            // Extract text content from XML format for display
                            let display_text = extract_final_text_from_xml(&params.content.text);
                            let content = if display_text.is_empty() {
                                params.content.text.clone()
                            } else {
                                display_text
                            };
                            bot.send_message(ChatId(cid), content).await.map_err(|e| {
                                zoey_core::ZoeyError::other(format!(
                                    "telegram send error: {:?}",
                                    e
                                ))
                            })?;
                        } else {
                            return Err(zoey_core::ZoeyError::other(
                                "missing chat_id in target.metadata",
                            ));
                        }
                        Ok(())
                    })
                });
            rt.register_send_handler("telegram".to_string(), handler);
        }
        Ok(())
    }

    fn services(&self) -> Vec<Arc<dyn Service>> {
        if !self.config.enabled {
            return Vec::new();
        }
        vec![Arc::new(TelegramAdapterService::new(
            self.config.clone(),
            self.runtime.clone(),
        ))]
    }
}

pub async fn register_telegram_send(runtime: Arc<RwLock<AgentRuntime>>, token: String) {
    let handler: zoey_core::types::messaging::SendHandlerFunction = Arc::new(move |params| {
        let token = token.clone();
        Box::pin(async move {
            let bot = Bot::new(&token);
            if let Some(cid) = params
                .target
                .metadata
                .get("chat_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
            {
                // Extract text content from XML format for display
                let display_text = extract_final_text_from_xml(&params.content.text);
                let content = if display_text.is_empty() {
                    params.content.text.clone()
                } else {
                    display_text
                };
                bot.send_message(ChatId(cid), content).await.map_err(|e| {
                    zoey_core::ZoeyError::other(format!("telegram send error: {:?}", e))
                })?;
            } else {
                return Err(zoey_core::ZoeyError::other(
                    "missing chat_id in target.metadata",
                ));
            }
            Ok(())
        })
    });
    let mut rt = runtime.write().unwrap();
    rt.register_send_handler("telegram".to_string(), handler);
}

pub async fn start_telegram(
    runtime: Arc<RwLock<AgentRuntime>>,
    config: TelegramConfig,
) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }
    register_telegram_send(runtime.clone(), config.token.clone()).await;
    let mut svc = TelegramAdapterService::new(config.clone(), runtime.clone());
    svc.initialize(Arc::new(())).await?;
    svc.start().await?;

    // Send test message if configured
    if let Ok(cid_str) = std::env::var("TELEGRAM_TEST_CHAT_ID") {
        if let Ok(cid) = cid_str.parse::<i64>() {
            let token = config.token.clone();
            tokio::spawn(async move {
                let bot = Bot::new(&token);
                match bot.send_message(ChatId(cid), "Zoey online").await {
                    Ok(_) => info!(chat_id = %cid, "Telegram ready test message sent"),
                    Err(e) => {
                        warn!(chat_id = %cid, error = %format!("{:?}", e), "Telegram ready test message failed")
                    }
                }
            });
        }
    }
    Ok(())
}
