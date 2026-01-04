//! Database adapter types

use super::{
    Agent, Component, Entity, Memory, MemoryQuery, Participant, Relationship, Room,
    SearchMemoriesParams, Task, World, UUID,
};
use crate::observability::types::LLMCostRecord;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    /// Optional unique identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<UUID>,

    /// Associated entity ID
    pub entity_id: UUID,

    /// Associated room ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<UUID>,

    /// Log body
    pub body: serde_json::Value,

    /// Log type
    #[serde(rename = "type")]
    pub log_type: String,

    /// Creation timestamp
    pub created_at: i64,
}

/// Run status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    /// Run started
    Started,
    /// Run completed successfully
    Completed,
    /// Run timed out
    Timeout,
    /// Run encountered an error
    Error,
}

/// Agent run counts
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunCounts {
    /// Number of actions executed
    pub actions: usize,
    /// Number of model calls made
    pub model_calls: usize,
    /// Number of errors
    pub errors: usize,
    /// Number of evaluators run
    pub evaluators: usize,
}

/// Agent run summary
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunSummary {
    /// Run ID
    pub run_id: String,
    /// Status
    pub status: RunStatus,
    /// Start timestamp
    pub started_at: Option<i64>,
    /// End timestamp
    pub ended_at: Option<i64>,
    /// Duration in milliseconds
    pub duration_ms: Option<i64>,
    /// Message ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<UUID>,
    /// Room ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<UUID>,
    /// Entity ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<UUID>,
    /// Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Counts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<AgentRunCounts>,
}

/// Agent run summary result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunSummaryResult {
    /// Run summaries
    pub runs: Vec<AgentRunSummary>,
    /// Total count
    pub total: usize,
    /// Whether there are more results
    pub has_more: bool,
}

/// Database adapter trait
#[async_trait]
pub trait IDatabaseAdapter: Send + Sync {
    /// Get database instance (type-erased)
    fn db(&self) -> &dyn std::any::Any;

    /// Initialize database connection
    async fn initialize(&mut self, config: Option<serde_json::Value>) -> Result<()>;

    /// Initialize database (alias)
    async fn init(&mut self) -> Result<()> {
        self.initialize(None).await
    }

    /// Run plugin schema migrations
    async fn run_plugin_migrations(
        &self,
        _plugins: Vec<PluginMigration>,
        _options: MigrationOptions,
    ) -> Result<()> {
        Ok(())
    }

    /// Check if database is ready
    async fn is_ready(&self) -> Result<bool>;

    /// Close database connection
    async fn close(&mut self) -> Result<()>;

    /// Get database connection (type-erased)
    async fn get_connection(&self) -> Result<Box<dyn std::any::Any + Send>>;

    /// Persist an LLM cost record
    async fn persist_llm_cost(&self, _record: LLMCostRecord) -> Result<()> {
        Ok(())
    }

    // Agent operations
    /// Get agent by ID
    async fn get_agent(&self, agent_id: UUID) -> Result<Option<Agent>>;

    /// Get all agents
    async fn get_agents(&self) -> Result<Vec<Agent>>;

    /// Create agent
    async fn create_agent(&self, agent: &Agent) -> Result<bool>;

    /// Update agent
    async fn update_agent(&self, agent_id: UUID, agent: &Agent) -> Result<bool>;

    /// Delete agent
    async fn delete_agent(&self, agent_id: UUID) -> Result<bool>;

    // Embedding operations
    /// Ensure embedding dimension
    async fn ensure_embedding_dimension(&self, dimension: usize) -> Result<()>;

    // Entity operations
    /// Get entities by IDs
    async fn get_entities_by_ids(&self, entity_ids: Vec<UUID>) -> Result<Vec<Entity>>;

    /// Get entities for room
    async fn get_entities_for_room(
        &self,
        room_id: UUID,
        include_components: bool,
    ) -> Result<Vec<Entity>>;

    /// Create entities
    async fn create_entities(&self, entities: Vec<Entity>) -> Result<bool>;

    /// Update entity
    async fn update_entity(&self, entity: &Entity) -> Result<()>;

    /// Get entity by ID
    async fn get_entity_by_id(&self, entity_id: UUID) -> Result<Option<Entity>>;

    // Component operations
    /// Get component
    async fn get_component(
        &self,
        entity_id: UUID,
        component_type: &str,
        world_id: Option<UUID>,
        source_entity_id: Option<UUID>,
    ) -> Result<Option<Component>>;

    /// Get all components for an entity
    async fn get_components(
        &self,
        entity_id: UUID,
        world_id: Option<UUID>,
        source_entity_id: Option<UUID>,
    ) -> Result<Vec<Component>>;

