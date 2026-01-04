use async_trait::async_trait;
use zoey_core::{
    types::{service::Service, ChannelType, Content, Memory, Room},
    validate_input, AgentRuntime, RateLimiter, Result,
};
use reqwest::Client as HttpClient;
use serenity::all::Interaction;
use serenity::async_trait as serenity_async_trait;
use serenity::builder::{
    CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage, EditMessage,
};
use serenity::http::Http;
use serenity::model::application::Command;
use serenity::model::channel::Message as DiscordMessage;
use serenity::model::gateway::GatewayIntents;
use serenity::cache::Settings as CacheSettings;
use serenity::model::id::ChannelId;
use serenity::model::id::MessageId;
use serenity::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

pub mod voice;
pub use voice::{VoiceConfig, VoiceManager, VoiceSession};

#[cfg(feature = "voice")]
use songbird::serenity::{SerenityInit, SongbirdKey};

/// Extract text content from XML response format
/// Handles both complete and partial XML responses
fn extract_text_from_xml(content: &str) -> String {
    // Try to extract content from <text>...</text> tags
    if let Some(start) = content.find("<text>") {
        let after_tag = &content[start + 6..];
        if let Some(end) = after_tag.find("</text>") {
            return after_tag[..end].trim().to_string();
        }
        // Partial XML - text tag opened but not closed yet
        // Return content after <text> tag
        return after_tag.trim().to_string();
    }

    // Check for self-closing or empty text tag
    if content.contains("<text/>") || content.contains("<text />") {
        return String::new();
    }

    // If no XML structure detected, check if we're inside a response block
    // This handles streaming where <text> came in an earlier chunk
    if content.contains("</text>") {
        // We have the closing tag - extract everything before it
        if let Some(end) = content.find("</text>") {
            return content[..end].trim().to_string();
        }
    }

    // No XML tags found - might be plain text or partial content
    // Return as-is if it doesn't look like XML metadata
    let trimmed = content.trim();
    if trimmed.starts_with("<response>")
        || trimmed.starts_with("<thought>")
        || trimmed.starts_with("<actions>")
        || trimmed.contains("</response>")
        || trimmed.contains("</thought>")
        || trimmed.contains("</actions>")
    {
        // This is XML structure but not the text content - return empty
        return String::new();
    }

    // Plain text or content not wrapped in XML
    trimmed.to_string()
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
pub struct DiscordConfig {
    pub enabled: bool,
    pub token: String,
    pub intents: GatewayIntents,
    pub application_id: Option<u64>,
    pub allowed_guilds: Option<Vec<u64>>,
    pub allowed_channels: Option<Vec<u64>>,
    pub allowed_users: Option<Vec<u64>>,
    /// Voice configuration from character XML
    pub voice: VoiceConfig,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            intents: GatewayIntents::GUILDS
                | GatewayIntents::GUILD_MESSAGES
                | GatewayIntents::DIRECT_MESSAGES
                | GatewayIntents::MESSAGE_CONTENT
                | GatewayIntents::GUILD_VOICE_STATES, // Added for voice support
            application_id: None,
            allowed_guilds: None,
            allowed_channels: None,
            allowed_users: None,
            voice: VoiceConfig::default(),
        }
    }
}

impl DiscordConfig {
    /// Create config with voice settings from character
    pub fn with_voice(mut self, voice_config: VoiceConfig) -> Self {
        self.voice = voice_config;
        self
    }

    /// Load voice config from character settings JSON
    pub fn load_voice_from_settings(&mut self, settings: &serde_json::Value) {
        self.voice = VoiceConfig::from_character_settings(settings);
        if self.voice.enabled {
            info!(
                engine = %self.voice.engine,
                voice = %self.voice.voice_name,
                "Voice enabled in Discord config"
            );
        }
    }
}

pub struct DiscordAdapterService {
    config: DiscordConfig,
    runtime: Arc<RwLock<AgentRuntime>>,
    running: bool,
    limiter: Arc<RateLimiter>,
}

impl DiscordAdapterService {
    pub fn new(config: DiscordConfig, runtime: Arc<RwLock<AgentRuntime>>) -> Self {
        let limiter = Arc::new(RateLimiter::new(Duration::from_secs(60), 30));
        Self {
            config,
            runtime,
            running: false,
            limiter,
        }
    }
}

/// Custom voice state tracker - maps (guild_id, user_id) -> channel_id
/// More reliable than serenity's cache for voice state tracking
type VoiceStateMap = Arc<RwLock<HashMap<(u64, u64), u64>>>;

struct Handler {
    runtime: Arc<RwLock<AgentRuntime>>,
    token: String,
    limiter: Arc<RateLimiter>,
    application_id: Option<u64>,
    allowed_guilds: Option<HashSet<u64>>,
    allowed_channels: Option<HashSet<u64>>,
    allowed_users: Option<HashSet<u64>>,
    recent_sends: Arc<RwLock<HashMap<u64, (String, Instant)>>>,
    /// Voice manager for handling voice channels
    voice_manager: Arc<VoiceManager>,
    /// Custom voice state tracker - more reliable than cache
    voice_states: VoiceStateMap,
}

