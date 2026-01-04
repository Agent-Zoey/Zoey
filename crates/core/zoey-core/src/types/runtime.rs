//! Runtime trait definition

use super::*;
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Agent runtime interface - the main interface for agent operations
#[async_trait]
pub trait IAgentRuntime: Send + Sync {
    // Properties
    /// Agent ID
    fn agent_id(&self) -> UUID;

    /// Character configuration
    fn character(&self) -> &Character;

    /// Message service
    fn message_service(&self) -> Option<Arc<dyn IMessageService>>;

    /// Registered providers
    fn providers(&self) -> &[Arc<dyn Provider>];

    /// Registered actions
    fn actions(&self) -> &[Arc<dyn Action>];

    /// Registered evaluators
    fn evaluators(&self) -> &[Arc<dyn Evaluator>];

    /// Registered plugins
    fn plugins(&self) -> &[Arc<dyn Plugin>];

    /// Services map
    fn services(&self) -> &HashMap<ServiceTypeName, Vec<Arc<dyn Service>>>;

    /// Event handlers
    fn events(&self) -> &HashMap<String, Vec<EventHandler>>;

    /// HTTP routes
    fn routes(&self) -> &[Route];

    /// Logger
    fn logger(&self) -> &dyn std::any::Any;

    /// State cache
    fn state_cache(&self) -> &HashMap<String, State>;

    /// ZoeyOS instance reference (optional)
    fn zoey_os(&self) -> Option<Arc<dyn std::any::Any>>;

    /// Initialization promise/future
    fn init_promise(&self) -> Arc<dyn std::any::Any + Send + Sync>;

    // Plugin management
    /// Register a plugin
    async fn register_plugin(&mut self, plugin: Arc<dyn Plugin>) -> Result<()>;

    /// Initialize runtime
    async fn initialize(&mut self, options: InitializeOptions) -> Result<()>;

    /// Get database connection (type-erased)
    async fn get_connection(&self) -> Result<Box<dyn std::any::Any + Send>>;

    // Service management
    /// Get service by name or type
    fn get_service(&self, service_type: &str) -> Option<Arc<dyn Service>>;

    /// Get all services of a type
    fn get_services_by_type(&self, service_type: &str) -> Vec<Arc<dyn Service>>;

    /// Get all services
    fn get_all_services(&self) -> &HashMap<ServiceTypeName, Vec<Arc<dyn Service>>>;

    /// Register service type
    async fn register_service(&mut self, service: Arc<dyn Service>) -> Result<()>;

    /// Get service load promise
    async fn get_service_load_promise(&self, service_type: &str) -> Result<Arc<dyn Service>>;

    /// Get registered service types
    fn get_registered_service_types(&self) -> Vec<ServiceTypeName>;

    /// Check if service exists
    fn has_service(&self, service_type: &str) -> bool;

    /// Check if ZoeyOS instance exists
    fn has_zoey_os(&self) -> bool;

    // Database adapter management
    /// Register database adapter
    fn register_database_adapter(&mut self, adapter: Arc<dyn IDatabaseAdapter>);

    // Settings management
    /// Set a setting
    fn set_setting(&mut self, key: &str, value: serde_json::Value, secret: bool);

    /// Get a setting
    fn get_setting(&self, key: &str) -> Option<serde_json::Value>;

    /// Get conversation length
    fn get_conversation_length(&self) -> usize;

    // Action processing
    /// Process actions for a message
    async fn process_actions(
        &self,
        message: &Memory,
        responses: &[Memory],
        state: &State,
        callback: Option<HandlerCallback>,
    ) -> Result<()>;

    /// Get action results for a message
    fn get_action_results(&self, message_id: UUID) -> Vec<ActionResult>;

    // Evaluation
    /// Evaluate a message
    async fn evaluate(
        &self,
        message: &Memory,
        state: &State,
        did_respond: bool,
        callback: Option<HandlerCallback>,
        responses: Option<Vec<Memory>>,
    ) -> Result<Vec<Arc<dyn Evaluator>>>;

    // Provider management
    /// Register a provider
    fn register_provider(&mut self, provider: Arc<dyn Provider>);

    /// Register an action
    fn register_action(&mut self, action: Arc<dyn Action>);

    /// Register an evaluator
    fn register_evaluator(&mut self, evaluator: Arc<dyn Evaluator>);

    // Entity/Room/World management
    /// Ensure connections for entities
    async fn ensure_connections(
        &self,
        entities: Vec<Entity>,
        rooms: Vec<Room>,
        source: &str,
        world: &World,
    ) -> Result<()>;

    /// Ensure a single connection
    async fn ensure_connection(&self, params: EnsureConnectionParams) -> Result<()>;

    /// Ensure participant in room
    async fn ensure_participant_in_room(&self, entity_id: UUID, room_id: UUID) -> Result<()>;

    /// Ensure world exists
    async fn ensure_world_exists(&self, world: &World) -> Result<()>;

    /// Ensure room exists
    async fn ensure_room_exists(&self, room: &Room) -> Result<()>;

    // State composition
    /// Compose state from providers
    async fn compose_state(
        &self,
        message: &Memory,
        include_list: Option<Vec<String>>,
        only_include: bool,
        skip_cache: bool,
    ) -> Result<State>;