    /// Create component
    async fn create_component(&self, component: &Component) -> Result<bool>;

    /// Update component
    async fn update_component(&self, component: &Component) -> Result<()>;

    /// Delete component
    async fn delete_component(&self, component_id: UUID) -> Result<()>;

    // Memory operations
    /// Get memories matching criteria
    async fn get_memories(&self, params: MemoryQuery) -> Result<Vec<Memory>>;

    /// Create memory
    async fn create_memory(&self, memory: &Memory, table_name: &str) -> Result<UUID>;

    /// Search memories by embedding
    async fn search_memories_by_embedding(
        &self,
        params: SearchMemoriesParams,
    ) -> Result<Vec<Memory>>;

    /// Get cached embeddings
    async fn get_cached_embeddings(&self, params: MemoryQuery) -> Result<Vec<Memory>>;

    /// Update memory
    async fn update_memory(&self, memory: &Memory) -> Result<bool>;

    /// Remove memory
    async fn remove_memory(&self, memory_id: UUID, table_name: &str) -> Result<bool>;

    /// Remove all memories for agent
    async fn remove_all_memories(&self, agent_id: UUID, table_name: &str) -> Result<bool>;

    /// Count memories
    async fn count_memories(&self, params: MemoryQuery) -> Result<usize>;

    // World/Room operations
    /// Get world
    async fn get_world(&self, world_id: UUID) -> Result<Option<World>>;

    /// Ensure world exists
    async fn ensure_world(&self, world: &World) -> Result<()>;

    /// Get room
    async fn get_room(&self, room_id: UUID) -> Result<Option<Room>>;

    /// Create room
    async fn create_room(&self, room: &Room) -> Result<UUID>;

    /// Get rooms for world
    async fn get_rooms(&self, world_id: UUID) -> Result<Vec<Room>>;

    /// Get rooms for agent
    async fn get_rooms_for_agent(&self, agent_id: UUID) -> Result<Vec<Room>>;

    // Participant operations
    /// Add participant to room
    async fn add_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool>;

    /// Remove participant from room
    async fn remove_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool>;

    /// Get participants for room
    async fn get_participants(&self, room_id: UUID) -> Result<Vec<Participant>>;

    // Relationship operations
    /// Create relationship
    async fn create_relationship(&self, relationship: &Relationship) -> Result<bool>;

    /// Get relationship
    async fn get_relationship(
        &self,
        entity_id_a: UUID,
        entity_id_b: UUID,
    ) -> Result<Option<Relationship>>;

    // Task operations
    /// Create task
    async fn create_task(&self, task: &Task) -> Result<UUID>;

    /// Update task
    async fn update_task(&self, task: &Task) -> Result<bool>;

    /// Get task
    async fn get_task(&self, task_id: UUID) -> Result<Option<Task>>;

    /// Get pending tasks
    async fn get_pending_tasks(&self, agent_id: UUID) -> Result<Vec<Task>>;

    // Logging operations
    /// Log entry
    async fn log(&self, log: &Log) -> Result<()>;

    /// Get logs
    async fn get_logs(&self, params: LogQuery) -> Result<Vec<Log>>;

    // Run tracking
    /// Get agent run summaries
    async fn get_agent_run_summaries(
        &self,
        params: RunSummaryQuery,
    ) -> Result<AgentRunSummaryResult>;
}

/// Plugin migration info
#[derive(Debug, Clone)]
pub struct PluginMigration {
    /// Plugin name
    pub name: String,
    /// Optional schema
    pub schema: Option<serde_json::Value>,
}

/// Migration options
#[derive(Debug, Clone, Default)]
pub struct MigrationOptions {
    /// Verbose output
    pub verbose: bool,
    /// Force migration
    pub force: bool,
    /// Dry run (don't apply changes)
    pub dry_run: bool,
}

/// Log query parameters
#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    /// Filter by entity ID
    pub entity_id: Option<UUID>,
    /// Filter by room ID
    pub room_id: Option<UUID>,
    /// Filter by log type
    pub log_type: Option<String>,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
}

/// Run summary query parameters
#[derive(Debug, Clone, Default)]
pub struct RunSummaryQuery {
    /// Filter by agent ID
    pub agent_id: Option<UUID>,
    /// Filter by status
    pub status: Option<RunStatus>,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_status_serialization() {
        let status = RunStatus::Completed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"completed\"");
    }

    #[test]
    fn test_agent_run_counts() {
        let counts = AgentRunCounts {
            actions: 5,
            model_calls: 10,
            errors: 0,
            evaluators: 2,
        };

        assert_eq!(counts.actions, 5);
        assert_eq!(counts.model_calls, 10);
    }
}
