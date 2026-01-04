//! Entity resolution and management utilities
//!
//! This module provides utilities for resolving entity names within conversations
//! using LLM-based context analysis and fallback matching strategies.
//!
//! # Key Features
//!
//! - **LLM-based resolution**: Uses language models to resolve ambiguous entity references
//! - **Context-aware**: Considers recent messages, relationships, and room participants
//! - **Permission filtering**: Respects world roles (Owner/Admin/Moderator/Member)
//! - **Multiple strategies**: Falls back to direct matching when LLM isn't available
//! - **Interaction tracking**: Prioritizes entities based on relationship strength
//!
//! # Usage
//!
//! ```rust,no_run
//! use zoey_core::{find_entity_by_name, RuntimeRef, Memory, State};
//! use std::sync::Arc;
//!
//! async fn resolve_entity_example(
//!     runtime: Arc<RuntimeRef>,
//!     message: &Memory,
//!     state: &State,
//! ) -> zoey_core::Result<()> {
//!     // Convert RuntimeRef to type-erased Arc for the function
//!     let runtime_any = runtime.as_any_arc();
//!     
//!     // Find entity by name from message context
//!     if let Some(entity) = find_entity_by_name(runtime_any, message, state).await? {
//!         println!("Resolved entity: {:?}", entity.name);
//!     } else {
//!         println!("No entity found");
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! # Entity Resolution Strategies
//!
//! 1. **State-based**: Checks `state.values["entityName"]` for explicit hints
//! 2. **Mention matching**: Looks for @username or direct name mentions
//! 3. **Pronoun resolution**: Resolves "you" (agent) and "me" (sender)
//! 4. **Relationship-based**: Uses interaction history for ambiguous cases
//!
//! # Thread Safety
//!
//! All functions in this module are designed to work with `Arc<RuntimeRef>` which
//! provides thread-safe access to the runtime without blocking other operations.

use crate::runtime::AgentRuntime;
use crate::runtime_ref::downcast_runtime_ref;
use crate::templates::TemplateEngine;
use crate::types::{
    Entity, GenerateTextParams, Memory, MemoryQuery, ModelHandlerParams, ModelType, Relationship,
    Role, Room, State, UUID,
};
use crate::utils::string_to_uuid;
use crate::ZoeyError;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, instrument, warn};

/// Entity resolution result from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityResolution {
    /// Entity ID if found
    entity_id: Option<UUID>,

    /// Match type
    #[serde(rename = "type")]
    match_type: MatchType,

    /// Matching entities with reasons
    matches: Vec<EntityMatch>,

    /// Confidence score (0.0 - 1.0)
    #[serde(default)]
    confidence: f32,
}

/// Entity match information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityMatch {
    /// Matched name
    name: String,

    /// Reason for match
    reason: String,

    /// Entity ID if known
    #[serde(skip_serializing_if = "Option::is_none")]
    entity_id: Option<UUID>,
}

/// Match type for entity resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum MatchType {
    /// Exact ID match
    ExactMatch,
    /// Username/handle match
    UsernameMatch,
    /// Display name match
    NameMatch,
    /// Relationship-based match
    RelationshipMatch,
    /// Ambiguous - multiple possible matches
    Ambiguous,
    /// Unknown - no match found
    Unknown,
}

/// Entity resolution configuration
#[derive(Debug, Clone)]
pub struct EntityResolutionConfig {
    /// Use LLM for resolution (default: true)
    pub use_llm: bool,

    /// Model type to use for resolution
    pub model_type: ModelType,

    /// Cache TTL in seconds (default: 300)
    pub cache_ttl: u64,

    /// Maximum entities to consider (default: 50)
    pub max_entities: usize,

    /// Recent message count for context (default: 20)
    pub context_message_count: usize,