#[serenity_async_trait]
impl serenity::prelude::EventHandler for Handler {
    #[allow(unused_variables)]
    async fn message(&self, ctx: Context, msg: DiscordMessage) {
        // Log every message received - use eprintln for visibility
        eprintln!(
            "[DISCORD HANDLER] Message received: author={} guild_id={:?} channel_id={} content_len={} is_bot={}",
            msg.author.name,
            msg.guild_id.map(|g| g.get()),
            msg.channel_id.get(),
            msg.content.len(),
            msg.author.bot
        );
        
        info!(
            author = %msg.author.name,
            guild_id = ?msg.guild_id.map(|g| g.get()),
            channel_id = %msg.channel_id.get(),
            content_len = %msg.content.len(),
            is_bot = %msg.author.bot,
            "Discord message received"
        );

        // Quick early returns - minimal work on serenity's thread
        if msg.author.bot {
            eprintln!("[DISCORD HANDLER] Early return: bot message");
            return;
        }
        if validate_input(&msg.content, 2000).is_err() {
            eprintln!("[DISCORD HANDLER] Early return: invalid input");
            return;
        }

        // Check for voice triggers
        eprintln!("[DISCORD HANDLER] About to check voice triggers for: {}", msg.content);
        info!(content = %msg.content, "About to check voice triggers");
        let voice_manager = self.voice_manager.clone();
        let voice_enabled = voice_manager.is_enabled();
        let is_trigger = voice_manager.config.is_voice_trigger(&msg.content);

        eprintln!(
            "[DISCORD HANDLER] Voice check: voice_enabled={} is_trigger={} content={}",
            voice_enabled, is_trigger, msg.content
        );
        info!(
            voice_enabled = %voice_enabled,
            is_trigger = %is_trigger,
            content = %msg.content,
            "Voice check"
        );

        if voice_enabled && is_trigger {
            // Handle voice trigger - try to join user's voice channel
            if let Some(guild_id) = msg.guild_id {
                info!(
                    guild_id = %guild_id.get(),
                    user_id = %msg.author.id.get(),
                    "Voice trigger detected - attempting to join"
                );

                // Try to find user's voice channel and join
                #[cfg(feature = "voice")]
                {
                    let gid = guild_id.get();
                    let uid = msg.author.id.get();
                    
                    // First check our custom voice state tracker (most reliable)
                    let user_voice_channel: Option<u64> = {
                        let states = self.voice_states.read().unwrap();
                        let custom_channel = states.get(&(gid, uid)).copied();
                        
                        if let Some(cid) = custom_channel {
                            info!(
                                guild_id = %gid,
                                user_id = %uid,
                                channel_id = %cid,
                                "Found user's voice channel in custom tracker"
                            );
                            Some(cid)
                        } else {
                            // Fallback to serenity cache
                            info!(
                                guild_id = %gid,
                                user_id = %uid,
                                tracked_states = %states.len(),
                                "User not in custom tracker, checking serenity cache"
                            );
                            drop(states); // Release lock before cache access
                            
                            if let Some(guild) = ctx.cache.guild(guild_id) {
                                info!(
                                    guild_name = %guild.name,
                                    voice_states_count = %guild.voice_states.len(),
                                    "Checking guild cache for voice states"
                                );
                                
                                // Log all voice states for debugging
                                for (uid, vs) in guild.voice_states.iter() {
                                    info!(
                                        user_id = %uid.get(),
                                        channel_id = ?vs.channel_id.map(|c| c.get()),
                                        "Voice state in serenity cache"
                                    );
                                }
                                
                                guild.voice_states.get(&msg.author.id)
                                    .and_then(|vs| vs.channel_id)
                                    .map(|c| c.get())
                            } else {
                                info!("Guild not found in serenity cache");
                                None
                            }
                        }
                    };
                    
                    let vm = voice_manager.clone();
                    let http = ctx.http.clone();
                    let reply_channel = msg.channel_id;
                    let channel_id_for_voice = msg.channel_id.get();
                    let guild_id_for_voice = gid;
                    
                    // Get character name for wake word detection
                    let char_name_for_voice = {
                        let rt_guard = self.runtime.read().unwrap();
                        rt_guard.character.name.clone()
                    };
                    
                    if let Some(cid) = user_voice_channel {
                        // User found in voice channel - spawn task to join
                        tokio::spawn(async move {
                            info!(channel_id = %cid, "Joining voice channel");
                            
                            // Create transcription callback that routes to agent when name is mentioned
                            let char_name = char_name_for_voice.clone();
                            let api_base = std::env::var("AGENT_API_URL")
                                .ok()
                                .filter(|s| !s.trim().is_empty())
                                .unwrap_or_else(|| "http://127.0.0.1:9090/agent".to_string());
                            
                            // Track active conversations per user (persistent mode: once name is detected, keep conversation active)
                            // Maps user_id -> last interaction timestamp
                            let active_conversations: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<u64, std::time::Instant>>> = 
                                std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
                            const CONVERSATION_TIMEOUT_SECS: u64 = 45; // Conversation stays active for 45 seconds after last interaction
                            
                            let callback: voice::TranscriptionCallback = Box::new(move |user_id, text| {
                                let char_name = char_name.clone();
                                let api_base = api_base.clone();
                                let channel_id = channel_id_for_voice;
                                let guild_id = guild_id_for_voice;
                                let active_conversations = active_conversations.clone();
                                
                                Box::pin(async move {
                                    // Check if user is in an active conversation (within timeout window)
                                    let is_in_active_conversation = {
                                        let convs = active_conversations.read().unwrap();
                                        if let Some(last_interaction) = convs.get(&user_id) {
                                            last_interaction.elapsed().as_secs() < CONVERSATION_TIMEOUT_SECS
                                        } else {
                                            false
                                        }
                                    };
                                    
                                    // Check if the transcribed text mentions the bot's name
                                    // Handle STT variations: "Zoey" might be transcribed as "Zoe", "Zowie", etc.
                                    let text_lower = text.to_lowercase();
                                    let char_name_lower = char_name.to_lowercase();
                                    
                                    // Generate comprehensive name variants for fuzzy matching
                                    let name_variants: Vec<String> = {
                                        let mut variants = vec![char_name_lower.clone()];
                                        
                                        // For names ending in 'ey' (like Zoey)
                                        if char_name_lower.ends_with("ey") {
                                            let base = &char_name_lower[..char_name_lower.len()-2];
                                            variants.push(format!("{}e", base));      // zoey -> zoe
                                            variants.push(format!("{}ie", base));     // zoey -> zoie
                                            variants.push(format!("{}owie", base));   // zoey -> zowie
                                            variants.push(format!("{}oy", base));     // zoey -> zoy
                                            variants.push(format!("{}oi", base));     // zoey -> zoi
                                            variants.push(format!("{}o e", base));    // zoey -> zo e (space)
                                            variants.push(format!("{}o-i", base));    // zoey -> zo-i (hyphen)
                                            // Also check the base itself (zo)
                                            if base.len() >= 2 {
                                                variants.push(base.to_string());
                                            }
                                        } else if char_name_lower.ends_with("y") {
                                            // For names ending in 'y' (like Joey, Amy)
                                            let base = &char_name_lower[..char_name_lower.len()-1];
                                            variants.push(base.to_string());          // joey -> joe
                                            variants.push(format!("{}ie", base));     // joey -> joie
                                            variants.push(format!("{}i", base));      // joey -> joi
                                        }
                                        
                                        // Remove duplicates
                                        variants.sort();
                                        variants.dedup();
                                        
                                        variants
                                    };
                                    
                                    // More flexible matching: check for variants OR partial matches
                                    let mentioned = {
                                        // First try exact variant matches (case-insensitive substring match)
                                        let exact_match = name_variants.iter().any(|variant| {
                                            // Check if variant appears anywhere in the text (as substring)
                                            text_lower.contains(variant)
                                        });
                                        
                                        if exact_match {
                                            true
                                        } else {
                                            // Fallback: check for partial phonetic matches
                                            // If name is 4+ chars, check if first 3-4 chars match anywhere
                                            if char_name_lower.len() >= 4 {
                                                let prefix = &char_name_lower[..char_name_lower.len().min(4)];
                                                // Check if prefix appears and is likely the name (not just a common word)
                                                let words: Vec<&str> = text_lower.split_whitespace().collect();
                                                words.iter().any(|word| {
                                                    word.starts_with(prefix) && word.len() <= prefix.len() + 2
                                                })
                                            } else {
                                                false
                                            }
                                        }
                                    };
                                    
                                    // Process message if name is mentioned OR user is in active conversation
                                    if !mentioned && !is_in_active_conversation {
                                        // Only log debug when not in active conversation to reduce noise
                                        return None;
                                    }
                                    
                                    // Update conversation timestamp (user is actively engaged)
                                    {
                                        let mut convs = active_conversations.write().unwrap();
                                        convs.insert(user_id, std::time::Instant::now());
                                    }
                                    
                                    if mentioned {
                                        eprintln!("[{}][voice] Processing: '{}' - name detected! (conversation activated)", char_name, text);
                                    } else {
                                        eprintln!("[{}][voice] Processing: '{}' - continuing active conversation", char_name, text);
                                    }
                                    
                                    // Build room ID consistent with text chat
                                    let room_id = zoey_core::string_to_uuid(&format!("discord-room-{}-{}", guild_id, channel_id));
                                    let entity_id = zoey_core::string_to_uuid(&format!("discord-voice-user-{}", user_id));
                                    
                                    // Use streaming endpoint (like text chat) - much faster and more reliable than task polling
                                    let client = reqwest::Client::builder()
                                        .timeout(std::time::Duration::from_secs(30)) // Reduced timeout for faster failure detection
                                        .build()
                                        .unwrap_or_else(|_| reqwest::Client::new());
                                    let body = serde_json::json!({
                                        "text": text,
                                        "roomId": room_id,
                                        "entityId": entity_id,
                                        "stream": true
                                    });
                                    
                                    // Call streaming endpoint with timeout (reduced from 60s to 30s for faster responses)
                                    let mut stream_resp = match tokio::time::timeout(
                                        std::time::Duration::from_secs(30),
                                        client
                                            .post(format!("{}/chat/stream", api_base))
                                            .header("accept", "text/event-stream")
                                            .json(&body)
                                            .send()
                                    )
                                    .await
                                    {
                                        Ok(Ok(resp)) => resp,
                                        Ok(Err(e)) => {
                                            eprintln!("[{}][voice] Stream request error: {}", char_name, e);
                                            return None;
                                        }
                                        Err(_) => {
                                            eprintln!("[{}][voice] Stream request timed out", char_name);
                                            return None;
                                        }
                                    };
                                    
                                    // Parse streaming response (SSE format) - same approach as text chat
                                    let mut buffer = String::new();
                                    let mut assembled = String::new();
                                    
                                    while let Ok(opt) = stream_resp.chunk().await {
                                        let chunk = match opt {
                                            Some(c) => c,
                                            None => break,
                                        };
                                        let s = String::from_utf8_lossy(&chunk);
                                        buffer.push_str(&s);
                                        
                                        // Parse SSE format (data: {...}\n\n)
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
                                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
                                                if json.get("error").is_some() {
                                                    continue;
                                                }
                                                let is_final = json.get("final").and_then(|v| v.as_bool()).unwrap_or(false);
                                                let text_chunk = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
                                                if !text_chunk.is_empty() {
                                                    assembled.push_str(text_chunk);
                                                }
                                                if is_final {
                                                    let clean_text = extract_final_text_from_xml(&assembled);
                                                    if !clean_text.is_empty() {
                                                        eprintln!("[{}][voice] Response: '{}'", char_name, clean_text);
                                                        return Some(clean_text);
                                                    }
                                                }
                                            }
                                        }
                                        
                                        buffer = tail.to_string();
                                    }
                                    
                                    // If we got here, try to extract final text from what we assembled
                                    if !assembled.is_empty() {
                                        let clean_text = extract_final_text_from_xml(&assembled);
                                        if !clean_text.is_empty() {
                                            eprintln!("[{}][voice] Response (from partial stream): '{}'", char_name, clean_text);
                                            return Some(clean_text);
                                        }
                                    }
                                    
                                    eprintln!("[{}][voice] No response extracted from stream", char_name);
                                    None
                                })
                            });
                            
                            let vm_clone = vm.clone();
                            match vm_clone.join_channel_with_callback(gid, cid, Some(callback)).await {
                                Ok(_) => {
                                    info!("Successfully joined voice channel with transcription callback");
                                    let listen_msg = if vm.config.discord.listen_enabled {
                                        "🎤 Joining voice channel! Say my name to start chatting - I'll remember our conversation for 45 seconds!"
                                    } else {
                                        "🎤 Joining voice channel!"
                                    };
                                    let _ = reply_channel.say(&http, listen_msg).await;
                                }
                                Err(e) => {
                                    warn!(error = %e, "Failed to join voice channel");
                                    let _ = reply_channel
                                        .say(&http, format!("Could not join voice: {}", e))
                                        .await;
                                }
                            }
                        });
                    } else {
                        // User not found in voice - tell them to join first
                        info!("Could not find user's voice channel in cache");
                        let http = ctx.http.clone();
                        tokio::spawn(async move {
                            let _ = reply_channel
                                .say(&http, "🎤 Join a voice channel first, then ask me to chat!")
                                .await;
                        });
                    }
                }

                #[cfg(not(feature = "voice"))]
                {
                    warn!("Voice feature not compiled in");
                }
            }
        }

        // Capture minimal data needed for the worker thread
        let msg_content = msg.content.clone();
        let msg_id = msg.id.get();
        let channel_id_raw = msg.channel_id.get();
        let guild_id_raw = msg.guild_id.map(|g| g.get()).unwrap_or(0);
        let author_id = msg.author.id.get();
        let mentions: Vec<u64> = msg.mentions.iter().map(|u| u.id.get()).collect();
        let mention_roles: Vec<u64> = msg.mention_roles.iter().map(|r| r.get()).collect();

        // Log voice trigger check result before spawning worker thread
        let voice_enabled_check = voice_manager.is_enabled();
        let is_trigger_check = voice_manager.config.is_voice_trigger(&msg_content);
        eprintln!(
            "[VOICE TRIGGER CHECK] voice_enabled={} is_trigger={} content=\"{}\"",
            voice_enabled_check, is_trigger_check, msg_content
        );
        
        let runtime = self.runtime.clone();
        let token = self.token.clone();
        let limiter = self.limiter.clone();
        let application_id = self.application_id;
        let allowed_guilds = self.allowed_guilds.clone();
        let allowed_channels = self.allowed_channels.clone();
        let allowed_users = self.allowed_users.clone();
        let voice_mgr = voice_manager.clone();

        // Spawn worker thread with large stack - all heavy work happens here
        std::thread::Builder::new()
            .name("discord_msg_worker".to_string())
            .stack_size(32 * 1024 * 1024) // 32MB stack
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                
                rt.block_on(async move {
                    // Ping command
                    if msg_content.trim() == "!ping" {
                        let http = Http::new(&token);
                        let _ = ChannelId::new(channel_id_raw).say(&http, "Pong!").await;
                        return;
                    }
                    
                    // Dedup check
                    static ACTIVE: OnceLock<Arc<RwLock<std::collections::HashSet<String>>>> = OnceLock::new();
                    let active_store = ACTIVE
                        .get_or_init(|| Arc::new(RwLock::new(std::collections::HashSet::new())))
                        .clone();
                    let active_key = format!("{}:{}:{}", channel_id_raw, author_id, msg_id);
                    {
                        let mut guard = active_store.write().unwrap();
                        if guard.contains(&active_key) {
                            return;
                        }
                        guard.insert(active_key.clone());
                    }
                    
                    // Rate limiting
                    let key = format!("{}:{}:{}", guild_id_raw, channel_id_raw, author_id);
                    if !limiter.check(&key) {
                        return;
                    }
                    
                    // Guild/channel/user filters
                    if let Some(ref set) = allowed_guilds {
                        debug!(guild_id = %guild_id_raw, allowed = ?set, "Checking guild filter");
                        if guild_id_raw != 0 && !set.contains(&guild_id_raw) {
                            debug!(guild_id = %guild_id_raw, "Filtered out - guild not in allowed list");
                            return;
                        }
                    }
                    if let Some(ref set) = allowed_channels {
                        debug!(channel_id = %channel_id_raw, allowed = ?set, "Checking channel filter");
                        if guild_id_raw != 0 && !set.contains(&channel_id_raw) {
                            debug!(channel_id = %channel_id_raw, "Filtered out - channel not in allowed list");
                            return;
                        }
                    }
                    if let Some(ref set) = allowed_users {
                        debug!(author_id = %author_id, allowed = ?set, "Checking user filter");
                        if !set.contains(&author_id) {
                            debug!(author_id = %author_id, "Filtered out - user not in allowed list");
                            return;
                        }
                    }
                    
                    // Get agent info
                    let (agent_id, world_id, char_name) = {
                        let rt_guard = runtime.read().unwrap();
                        let world_id = zoey_core::string_to_uuid(&format!("discord-guild-{}", guild_id_raw));
                        (rt_guard.agent_id, world_id, rt_guard.character.name.clone())
                    };
                    
                    // Safe UTF-8 truncation helper - finds valid char boundary
                    fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
                        if s.len() <= max_bytes {
                            return s;
                        }
                        // Find the last valid char boundary at or before max_bytes
                        let mut end = max_bytes;
                        while end > 0 && !s.is_char_boundary(end) {
                            end -= 1;
                        }
                        &s[..end]
                    }
                    
                    info!(
                        "[{}][discord] incoming guild={} channel={} user={} len={} content=\"{}\"",
                        char_name, guild_id_raw, channel_id_raw, author_id, msg_content.len(),
                        truncate_utf8(&msg_content, 100)
                    );
                    
                    let is_dm = guild_id_raw == 0;
                    let http = Http::new(&token);
                    if let Some(app_id) = application_id {
                        http.set_application_id(app_id.into());
                    }
                    
                    // Check if message contains the bot's name (case-insensitive)
                    // This allows users to say "Zoey", "hey zoey", "Hi Zoey" etc.
                    let msg_lower = msg_content.to_lowercase();
                    let char_name_lower = char_name.to_lowercase();
                    let mentioned_by_name = msg_lower.contains(&char_name_lower);
                    
                    // Check if bot is mentioned via @ - use cached bot_id from application_id if available
                    // This avoids a slow API call on every message
                    let bot_id = application_id.unwrap_or(0);
                    let mentioned_struct = mentions.iter().any(|&u| u == bot_id);
                    let mentioned_inline = if bot_id != 0 {
                        let a = format!("<@{}>", bot_id);
                        let b = format!("<@!{}>", bot_id);
                        msg_content.contains(&a) || msg_content.contains(&b)
                    } else {
                        false
                    };
                    
                    // Check if bot's specific role is mentioned (not just any role)
                    // Only respond to role mentions if we have a configured bot role ID
                    let bot_role_id: Option<u64> = std::env::var("DISCORD_BOT_ROLE_ID")
                        .ok()
                        .and_then(|s| s.parse().ok());
                    let has_role_mention = if let Some(role_id) = bot_role_id {
                        mention_roles.contains(&role_id) || msg_content.contains(&format!("<@&{}>", role_id))
                    } else {
                        false // Don't respond to arbitrary role mentions without configured role
                    };
                    
                    let in_allowed_channel = allowed_channels
                        .as_ref()
                        .map(|set| set.contains(&channel_id_raw))
                        .unwrap_or(false);
                    
                    // Determine if addressed to bot
                    let addressed_to_me = is_dm || mentioned_struct || mentioned_inline || mentioned_by_name || has_role_mention || in_allowed_channel;
                    
                    // Log addressing decision - use eprintln for visibility since tracing may not work in spawned thread
                    eprintln!(
                        "[{}][discord] MSG: \"{}\" | addressed_check: is_dm={} struct={} inline={} by_name={} role={} channel={} => {}",
                        char_name, 
                        truncate_utf8(&msg_content, 50),
                        is_dm, mentioned_struct, mentioned_inline, mentioned_by_name, has_role_mention, in_allowed_channel, addressed_to_me
                    );
                    
                    // Only show typing indicator if we're going to respond
                    if addressed_to_me {
                        let _ = ChannelId::new(channel_id_raw).broadcast_typing(&http).await;
                    } else {
                        eprintln!("[{}][discord] Ignoring - not addressed to bot", char_name);
                        return;
                    }
                    
                    // Build room and memory
                    // Use deterministic room ID based on channel for consistent conversation history
                    let room_id = zoey_core::string_to_uuid(&format!("discord-room-{}-{}", guild_id_raw, channel_id_raw));
                    let room = Room {
                        id: room_id,
                        agent_id: Some(agent_id),
                        name: format!("discord-{}-{}", guild_id_raw, channel_id_raw),
                        source: "discord".to_string(),
                        channel_type: if is_dm { ChannelType::Dm } else { ChannelType::GuildText },
                        channel_id: Some(channel_id_raw.to_string()),
                        server_id: if guild_id_raw != 0 { Some(guild_id_raw.to_string()) } else { None },
                        world_id,
                        metadata: Default::default(),
                        created_at: Some(chrono::Utc::now().timestamp()),
                    };
                    
                    // Deterministic entity ID based on Discord user ID for consistent message attribution
                    let entity_id = zoey_core::string_to_uuid(&format!("discord-user-{}", author_id));
                    
                    let mut content = Content {
                        text: msg_content.clone(),
                        source: Some("discord".to_string()),
                        channel_type: Some(if is_dm { "DM".to_string() } else { "GUILD_TEXT".to_string() }),
                        ..Default::default()
                    };
                    content.metadata.insert("addressed_to_me".to_string(), serde_json::Value::Bool(addressed_to_me));
                    
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
                    let ch = ChannelId::new(channel_id_raw);
                    let placeholder_id: Option<u64> = if addressed_to_me || is_dm {
                        match ch.say(&http, "").await {
                            Ok(m) => Some(m.id.get()),
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
                    
                    static DISCORD_SSE_CLIENT: OnceLock<HttpClient> = OnceLock::new();
                    let client = DISCORD_SSE_CLIENT.get_or_init(|| {
                        HttpClient::builder()
                            .pool_max_idle_per_host(50)
                            .pool_idle_timeout(std::time::Duration::from_secs(30))
                            .build()
                            .unwrap_or_else(|_| HttpClient::new())
                    }).clone();
                    
                    // Memory persistence is handled by Agent API's /chat/stream endpoint
                    let _ = &runtime; // Keep runtime in scope
            let body = serde_json::json!({
                "text": msg.content.clone(),
                "roomId": room.id,
                "entityId": memory.entity_id,
                "stream": true
            });
            let resp = tokio::time::timeout(
                std::time::Duration::from_secs(
                    std::env::var("DISCORD_STREAM_REQUEST_TIMEOUT_SECS")
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
                        std::env::var("DISCORD_EDIT_INTERVAL_MS")
                            .ok()
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(250),
                    );
                    let mut replaced_ack = false;
                    let mut finalized = false;
                    let inactivity_ms = std::env::var("DISCORD_STREAM_INACTIVITY_MS")
                        .ok()
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(2000);
                    let inactivity_limit = std::time::Duration::from_millis(inactivity_ms);
                    let mut last_chunk_at = std::time::Instant::now();
                    while let Ok(opt) = r.chunk().await {
                        let chunk = match opt {
                            Some(c) => c,
                            None => break,
                        };
                        let s = String::from_utf8_lossy(&chunk);
                        buffer.push_str(&s);
                        last_chunk_at = std::time::Instant::now();
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
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
                                if json.get("error").is_some() {
                                    continue;
                                }
                                let is_final =
                                    json.get("final").and_then(|v| v.as_bool()).unwrap_or(false);
                                let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
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
                                        let display_text = extract_final_text_from_xml(&assembled);
                                        if !display_text.is_empty() {
                                            let _ = ch
                                                .edit_message(
                                                    &http,
                                                    MessageId::new(pid),
                                                    EditMessage::new().content(display_text),
                                                )
                                                .await;
                                        }
                                    }
                                    last_edit = now;
                                }
                                if is_final && !finalized {
                                    finalized = true;
                                    // Extract text content from XML format for final display
                                    let display_text = extract_final_text_from_xml(&assembled);
                                    let final_content = if display_text.is_empty() { assembled.clone() } else { display_text.clone() };
                                    
                                    // Send final message to Discord
                                    if let Some(pid) = placeholder_id {
                                        let _ = ch
                                            .edit_message(
                                                &http,
                                                MessageId::new(pid),
                                                EditMessage::new().content(final_content.clone()),
                                            )
                                            .await;
                                    } else if !final_content.is_empty() {
                                        let _ = ch.say(&http, final_content.clone()).await;
                                    }
                                    
                                    // Speak in voice channel if enabled and in voice
                                    if voice_mgr.is_enabled() && 
                                       voice_mgr.config.discord.speak_responses &&
                                       guild_id_raw != 0 &&
                                       !final_content.is_empty() {
                                        info!(guild_id = %guild_id_raw, text_len = %final_content.len(), "Attempting TTS speak");
                                        // Check if we're in a voice channel for this guild
                                        let sessions = voice_mgr.sessions.read().await;
                                        let in_voice = sessions.contains_key(&guild_id_raw);
                                        drop(sessions);
                                        
                                        if in_voice {
                                            info!(guild_id = %guild_id_raw, "In voice channel, calling speak()");
                                            match voice_mgr.speak(guild_id_raw, &final_content).await {
                                                Ok(_) => info!(guild_id = %guild_id_raw, "TTS speak completed successfully"),
                                                Err(e) => warn!(error = %e, guild_id = %guild_id_raw, "Failed to speak in voice channel"),
                                            }
                                        } else {
                                            info!(guild_id = %guild_id_raw, "Not in voice channel, skipping TTS");
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
                            if let Some(pid) = placeholder_id {
                                let final_content = if display_text.is_empty() { assembled.clone() } else { display_text.clone() };
                                let _ = ch
                                    .edit_message(
                                        &http,
                                        MessageId::new(pid),
                                        EditMessage::new().content(final_content),
                                    )
                                    .await;
                            } else if !display_text.is_empty() {
                                let _ = ch.say(&http, display_text).await;
                            } else if !assembled.is_empty() {
                                let _ = ch.say(&http, assembled.clone()).await;
                            }
                            break;
                        }
                    }
                    // Ensure finalization after stream ends without explicit final
                    if !finalized {
                        // Extract text content from XML format
                        let display_text = extract_final_text_from_xml(&assembled);
                        if let Some(pid) = placeholder_id {
                            let final_content = if display_text.is_empty() { assembled.clone() } else { display_text.clone() };
                            let _ = ch
                                .edit_message(
                                    &http,
                                    MessageId::new(pid),
                                    EditMessage::new().content(final_content),
                                )
                                .await;
                        } else if !display_text.is_empty() {
                            let _ = ch.say(&http, display_text).await;
                        } else if !assembled.is_empty() {
                            let _ = ch.say(&http, assembled.clone()).await;
                        }
                    }
                }
                _ => {
                    error!(error = %"stream send timeout or error", "Streaming request failed");
                    if let Some(pid) = placeholder_id {
                        let _ = ch
                            .edit_message(
                                &http,
                                MessageId::new(pid),
                                EditMessage::new().content("Error"),
                            )
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

    /// Populate initial voice states when guild data is received
    async fn guild_create(&self, _ctx: Context, guild: serenity::model::guild::Guild, _is_new: Option<bool>) {
        let guild_id = guild.id.get();
        info!(
            guild_id = %guild_id,
            guild_name = %guild.name,
            voice_states_count = %guild.voice_states.len(),
            "Guild create: populating initial voice states"
        );
        
        // Populate our custom voice state tracker with initial voice states
        let mut states = self.voice_states.write().unwrap();
        for (user_id, voice_state) in guild.voice_states.iter() {
            if let Some(channel_id) = voice_state.channel_id {
                let uid = user_id.get();
                let cid = channel_id.get();
                info!(
                    guild_id = %guild_id,
                    user_id = %uid,
                    channel_id = %cid,
                    "Initial voice state: user already in voice channel"
                );
                states.insert((guild_id, uid), cid);
            }
        }
        info!(
            guild_id = %guild_id,
            tracked_users = %states.len(),
            "Voice states initialized from guild_create"
        );
    }

    async fn ready(&self, ctx: Context, data_about_bot: serenity::model::gateway::Ready) {
        info!(
            user = %data_about_bot.user.name,
            guilds_count = %data_about_bot.guilds.len(),
            "Discord ready"
        );

        // Log all guilds the bot is in
        for guild in &data_about_bot.guilds {
            info!(guild_id = %guild.id.get(), "Bot is in guild");
        }
        let http = ctx.http.clone();
        tokio::spawn(async move {
            let builder = CreateCommand::new("ping").description("A simple ping command");
            if let Err(e) = Command::create_global_command(&http, builder).await {
                warn!(error = %format!("{:?}", e), "Register global ping failed");
            }
        });

        if let Ok(cid_str) = std::env::var("DISCORD_TEST_CHANNEL_ID") {
            if let Ok(cid) = cid_str.parse::<u64>() {
                let http2 = ctx.http.clone();
                tokio::spawn(async move {
                    let ch = ChannelId::new(cid);
                    match ch.say(&http2, "Zoey online").await {
                        Ok(_) => info!(channel_id = %cid, "Discord ready test message sent"),
                        Err(e) => {
                            warn!(channel_id = %cid, error = %format!("{:?}", e), "Discord ready test message failed")
                        }
                    }
                });
            }
        }

        let _ = std::env::var("DISCORD_GUILD_ID");
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(cmd) = interaction {
            if cmd.data.name == "ping" {
                let _ = cmd
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::default().content("Pong!"),
                        ),
                    )
                    .await;
            }
        }
    }

    /// Track voice state changes - this is more reliable than serenity's cache
    async fn voice_state_update(&self, _ctx: Context, old: Option<serenity::model::voice::VoiceState>, new: serenity::model::voice::VoiceState) {
        let user_id = new.user_id.get();
        let guild_id = match new.guild_id {
            Some(g) => g.get(),
            None => return, // Ignore DM voice states
        };
        
        if let Some(channel_id) = new.channel_id {
            // User joined or moved to a voice channel
            let cid = channel_id.get();
            info!(
                guild_id = %guild_id,
                user_id = %user_id,
                channel_id = %cid,
                "Voice state update: user joined/moved to voice channel"
            );
            let mut states = self.voice_states.write().unwrap();
            states.insert((guild_id, user_id), cid);
        } else {
            // User left voice channel
            info!(
                guild_id = %guild_id,
                user_id = %user_id,
                old_channel = ?old.as_ref().and_then(|o| o.channel_id).map(|c| c.get()),
                "Voice state update: user left voice channel"
            );
            let mut states = self.voice_states.write().unwrap();
            states.remove(&(guild_id, user_id));
        }
    }
}

#[async_trait]
impl Service for DiscordAdapterService {
    fn service_type(&self) -> &str {
        "discord-adapter"
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
        let intents = self.config.intents;
        let voice_config = self.config.voice.clone();

        // Create Songbird voice client if voice feature is enabled
        #[cfg(feature = "voice")]
        let songbird = {
            use songbird::{Config, driver::DecodeMode};
            
            // Configure Songbird with decode mode for receiving voice
            let config = Config::default()
                .decode_mode(DecodeMode::Decode); // Enable decoding incoming audio
            
            songbird::Songbird::serenity_from_config(config)
        };

        // Create voice manager with Songbird
        #[cfg(feature = "voice")]
        let voice_manager = {
            let vm = Arc::new(VoiceManager::with_songbird(voice_config, songbird.clone()));
            // Initialize voice manager - starts Piper server if engine is "piper"
            if let Err(e) = vm.init().await {
                warn!(error = %e, "Failed to initialize voice manager");
            }
            info!(
                voice_enabled = %vm.config.enabled,
                listen_enabled = %vm.config.discord.listen_enabled,
                speak_responses = %vm.config.discord.speak_responses,
                engine = %vm.config.engine,
                "Voice manager initialized"
            );
            vm
        };

        #[cfg(not(feature = "voice"))]
        let voice_manager = Arc::new(VoiceManager::new(voice_config));

        let handler = Handler {
            runtime: self.runtime.clone(),
            token: token.clone(),
            limiter: self.limiter.clone(),
            application_id: self.config.application_id,
            allowed_guilds: self
                .config
                .allowed_guilds
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            allowed_channels: self
                .config
                .allowed_channels
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            allowed_users: self
                .config
                .allowed_users
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            recent_sends: Arc::new(RwLock::new(HashMap::new())),
            voice_manager,
            voice_states: Arc::new(RwLock::new(HashMap::new())),
        };

        #[cfg(feature = "voice")]
        let songbird_for_client = songbird.clone();

        tokio::spawn(async move {
            // Configure cache to store voice states (required for voice channel detection)
            let mut cache_settings = CacheSettings::default();
            cache_settings.cache_guilds = true;
            cache_settings.cache_channels = true;
            cache_settings.cache_users = true;
            
            #[cfg(feature = "voice")]
            let client_result = Client::builder(&token, intents)
                .event_handler(handler)
                .cache_settings(cache_settings)
                .register_songbird_with(songbird_for_client)
                .await;

            #[cfg(not(feature = "voice"))]
            let client_result = Client::builder(&token, intents)
                .event_handler(handler)
                .cache_settings(cache_settings)
                .await;

            match client_result {
                Ok(mut client) => {
                    if let Err(why) = client.start().await {
                        error!(error = %format!("{:?}", why), "Discord client error");
                    }
                }
                Err(why) => {
                    error!(error = %format!("{:?}", why), "Err creating Discord client");
                }
            }
        });

        self.running = true;
        info!("Discord adapter started");
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }
    fn is_running(&self) -> bool {
        self.running
    }
    async fn health_check(&self) -> Result<zoey_core::types::service::ServiceHealth> {
        Ok(zoey_core::types::service::ServiceHealth::Healthy)
    }
}

pub struct DiscordPlugin {
    config: DiscordConfig,
    runtime: Arc<RwLock<AgentRuntime>>,
}

impl DiscordPlugin {
    pub fn new(config: DiscordConfig, runtime: Arc<RwLock<AgentRuntime>>) -> Self {
        Self { config, runtime }
    }
}

#[async_trait]
impl zoey_core::types::Plugin for DiscordPlugin {
    fn name(&self) -> &str {
        "discord"
    }
    fn description(&self) -> &str {
        "Discord adapter using serenity websocket"
    }

    async fn init(
        &self,
        _config: std::collections::HashMap<String, String>,
        _runtime_any: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        {
            let mut rt = self.runtime.write().unwrap();
            let token = self.config.token.clone();
            let app_id = self.config.application_id;
            let allowed_channels = self.config.allowed_channels.as_ref().map(|v| {
                v.iter()
                    .cloned()
                    .collect::<std::collections::HashSet<u64>>()
            });
            let handler: zoey_core::types::messaging::SendHandlerFunction =
                Arc::new(move |params| {
                    let token = token.clone();
                    let app_id = app_id.clone();
                    let allowed_channels = allowed_channels.clone();
                    Box::pin(async move {
                        let http = Http::new(&token);
                        if let Some(id) = app_id {
                            http.set_application_id(id.into());
                        }
                        if let Some(cid) = params
                            .target
                            .metadata
                            .get("channel_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse::<u64>().ok())
                        {
                            if let Some(ref set) = allowed_channels {
                                if !set.contains(&cid) {
                                    return Ok(());
                                }
                            }
                            let ch = ChannelId::new(cid);
                            // Extract text content from XML format for display
                            let display_text = extract_final_text_from_xml(&params.content.text);
                            let content = if display_text.is_empty() {
                                params.content.text.clone()
                            } else {
                                display_text
                            };
                            ch.say(&http, content).await.map_err(|e| {
                                zoey_core::ZoeyError::other(format!(
                                    "discord send error: {:?}",
                                    e
                                ))
                            })?;
                        } else {
                            return Err(zoey_core::ZoeyError::other(
                                "missing channel_id in target.metadata",
                            ));
                        }
                        Ok(())
                    })
                });
            rt.register_send_handler("discord".to_string(), handler);
        }
        Ok(())
    }

    fn services(&self) -> Vec<Arc<dyn Service>> {
        if !self.config.enabled {
            return Vec::new();
        }
        vec![Arc::new(DiscordAdapterService::new(
            self.config.clone(),
            self.runtime.clone(),
        ))]
    }
}

pub async fn register_discord_send(
    runtime: Arc<RwLock<AgentRuntime>>,
    token: String,
    application_id: Option<u64>,
) {
    let handler: zoey_core::types::messaging::SendHandlerFunction = Arc::new(move |params| {
        let token = token.clone();
        let app_id = application_id.clone();
        Box::pin(async move {
            let http = Http::new(&token);
            if let Some(id) = app_id {
                http.set_application_id(id.into());
            }
            if let Some(cid) = params
                .target
                .metadata
                .get("channel_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
            {
                let ch = ChannelId::new(cid);
                // Extract text content from XML format for display
                let display_text = extract_final_text_from_xml(&params.content.text);
                let content = if display_text.is_empty() {
                    params.content.text.clone()
                } else {
                    display_text
                };
                ch.say(&http, content).await.map_err(|e| {
                    zoey_core::ZoeyError::other(format!("discord send error: {:?}", e))
                })?;
            } else {
                return Err(zoey_core::ZoeyError::other(
                    "missing channel_id in target.metadata",
                ));
            }
            Ok(())
        })
    });
    let mut rt = runtime.write().unwrap();
    rt.register_send_handler("discord".to_string(), handler);
}

pub async fn start_discord(
    runtime: Arc<RwLock<AgentRuntime>>,
    mut config: DiscordConfig,
) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }
    register_discord_send(runtime.clone(), config.token.clone(), config.application_id).await;
    let mut svc = DiscordAdapterService::new(config.clone(), runtime.clone());
    svc.initialize(Arc::new(())).await?;
    svc.start().await?;
    if let Ok(cid_str) = std::env::var("DISCORD_TEST_CHANNEL_ID") {
        if let Ok(cid) = cid_str.parse::<u64>() {
            let token = config.token.clone();
            let app_id = config.application_id;
            tokio::spawn(async move {
                let http = Http::new(&token);
                if let Some(id) = app_id {
                    http.set_application_id(id.into());
                }
                let ch = ChannelId::new(cid);
                match ch.say(&http, "Zoey online").await {
                    Ok(_) => info!(channel_id = %cid, "Discord test message sent"),
                    Err(e) => {
                        error!(channel_id = %cid, error = %format!("{:?}", e), "Discord test message failed")
                    }
                }
            });
        }
    }
    Ok(())
}