    // Model usage
    /// Use a model
    async fn use_model(
        &self,
        model_type: ModelType,
        params: GenerateTextParams,
        provider: Option<&str>,
    ) -> Result<String>;

    /// Generate text (convenience method)
    async fn generate_text(
        &self,
        input: &str,
        options: GenerateTextOptions,
    ) -> Result<GenerateTextResult>;

    /// Register model handler
    fn register_model(
        &mut self,
        model_type: ModelType,
        handler: ModelHandler,
        provider: &str,
        priority: i32,
    );

    /// Get model handler
    fn get_model(&self, model_type: ModelType) -> Option<&ModelHandler>;

    // Event system
    /// Register event handler
    fn register_event(&mut self, event: &str, handler: EventHandler);

    /// Get event handlers
    fn get_event(&self, event: &str) -> Option<&[EventHandler]>;

    /// Emit event
    async fn emit_event(&self, event: &str, params: EventPayload) -> Result<()>;

    /// Emit multiple events
    async fn emit_events(&self, events: Vec<String>, params: EventPayload) -> Result<()>;

    // Task management
    /// Register task worker
    fn register_task_worker(&mut self, worker: Arc<dyn TaskWorker>);

    /// Get task worker
    fn get_task_worker(&self, name: &str) -> Option<Arc<dyn TaskWorker>>;

    // Lifecycle
    /// Stop the runtime
    async fn stop(&mut self) -> Result<()>;

    // Memory operations (delegated to database adapter)
    /// Add embedding to memory
    async fn add_embedding_to_memory(&self, memory: &Memory) -> Result<Memory>;

    /// Queue embedding generation
    async fn queue_embedding_generation(
        &self,
        memory: &Memory,
        priority: EmbeddingPriority,
    ) -> Result<()>;

    /// Get all memories
    async fn get_all_memories(&self) -> Result<Vec<Memory>>;

    /// Clear all agent memories
    async fn clear_all_agent_memories(&self) -> Result<()>;

    /// Update memory
    async fn update_memory(&self, memory: &Memory) -> Result<bool>;

    // Database operations (delegated to database adapter)
    /// Get entity by ID
    async fn get_entity_by_id(&self, entity_id: UUID) -> Result<Option<Entity>>;

    /// Get room
    async fn get_room(&self, room_id: UUID) -> Result<Option<Room>>;

    /// Get world
    async fn get_world(&self, world_id: UUID) -> Result<Option<World>>;

    /// Create entity
    async fn create_entity(&self, entity: &Entity) -> Result<bool>;

    /// Update entity
    async fn update_entity(&self, entity: &Entity) -> Result<()>;

    /// Create room
    async fn create_room(&self, room: &Room) -> Result<UUID>;

    /// Add participant
    async fn add_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool>;

    /// Get rooms for world
    async fn get_rooms(&self, world_id: UUID) -> Result<Vec<Room>>;

    /// Create memory
    async fn create_memory(&self, memory: &Memory, table_name: &str) -> Result<UUID>;

    /// Get memories
    async fn get_memories(&self, params: MemoryQuery) -> Result<Vec<Memory>>;

    /// Search memories by embedding
    async fn search_memories_by_embedding(
        &self,
        params: SearchMemoriesParams,
    ) -> Result<Vec<Memory>>;

    /// Log entry
    async fn log(&self, log: &Log) -> Result<()>;

    // Run tracking
    /// Create run ID
    fn create_run_id(&self) -> UUID;

    /// Start run
    fn start_run(&mut self) -> UUID;

    /// End run
    fn end_run(&mut self);

    /// Get current run ID
    fn get_current_run_id(&self) -> Option<UUID>;

    // Messaging
    /// Register send handler for a source
    fn register_send_handler(&mut self, source: &str, handler: SendHandlerFunction);

    /// Send message to target
    async fn send_message_to_target(&self, target: &TargetInfo, content: &Content) -> Result<()>;
}

/// Initialize options
#[derive(Debug, Clone, Default)]
pub struct InitializeOptions {
    /// Skip migrations
    pub skip_migrations: bool,
}

/// Parameters for ensure_connection
#[derive(Debug, Clone)]
pub struct EnsureConnectionParams {
    /// Entity ID
    pub entity_id: UUID,
    /// Room ID
    pub room_id: UUID,
    /// Optional metadata
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    /// User name
    pub user_name: Option<String>,
    /// World name
    pub world_name: Option<String>,
    /// Room name
    pub name: Option<String>,
    /// Source platform
    pub source: Option<String>,
    /// Channel ID
    pub channel_id: Option<String>,
    /// Server ID
    pub server_id: Option<String>,
    /// Channel type
    pub channel_type: Option<ChannelType>,
    /// World ID
    pub world_id: UUID,
    /// User ID
    pub user_id: Option<UUID>,
}

/// Embedding generation priority
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingPriority {
    /// High priority
    High,
    /// Normal priority
    Normal,
    /// Low priority
    Low,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_options() {
        let opts = InitializeOptions::default();
        assert!(!opts.skip_migrations);
    }

    #[test]
    fn test_embedding_priority() {
        let priority = EmbeddingPriority::High;
        assert_eq!(priority, EmbeddingPriority::High);
    }
}