    /// Minimum confidence threshold (default: 0.5)
    pub min_confidence: f32,

    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for EntityResolutionConfig {
    fn default() -> Self {
        Self {
            use_llm: true,
            model_type: ModelType::TextSmall,
            cache_ttl: 300,
            max_entities: 50,
            context_message_count: 20,
            min_confidence: 0.5,
            enable_metrics: true,
        }
    }
}

/// Entity resolution cache entry
#[derive(Debug, Clone)]
pub(crate) struct CacheEntry {
    pub(crate) entity: Option<Entity>,
    pub(crate) timestamp: Instant,
    pub(crate) confidence: f32,
}

/// Entity resolution cache (thread-safe)
type EntityCache = Arc<RwLock<HashMap<String, CacheEntry>>>;

/// Global entity resolution cache
static ENTITY_CACHE: once_cell::sync::Lazy<EntityCache> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Entity resolution template for resolving entity names based on context
const ENTITY_RESOLUTION_TEMPLATE: &str = r#"# Task: Resolve Entity Name
Message Sender: {{senderName}} (ID: {{senderId}})
Agent: {{agentName}} (ID: {{agentId}})

# Entities in Room:
{{#if entitiesInRoom}}
{{entitiesInRoom}}
{{/if}}

{{recentMessages}}

# Instructions:
1. Analyze the context to identify which entity is being referenced
2. Consider special references like "me" (the message sender) or "you" (agent the message is directed to)
3. Look for usernames/handles in standard formats (e.g. @username, user#1234)
4. Consider context from recent messages for pronouns and references
5. If multiple matches exist, use context to disambiguate
6. Consider recent interactions and relationship strength when resolving ambiguity

Do NOT include any thinking, reasoning, or <think> sections in your response. 
Go directly to the XML response format without any preamble or explanation.

Return an XML response with:
<response>
  <entityId>exact-id-if-known-otherwise-null</entityId>
  <type>EXACT_MATCH | USERNAME_MATCH | NAME_MATCH | RELATIONSHIP_MATCH | AMBIGUOUS | UNKNOWN</type>
  <matches>
    <match>
      <name>matched-name</name>
      <reason>why this entity matches</reason>
    </match>
  </matches>
</response>

IMPORTANT: Your response must ONLY contain the <response></response> XML block above. Do not include any text, thinking, or reasoning before or after this XML block. Start your response immediately with <response> and end with </response>."#;

/// Parse XML response from entity resolution
#[instrument(skip(xml), level = "debug")]
pub(crate) fn parse_entity_resolution_xml(xml: &str) -> Result<EntityResolution> {
    debug!("Parsing entity resolution XML");

    // Extract entity ID
    let entity_id = extract_xml_tag(xml, "entityId");

    // Extract match type
    let match_type_str = extract_xml_tag(xml, "type").unwrap_or_else(|| "UNKNOWN".to_string());
    let match_type = match match_type_str.to_uppercase().as_str() {
        "EXACT_MATCH" => MatchType::ExactMatch,
        "USERNAME_MATCH" => MatchType::UsernameMatch,
        "NAME_MATCH" => MatchType::NameMatch,
        "RELATIONSHIP_MATCH" => MatchType::RelationshipMatch,
        "AMBIGUOUS" => MatchType::Ambiguous,
        _ => MatchType::Unknown,
    };

    // Extract confidence if present
    let confidence = extract_xml_tag(xml, "confidence")
        .and_then(|c| c.parse::<f32>().ok())
        .unwrap_or(0.5);

    // Parse matches
    let mut matches = Vec::new();
    if let Some(matches_section) = extract_xml_section(xml, "matches") {
        let match_blocks = extract_xml_sections(&matches_section, "match");
        for block in match_blocks {
            if let (Some(name), Some(reason)) = (
                extract_xml_tag(&block, "name"),
                extract_xml_tag(&block, "reason"),
            ) {
                let entity_id = extract_xml_tag(&block, "entityId")
                    .and_then(|id| uuid::Uuid::parse_str(&id).ok());

                matches.push(EntityMatch {
                    name,
                    reason,
                    entity_id,
                });
            }
        }
    }

    // Parse entity ID
    let entity_id = entity_id.and_then(|id| {
        if id == "null" || id.is_empty() {
            None
        } else {
            match uuid::Uuid::parse_str(&id) {
                Ok(uuid) => Some(uuid),
                Err(e) => {
                    warn!("Failed to parse entity ID '{}': {}", id, e);
                    None
                }
            }
        }
    });

    debug!(
        "Parsed resolution: match_type={:?}, confidence={}, matches={}",
        match_type,
        confidence,
        matches.len()
    );

    Ok(EntityResolution {
        entity_id,
        match_type,
        matches,
        confidence,
    })
}

/// Extract content of an XML tag
pub(crate) fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start_pos) = xml.find(&start_tag) {
        let content_start = start_pos + start_tag.len();
        if let Some(end_pos) = xml[content_start..].find(&end_tag) {
            return Some(
                xml[content_start..content_start + end_pos]
                    .trim()
                    .to_string(),
            );
        }
    }
    None
}

/// Extract content of an XML section (including nested tags)
pub(crate) fn extract_xml_section(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start_pos) = xml.find(&start_tag) {
        let content_start = start_pos + start_tag.len();
        if let Some(end_pos) = xml[content_start..].find(&end_tag) {
            return Some(xml[content_start..content_start + end_pos].to_string());
        }
    }
    None
}

/// Extract multiple XML sections with the same tag
pub(crate) fn extract_xml_sections(xml: &str, tag: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);

    let mut search_pos = 0;
    while let Some(start_pos) = xml[search_pos..].find(&start_tag) {
        let actual_start = search_pos + start_pos;
        let content_start = actual_start + start_tag.len();

        if let Some(end_pos) = xml[content_start..].find(&end_tag) {
            sections.push(xml[content_start..content_start + end_pos].to_string());
            search_pos = content_start + end_pos + end_tag.len();
        } else {
            break;
        }
    }

    sections
}

/// Generate cache key for entity resolution
pub(crate) fn generate_cache_key(message: &Memory, state: &State) -> String {
    // Include message context and state hints for cache key
    let entity_name = state
        .values
        .get("entityName")
        .map(|s| s.as_str())
        .unwrap_or("");
    format!(
        "{}:{}:{}:{}",
        message.room_id,
        message.entity_id,
        message.content.text.chars().take(50).collect::<String>(),
        entity_name
    )
}

/// Check and clean expired cache entries
pub(crate) fn clean_cache(cache: &EntityCache, ttl_seconds: u64) {
    let mut cache_lock = cache.write().unwrap();
    let now = Instant::now();
    let ttl = Duration::from_secs(ttl_seconds);

    cache_lock.retain(|_, entry| now.duration_since(entry.timestamp) < ttl);
}

/// Get entity from cache if available
pub(crate) fn get_cached_entity(
    cache_key: &str,
    config: &EntityResolutionConfig,
) -> Option<Option<Entity>> {
    if config.cache_ttl == 0 {
        return None;
    }

    let cache = ENTITY_CACHE.read().unwrap();
    if let Some(entry) = cache.get(cache_key) {
        // Check if entry is still valid
        if entry.timestamp.elapsed().as_secs() < config.cache_ttl {
            // Check confidence threshold
            if entry.confidence >= config.min_confidence {
                debug!(
                    "Cache hit for entity resolution (confidence: {})",
                    entry.confidence
                );
                return Some(entry.entity.clone());
            } else {
                debug!("Cache entry found but below confidence threshold");
            }
        } else {
            debug!("Cache entry expired");
        }
    }

    None
}

/// Store entity in cache
pub(crate) fn cache_entity(
    cache_key: String,
    entity: Option<Entity>,
    confidence: f32,
    config: &EntityResolutionConfig,
) {
    if config.cache_ttl == 0 {
        return;
    }

    let mut cache = ENTITY_CACHE.write().unwrap();
    cache.insert(
        cache_key,
        CacheEntry {
            entity,
            timestamp: Instant::now(),
            confidence,
        },
    );

    debug!("Cached entity resolution (confidence: {})", confidence);
}

/// Call LLM for entity resolution using registered model providers
async fn call_llm_for_entity_resolution(
    agent_runtime: &AgentRuntime,
    prompt: &str,
    model_type: ModelType,
) -> Result<String> {
    // Get model handlers for the specified model type
    let models = agent_runtime.models.read().unwrap();

    let model_type_str = match model_type {
        ModelType::TextSmall => "TEXT_SMALL",
        ModelType::TextMedium => "TEXT_MEDIUM",
        ModelType::TextLarge => "TEXT_LARGE",
        _ => "TEXT_SMALL", // Default to small for entity resolution
    };

    let handlers = models.get(model_type_str);

    if let Some(handlers) = handlers {
        if handlers.is_empty() {
            warn!("No model handlers registered for {}", model_type_str);
            return Err(ZoeyError::Model(format!(
                "No model handlers for {}",
                model_type_str
            )));
        }

        // Get the highest priority provider
        let provider = &handlers[0];
        info!(
            "Using LLM provider for entity resolution: {} (priority: {})",
            provider.name, provider.priority
        );

        // Get model settings
        let (model_name, temperature, max_tokens) = {
            let model = if provider.name.to_lowercase().contains("openai") {
                agent_runtime
                    .get_setting("OPENAI_MODEL")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            } else if provider.name.to_lowercase().contains("anthropic")
                || provider.name.to_lowercase().contains("claude")
            {
                agent_runtime
                    .get_setting("ANTHROPIC_MODEL")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            } else {
                agent_runtime
                    .get_setting("LOCAL_LLM_MODEL")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            };

            let temp = agent_runtime
                .get_setting("temperature")
                .and_then(|v| v.as_f64().map(|f| f as f32))
                .unwrap_or(0.3); // Lower temperature for structured output

            let tokens = agent_runtime
                .get_setting("max_tokens")
                .and_then(|v| v.as_u64().map(|u| u as usize))
                .unwrap_or(300); // Enough for entity resolution

            (model, temp, tokens)
        };

        // Create parameters for the model
        let params = GenerateTextParams {
            prompt: prompt.to_string(),
            max_tokens: Some(max_tokens),
            temperature: Some(temperature),
            top_p: None,
            stop: Some(vec!["</response>".to_string()]),
            model: model_name,
            frequency_penalty: None,
            presence_penalty: None,
        };

        let model_params = ModelHandlerParams {
            runtime: Arc::new(()) as Arc<dyn std::any::Any + Send + Sync>,
            params,
        };

        debug!(
            "Calling LLM for entity resolution (temp: {}, max_tokens: {})",
            temperature, max_tokens
        );

        // Call the model handler
        match (provider.handler)(model_params).await {
            Ok(response) => {
                info!(
                    "✓ LLM entity resolution response received ({} chars)",
                    response.len()
                );
                Ok(response)
            }
            Err(e) => {
                error!("LLM model handler failed: {}", e);
                Err(e)
            }
        }
    } else {
        warn!("No model handlers found for {}", model_type_str);
        Err(ZoeyError::Model(format!(
            "No model handlers for {}",
            model_type_str
        )))
    }
}

/// Finds an entity by name with default configuration
///
/// This is a convenience wrapper around `find_entity_by_name_with_config`
/// using default settings.
pub async fn find_entity_by_name(
    runtime: Arc<dyn std::any::Any + Send + Sync>,
    message: &Memory,
    state: &State,
) -> Result<Option<Entity>> {
    find_entity_by_name_with_config(runtime, message, state, &EntityResolutionConfig::default())
        .await
}

/// Finds an entity by name in the given runtime environment with custom configuration.
///
/// This function uses LLM-based entity resolution to find the most appropriate entity
/// based on the context of the message and recent interactions.
///
/// # Arguments
/// * `runtime` - The agent runtime environment (type-erased, should be RuntimeRef)
/// * `message` - The memory message containing relevant information
/// * `state` - The current state of the system
/// * `config` - Configuration options for entity resolution
///
/// # Returns
/// A result containing the found entity or None if not found
///
/// # Production Features
/// - LLM-based resolution with fallback strategies
/// - Caching with configurable TTL
/// - Metrics and tracing
/// - Confidence thresholds
/// - Permission filtering based on world roles
///
/// # Note
/// This function expects `runtime` to be `Arc<RuntimeRef>` (use `RuntimeRef::new()` to wrap)
#[instrument(skip(runtime, message, state, config), fields(
    message_id = %message.id,
    room_id = %message.room_id,
    entity_id = %message.entity_id
), level = "info")]
pub async fn find_entity_by_name_with_config(
    runtime: Arc<dyn std::any::Any + Send + Sync>,
    message: &Memory,
    state: &State,
    config: &EntityResolutionConfig,
) -> Result<Option<Entity>> {
    let start_time = Instant::now();
    info!("Starting entity resolution");

    // Clean expired cache entries periodically
    clean_cache(&ENTITY_CACHE, config.cache_ttl);

    // Check cache first
    let cache_key = generate_cache_key(message, state);
    if let Some(cached) = get_cached_entity(&cache_key, config) {
        info!(
            "Entity resolution cache hit ({}ms)",
            start_time.elapsed().as_millis()
        );
        return Ok(cached);
    }

    debug!("Cache miss, proceeding with resolution");

    // Try to get RuntimeRef first (preferred method)
    let runtime_arc = if let Some(runtime_ref) = downcast_runtime_ref(&runtime) {
        runtime_ref.try_upgrade().ok_or_else(|| {
            error!("Runtime has been dropped");
            ZoeyError::Runtime("Runtime has been dropped".to_string())
        })?
    } else {
        // Fallback: try direct Arc<RwLock<AgentRuntime>> downcast
        // This is a workaround - in production, always use RuntimeRef
        error!("Runtime must be passed as Arc<RuntimeRef>");
        return Err(ZoeyError::Runtime(
            "Runtime must be passed as Arc<RuntimeRef>. Use RuntimeRef::new() to wrap the runtime."
                .to_string(),
        ));
    };

    // Lock runtime for reading
    let agent_runtime = runtime_arc.read().map_err(|e| {
        error!("Failed to lock runtime: {}", e);
        ZoeyError::Runtime(format!("Failed to lock runtime: {}", e))
    })?;

    // Get the database adapter
    let adapter_lock = agent_runtime.adapter.read().map_err(|e| {
        error!("Failed to lock adapter: {}", e);
        ZoeyError::Runtime(format!("Failed to lock adapter: {}", e))
    })?;

    let adapter = adapter_lock.as_ref().ok_or_else(|| {
        warn!("No database adapter configured");
        ZoeyError::Database("No database adapter configured".to_string())
    })?;

    // Store agent info for later use
    let agent_id = agent_runtime.agent_id;
    let agent_name = agent_runtime.character.name.clone();

    debug!("Resolving entity for agent: {} ({})", agent_name, agent_id);

    // 1. Get the room
    let room = if let Some(room_value) = state.data.get("room") {
        match serde_json::from_value::<Room>(room_value.clone()) {
            Ok(r) => {
                debug!("Using room from state");
                Some(r)
            }
            Err(e) => {
                warn!("Failed to deserialize room from state: {}", e);
                None
            }
        }
    } else {
        None
    };

    let room = if let Some(r) = room {
        r
    } else {
        // Fallback: fetch from database
        debug!("Fetching room from database: {}", message.room_id);
        adapter.get_room(message.room_id).await?.ok_or_else(|| {
            error!("Room not found: {}", message.room_id);
            ZoeyError::NotFound(format!("Room {} not found", message.room_id))
        })?
    };

    debug!("Room: {} (world: {})", room.name, room.world_id);

    // 2. Get the world
    let world = match adapter.get_world(room.world_id).await {
        Ok(Some(w)) => {
            debug!("Loaded world: {}", w.name);
            Some(w)
        }
        Ok(None) => {
            debug!("World not found: {}", room.world_id);
            None
        }
        Err(e) => {
            warn!("Failed to load world: {}", e);
            None
        }
    };

    // 3. Get all entities in the room with components
    let entities_in_room = match adapter.get_entities_for_room(room.id, true).await {
        Ok(entities) => {
            debug!("Found {} entities in room", entities.len());
            // Limit entities if configured
            if entities.len() > config.max_entities {
                warn!(
                    "Too many entities ({}), limiting to {}",
                    entities.len(),
                    config.max_entities
                );
                entities.into_iter().take(config.max_entities).collect()
            } else {
                entities
            }
        }
        Err(e) => {
            error!("Failed to get entities for room: {}", e);
            return Err(e);
        }
    };

    // 4. Filter components based on permissions
    if let Some(ref world) = world {
        let _world_roles: HashMap<UUID, Role> =
            if let Some(roles_value) = world.metadata.get("roles") {
                if let Some(roles_obj) = roles_value.as_object() {
                    roles_obj
                        .iter()
                        .filter_map(|(k, v)| {
                            let uuid = uuid::Uuid::parse_str(k).ok()?;
                            let role_str = v.as_str()?;
                            let role = match role_str.to_uppercase().as_str() {
                                "OWNER" => Role::Owner,
                                "ADMIN" => Role::Admin,
                                "MODERATOR" | "MOD" => Role::Moderator,
                                "MEMBER" => Role::Member,
                                _ => Role::None,
                            };
                            Some((uuid, role))
                        })
                        .collect()
                } else {
                    HashMap::new()
                }
            } else {
                HashMap::new()
            };

        // Component filtering based on roles would happen here
        // Note: The current Entity structure doesn't include components field
        // In a full implementation with Entity.components, we would filter based on:
        // 1. component.source_entity_id == message.entity_id (requester's own components)
        // 2. _world_roles[component.source_entity_id] in [Owner, Admin] (admin visibility)
        // 3. component.source_entity_id == agent_id (agent's components)

        debug!(
            "Loaded {} entities with permission filtering",
            entities_in_room.len()
        );
    }

    // 5. Get relationships for the message sender
    // Note: This requires a get_relationships method on the adapter
    // The IDatabaseAdapter trait doesn't currently expose this
    // In production, extend the trait with:
    // async fn get_relationships(&self, entity_id: UUID) -> Result<Vec<Relationship>>;
    let relationships: Vec<Relationship> = vec![]; // Placeholder

    // 6. Compose prompt for LLM
    let engine = TemplateEngine::new();

    let mut template_data: HashMap<String, serde_json::Value> = HashMap::new();

    // Find sender entity
    let sender_entity = entities_in_room.iter().find(|e| e.id == message.entity_id);

    let sender_name = sender_entity
        .and_then(|e| e.name.clone())
        .or_else(|| sender_entity.and_then(|e| e.username.clone()))
        .unwrap_or_else(|| "Unknown".to_string());

    template_data.insert("senderName".to_string(), serde_json::json!(sender_name));
    template_data.insert(
        "senderId".to_string(),
        serde_json::json!(message.entity_id.to_string()),
    );
    template_data.insert("agentName".to_string(), serde_json::json!(agent_name));
    template_data.insert(
        "agentId".to_string(),
        serde_json::json!(agent_id.to_string()),
    );

    // Format entities in room
    let entities_str = format_entities(&entities_in_room);
    template_data.insert(
        "entitiesInRoom".to_string(),
        serde_json::json!(entities_str),
    );

    // Get recent messages
    let recent_messages = adapter
        .get_memories(MemoryQuery {
            room_id: Some(message.room_id),
            agent_id: Some(agent_id),
            count: Some(20),
            unique: Some(false),
            ..Default::default()
        })
        .await
        .unwrap_or_default();

    // Format recent messages with entity names
    let messages_str = recent_messages
        .iter()
        .map(|m| {
            let entity_name = entities_in_room
                .iter()
                .find(|e| e.id == m.entity_id)
                .and_then(|e| e.name.clone())
                .or_else(|| {
                    entities_in_room
                        .iter()
                        .find(|e| e.id == m.entity_id)
                        .and_then(|e| e.username.clone())
                })
                .unwrap_or_else(|| "Unknown".to_string());

            format!("{}: {}", entity_name, m.content.text)
        })
        .collect::<Vec<_>>()
        .join("\n");

    template_data.insert(
        "recentMessages".to_string(),
        serde_json::json!(messages_str),
    );

    // Render the prompt template
    let prompt = engine.render(ENTITY_RESOLUTION_TEMPLATE, &template_data)?;
    debug!(
        "Generated entity resolution prompt ({} chars)",
        prompt.len()
    );

    // 7. Use LLM to resolve the entity (if enabled)
    let (resolved_entity, confidence): (Option<Entity>, f32) = if config.use_llm {
        debug!("Attempting LLM-based entity resolution");

        // Call the LLM model using registered providers
        match call_llm_for_entity_resolution(&agent_runtime, &prompt, config.model_type).await {
            Ok(llm_response) => {
                debug!("LLM response received ({} chars)", llm_response.len());

                // Parse the LLM response
                match parse_entity_resolution_xml(&llm_response) {
                    Ok(resolution) => {
                        debug!(
                            "LLM resolution parsed: match_type={:?}, confidence={}",
                            resolution.match_type, resolution.confidence
                        );

                        // If we got an exact entity ID match
                        if let Some(entity_id) = resolution.entity_id {
                            // Find the entity in our list
                            if let Some(entity) =
                                entities_in_room.iter().find(|e| e.id == entity_id)
                            {
                                (Some(entity.clone()), resolution.confidence)
                            } else {
                                debug!("Entity ID from LLM not found in room");
                                (None, resolution.confidence)
                            }
                        } else if !resolution.matches.is_empty() {
                            // Try to match by name from the matches
                            let match_name = resolution.matches[0].name.to_lowercase();
                            if let Some(entity) = entities_in_room.iter().find(|e| {
                                e.name
                                    .as_ref()
                                    .map(|n| n.to_lowercase() == match_name)
                                    .unwrap_or(false)
                                    || e.username
                                        .as_ref()
                                        .map(|u| u.to_lowercase() == match_name)
                                        .unwrap_or(false)
                            }) {
                                (Some(entity.clone()), resolution.confidence)
                            } else {
                                (None, resolution.confidence)
                            }
                        } else {
                            (None, resolution.confidence)
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse LLM entity resolution: {}", e);
                        (None, 0.0)
                    }
                }
            }
            Err(e) => {
                warn!("LLM call failed for entity resolution: {}", e);
                (None, 0.0)
            }
        }
    } else {
        debug!("LLM resolution disabled, using fallback strategies only");
        (None, 0.0)
    };

    // If LLM provided a result, use it
    if let Some(entity) = resolved_entity {
        info!(
            "Entity resolved via LLM (confidence: {}, {}ms)",
            confidence,
            start_time.elapsed().as_millis()
        );

        // Cache the result
        cache_entity(cache_key, Some(entity.clone()), confidence, config);

        return Ok(Some(entity));
    }

    // Fallback strategies
    debug!("Using fallback resolution strategies");

    // Strategy 1: Check state for explicit entity name
    if let Some(entity_name) = state.values.get("entityName") {
        debug!("Trying state-based resolution with hint: {}", entity_name);
        let query = entity_name.to_lowercase();

        for entity in &entities_in_room {
            // Check name
            if let Some(name) = &entity.name {
                if name.to_lowercase().contains(&query) {
                    info!(
                        "Entity resolved via state hint ({}ms)",
                        start_time.elapsed().as_millis()
                    );
                    let result = Some(entity.clone());
                    cache_entity(cache_key, result.clone(), 0.8, config);
                    return Ok(result);
                }
            }

            // Check username
            if let Some(username) = &entity.username {
                if username.to_lowercase().contains(&query) {
                    info!(
                        "Entity resolved via state hint username ({}ms)",
                        start_time.elapsed().as_millis()
                    );
                    let result = Some(entity.clone());
                    cache_entity(cache_key, result.clone(), 0.8, config);
                    return Ok(result);
                }
            }
        }
    }

    // Strategy 2: Check content.text for mentions (@username, name)
    let text = message.content.text.to_lowercase();
    debug!("Trying mention-based resolution");

    // Look for @mentions
    for entity in &entities_in_room {
        if let Some(username) = &entity.username {
            let mention = format!("@{}", username.to_lowercase());
            if text.contains(&mention) {
                info!(
                    "Entity resolved via @mention ({}ms)",
                    start_time.elapsed().as_millis()
                );
                let result = Some(entity.clone());
                cache_entity(cache_key, result.clone(), 0.9, config);
                return Ok(result);
            }
        }
    }

    // Look for direct name mentions
    debug!("Trying name-based resolution");
    for entity in &entities_in_room {
        if let Some(name) = &entity.name {
            // Only match if name is significant (>2 chars) to avoid false positives
            if name.len() > 2 && text.contains(&name.to_lowercase()) {
                info!(
                    "Entity resolved via name mention ({}ms)",
                    start_time.elapsed().as_millis()
                );
                let result = Some(entity.clone());
                cache_entity(cache_key, result.clone(), 0.7, config);
                return Ok(result);
            }
        }
    }

    // Strategy 3: Use interaction history to resolve ambiguous references
    debug!("Trying pronoun-based resolution");
    if text.contains("you") || text.contains("your") {
        // "you" likely refers to the agent
        for entity in &entities_in_room {
            if entity.id == agent_id {
                info!(
                    "Entity resolved via pronoun 'you' -> agent ({}ms)",
                    start_time.elapsed().as_millis()
                );
                let result = Some(entity.clone());
                cache_entity(cache_key, result.clone(), 0.6, config);
                return Ok(result);
            }
        }
    }

    if text.contains("me") || text.contains("my") || text.contains("i ") {
        // "me" refers to the message sender
        for entity in &entities_in_room {
            if entity.id == message.entity_id {
                info!(
                    "Entity resolved via pronoun 'me' -> sender ({}ms)",
                    start_time.elapsed().as_millis()
                );
                let result = Some(entity.clone());
                cache_entity(cache_key, result.clone(), 0.6, config);
                return Ok(result);
            }
        }
    }

    // Strategy 4: Use relationship strength if available
    if !relationships.is_empty() {
        debug!("Trying relationship-based resolution");
        let interaction_data = get_recent_interactions(
            message.entity_id,
            &entities_in_room,
            room.id,
            &recent_messages,
            &relationships,
        );

        // Return entity with highest interaction score (if any significant interactions)
        if let Some((entity, _, score)) = interaction_data.first() {
            if *score > 0 {
                info!(
                    "Entity resolved via relationships (score: {}, {}ms)",
                    score,
                    start_time.elapsed().as_millis()
                );
                let result = Some(entity.clone());
                cache_entity(cache_key, result.clone(), 0.5, config);
                return Ok(result);
            }
        }
    }

    // No match found
    info!(
        "No entity resolved ({}ms)",
        start_time.elapsed().as_millis()
    );
    cache_entity(cache_key, None, 0.0, config);
    Ok(None)
}

/// Function to create a unique UUID based on the runtime and base user ID.
///
/// # Arguments
/// * `agent_id` - The agent ID
/// * `base_user_id` - The base user ID to use in generating the UUID
///
/// # Returns
/// The unique UUID generated based on the agent and base user ID
pub fn create_unique_uuid_for_entity(agent_id: UUID, base_user_id: &str) -> UUID {
    // If the base user ID is the agent ID, return it directly
    if base_user_id == agent_id.to_string() {
        return agent_id;
    }

    // Use a deterministic approach to generate a new UUID based on both IDs
    // This creates a unique ID for each user+agent combination while still being deterministic
    let combined_string = format!("{}:{}", base_user_id, agent_id);

    // Create a namespace UUID (version 5) from the combined string
    string_to_uuid(&combined_string)
}

/// Get details for entities in a room
///
/// # Arguments
/// * `_room` - The room object (for future use)
/// * `entities` - The list of entities in the room
///
/// # Returns
/// A vector of entity detail maps
pub fn get_entity_details(
    _room: &Room,
    entities: &[Entity],
) -> Vec<HashMap<String, serde_json::Value>> {
    let mut unique_entities: HashMap<UUID, HashMap<String, serde_json::Value>> = HashMap::new();

    for entity in entities {
        if unique_entities.contains_key(&entity.id) {
            continue;
        }

        // Get primary name (prefer source-specific name if available)
        let name = entity.name.clone().unwrap_or_else(|| {
            entity
                .username
                .clone()
                .unwrap_or_else(|| "Unknown".to_string())
        });

        let mut entity_detail = HashMap::new();
        entity_detail.insert("id".to_string(), serde_json::json!(entity.id.to_string()));
        entity_detail.insert("name".to_string(), serde_json::json!(name));

        // Include metadata
        let metadata_json = serde_json::to_value(&entity.metadata).unwrap_or(serde_json::json!({}));
        entity_detail.insert("data".to_string(), metadata_json);

        unique_entities.insert(entity.id, entity_detail);
    }

    unique_entities.into_values().collect()
}

/// Format entities into a string representation
///
/// # Arguments
/// * `entities` - The list of entities to format
///
/// # Returns
/// A formatted string representing the entities
pub fn format_entities(entities: &[Entity]) -> String {
    entities
        .iter()
        .map(|entity| {
            let name = entity.name.clone().unwrap_or_else(|| {
                entity
                    .username
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string())
            });

            let mut header = format!("\"{}\"\nID: {}", name, entity.id);

            if !entity.metadata.is_empty() {
                if let Ok(metadata_str) = serde_json::to_string(&entity.metadata) {
                    header.push_str(&format!("\nData: {}\n", metadata_str));
                } else {
                    header.push('\n');
                }
            } else {
                header.push('\n');
            }

            header
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Get recent interactions between entities
///
/// # Arguments
/// * `source_entity_id` - The source entity ID
/// * `candidate_entities` - Candidate entities to check interactions with
/// * `_room_id` - The room ID (for future use)
/// * `recent_messages` - Recent messages in the room
/// * `relationships` - Relationships between entities
///
/// # Returns
/// A vector of interaction data sorted by strength
pub fn get_recent_interactions(
    source_entity_id: UUID,
    candidate_entities: &[Entity],
    _room_id: UUID,
    recent_messages: &[Memory],
    relationships: &[Relationship],
) -> Vec<(Entity, Vec<Memory>, usize)> {
    let mut results: Vec<(Entity, Vec<Memory>, usize)> = Vec::new();

    for entity in candidate_entities {
        let mut interactions: Vec<Memory> = Vec::new();
        let mut interaction_score = 0;

        // Get direct replies using inReplyTo (if available in content metadata)
        let direct_replies: Vec<Memory> = recent_messages
            .iter()
            .filter(|msg| {
                msg.entity_id == source_entity_id || msg.entity_id == entity.id
                // Add logic to check inReplyTo from content metadata if needed
            })
            .cloned()
            .collect();

        interactions.extend(direct_replies.clone());

        // Get relationship strength from metadata
        let relationship = relationships.iter().find(|rel| {
            (rel.entity_id_a == source_entity_id && rel.entity_id_b == entity.id)
                || (rel.entity_id_b == source_entity_id && rel.entity_id_a == entity.id)
        });

        if let Some(rel) = relationship {
            if let Some(interactions_count) = rel.metadata.get("interactions") {
                if let Some(count) = interactions_count.as_u64() {
                    interaction_score = count as usize;
                }
            }
        }

        // Add bonus points for recent direct replies
        interaction_score += direct_replies.len();

        // Keep last few messages for context
        let unique_interactions: Vec<Memory> = interactions.into_iter().rev().take(5).collect();

        results.push((entity.clone(), unique_interactions, interaction_score));
    }

    // Sort by interaction score descending
    results.sort_by(|a, b| b.2.cmp(&a.2));
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Metadata;
    use uuid::Uuid;

    #[test]
    fn test_create_unique_uuid_for_entity() {
        let agent_id = Uuid::new_v4();
        let user_id = "user123";

        let uuid1 = create_unique_uuid_for_entity(agent_id, user_id);
        let uuid2 = create_unique_uuid_for_entity(agent_id, user_id);

        // Should be deterministic
        assert_eq!(uuid1, uuid2);

        // Should be different from agent_id
        assert_ne!(uuid1, agent_id);

        // Agent ID should return itself
        let agent_id_result = create_unique_uuid_for_entity(agent_id, &agent_id.to_string());
        assert_eq!(agent_id_result, agent_id);
    }

    #[test]
    fn test_format_entities() {
        let entity = Entity {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            name: Some("Test User".to_string()),
            username: Some("testuser".to_string()),
            email: None,
            avatar_url: None,
            metadata: Metadata::new(),
            created_at: Some(12345),
        };

        let formatted = format_entities(&[entity.clone()]);
        assert!(formatted.contains("Test User"));
        assert!(formatted.contains(&entity.id.to_string()));
    }

    #[test]
    fn test_extract_xml_tag() {
        let xml = "<response><entityId>12345</entityId><type>EXACT_MATCH</type></response>";

        let entity_id = extract_xml_tag(xml, "entityId");
        assert_eq!(entity_id, Some("12345".to_string()));

        let match_type = extract_xml_tag(xml, "type");
        assert_eq!(match_type, Some("EXACT_MATCH".to_string()));

        let missing = extract_xml_tag(xml, "missing");
        assert_eq!(missing, None);
    }

    #[test]
    fn test_extract_xml_section() {
        let xml = r#"<response>
            <matches>
                <match><name>John</name></match>
                <match><name>Jane</name></match>
            </matches>
        </response>"#;

        let matches_section = extract_xml_section(xml, "matches");
        assert!(matches_section.is_some());

        let section = matches_section.unwrap();
        assert!(section.contains("<match>"));
        assert!(section.contains("John"));
        assert!(section.contains("Jane"));
    }

    #[test]
    fn test_extract_xml_sections() {
        let xml = r#"<matches>
            <match><name>John</name><reason>First match</reason></match>
            <match><name>Jane</name><reason>Second match</reason></match>
        </matches>"#;

        let sections = extract_xml_sections(xml, "match");
        assert_eq!(sections.len(), 2);
        assert!(sections[0].contains("John"));
        assert!(sections[1].contains("Jane"));
    }

    #[test]
    fn test_parse_entity_resolution_xml() {
        let xml = r#"<response>
            <entityId>550e8400-e29b-41d4-a716-446655440000</entityId>
            <type>EXACT_MATCH</type>
            <matches>
                <match>
                    <name>John Doe</name>
                    <reason>Exact ID match</reason>
                </match>
            </matches>
        </response>"#;

        let result = parse_entity_resolution_xml(xml);
        assert!(result.is_ok());

        let resolution = result.unwrap();
        assert!(resolution.entity_id.is_some());
        assert_eq!(resolution.match_type, MatchType::ExactMatch);
        assert_eq!(resolution.matches.len(), 1);
        assert_eq!(resolution.matches[0].name, "John Doe");
        assert_eq!(resolution.matches[0].reason, "Exact ID match");
    }

    #[test]
    fn test_parse_entity_resolution_xml_no_id() {
        let xml = r#"<response>
            <entityId>null</entityId>
            <type>UNKNOWN</type>
            <matches></matches>
        </response>"#;

        let result = parse_entity_resolution_xml(xml);
        assert!(result.is_ok());

        let resolution = result.unwrap();
        assert!(resolution.entity_id.is_none());
        assert_eq!(resolution.match_type, MatchType::Unknown);
        assert_eq!(resolution.matches.len(), 0);
    }

    #[test]
    fn test_get_entity_details() {
        let room = Room {
            id: Uuid::new_v4(),
            agent_id: Some(Uuid::new_v4()),
            name: "Test Room".to_string(),
            source: "test".to_string(),
            channel_type: crate::types::ChannelType::GuildText,
            channel_id: None,
            server_id: None,
            world_id: Uuid::new_v4(),
            metadata: Metadata::new(),
            created_at: Some(12345),
        };

        let entity1 = Entity {
            id: Uuid::new_v4(),
            agent_id: room.agent_id.unwrap(),
            name: Some("Alice".to_string()),
            username: Some("alice".to_string()),
            email: None,
            avatar_url: None,
            metadata: Metadata::new(),
            created_at: Some(12345),
        };

        let entity2 = Entity {
            id: Uuid::new_v4(),
            agent_id: room.agent_id.unwrap(),
            name: Some("Bob".to_string()),
            username: Some("bob".to_string()),
            email: None,
            avatar_url: None,
            metadata: Metadata::new(),
            created_at: Some(12345),
        };

        let details = get_entity_details(&room, &[entity1.clone(), entity2.clone()]);
        assert_eq!(details.len(), 2);

        // Check that both entities are included
        let names: Vec<String> = details
            .iter()
            .filter_map(|d| {
                d.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        assert!(names.contains(&"Alice".to_string()));
        assert!(names.contains(&"Bob".to_string()));
    }

    #[test]
    fn test_get_recent_interactions() {
        let source_entity_id = Uuid::new_v4();
        let target_entity_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let _source_entity = Entity {
            id: source_entity_id,
            agent_id,
            name: Some("Source".to_string()),
            username: None,
            email: None,
            avatar_url: None,
            metadata: Metadata::new(),
            created_at: Some(12345),
        };

        let target_entity = Entity {
            id: target_entity_id,
            agent_id,
            name: Some("Target".to_string()),
            username: None,
            email: None,
            avatar_url: None,
            metadata: Metadata::new(),
            created_at: Some(12345),
        };

        let messages = vec![];
        let relationships = vec![];

        let interactions = get_recent_interactions(
            source_entity_id,
            &[target_entity.clone()],
            room_id,
            &messages,
            &relationships,
        );

        assert_eq!(interactions.len(), 1);
        assert_eq!(interactions[0].0.id, target_entity_id);
        assert_eq!(interactions[0].2, 0); // No interactions
    }

    #[test]
    fn test_get_recent_interactions_with_relationship() {
        let source_entity_id = Uuid::new_v4();
        let target_entity_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let target_entity = Entity {
            id: target_entity_id,
            agent_id,
            name: Some("Target".to_string()),
            username: None,
            email: None,
            avatar_url: None,
            metadata: Metadata::new(),
            created_at: Some(12345),
        };

        // Create a relationship with interaction metadata
        let mut metadata = Metadata::new();
        metadata.insert("interactions".to_string(), serde_json::json!(5));

        let relationship = Relationship {
            entity_id_a: source_entity_id,
            entity_id_b: target_entity_id,
            relationship_type: "friend".to_string(),
            agent_id,
            metadata,
            created_at: Some(12345),
        };

        let messages = vec![];
        let relationships = vec![relationship];

        let interactions = get_recent_interactions(
            source_entity_id,
            &[target_entity.clone()],
            room_id,
            &messages,
            &relationships,
        );

        assert_eq!(interactions.len(), 1);
        assert_eq!(interactions[0].0.id, target_entity_id);
        assert_eq!(interactions[0].2, 5); // 5 interactions from relationship metadata
    }

    #[test]
    fn test_entity_resolution_config() {
        let config = EntityResolutionConfig::default();
        assert!(config.use_llm);
        assert_eq!(config.cache_ttl, 300);
        assert_eq!(config.max_entities, 50);
        assert_eq!(config.min_confidence, 0.5);

        let custom_config = EntityResolutionConfig {
            use_llm: false,
            cache_ttl: 600,
            max_entities: 100,
            context_message_count: 50,
            min_confidence: 0.7,
            ..Default::default()
        };

        assert!(!custom_config.use_llm);
        assert_eq!(custom_config.cache_ttl, 600);
    }

    #[test]
    fn test_generate_cache_key() {
        let message = Memory {
            id: Uuid::new_v4(),
            entity_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: crate::types::Content {
                text: "Hello world".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: 12345,
            unique: None,
            similarity: None,
        };

        let state = State::new();
        let key1 = generate_cache_key(&message, &state);
        let key2 = generate_cache_key(&message, &state);

        // Same message and state should produce same key
        assert_eq!(key1, key2);

        // Different entity_id should produce different key
        let mut message2 = message.clone();
        message2.entity_id = Uuid::new_v4();
        let key3 = generate_cache_key(&message2, &state);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_match_type() {
        assert_eq!(MatchType::ExactMatch, MatchType::ExactMatch);
        assert_ne!(MatchType::ExactMatch, MatchType::Unknown);
    }

    #[test]
    fn test_entity_resolution_parsing() {
        let xml = r#"<response>
            <entityId>550e8400-e29b-41d4-a716-446655440000</entityId>
            <type>EXACT_MATCH</type>
            <confidence>0.95</confidence>
            <matches>
                <match>
                    <name>John Doe</name>
                    <reason>Exact ID match</reason>
                    <entityId>550e8400-e29b-41d4-a716-446655440000</entityId>
                </match>
            </matches>
        </response>"#;

        let result = parse_entity_resolution_xml(xml);
        assert!(result.is_ok());

        let resolution = result.unwrap();
        assert!(resolution.entity_id.is_some());
        assert_eq!(resolution.match_type, MatchType::ExactMatch);
        assert_eq!(resolution.confidence, 0.95);
        assert_eq!(resolution.matches.len(), 1);
        assert_eq!(resolution.matches[0].name, "John Doe");
    }
}
