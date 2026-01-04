//! Memory types for agent memory system

use super::primitives::{Content, Metadata, UUID};
use serde::{Deserialize, Serialize};

/// Memory metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMetadata {
    /// Type of memory (e.g., "message", "fact", "goal")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<String>,

    /// Entity name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_name: Option<String>,

    /// Additional metadata
    #[serde(flatten)]
    pub data: Metadata,
}

/// Memory entry in the agent's memory system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    /// Unique identifier
    pub id: UUID,

    /// Entity ID that created/owns this memory
    pub entity_id: UUID,

    /// Agent ID
    pub agent_id: UUID,

    /// Room ID where this memory was created
    pub room_id: UUID,

    /// Memory content
    pub content: Content,

    /// Embedding vector (for semantic search)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,

    /// Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MemoryMetadata>,

    /// Creation timestamp
    pub created_at: i64,

    /// Whether this memory is unique
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique: Option<bool>,

    /// Similarity score (when returned from search)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity: Option<f32>,
}

/// Parameters for querying memories
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    /// Filter by entity ID
    pub entity_id: Option<UUID>,

    /// Filter by agent ID
    pub agent_id: Option<UUID>,

    /// Filter by room ID
    pub room_id: Option<UUID>,

    /// Filter by world ID
    pub world_id: Option<UUID>,

    /// Maximum number of results
    pub count: Option<usize>,

    /// Offset for pagination
    pub offset: Option<usize>,

    /// Only return unique memories
    pub unique: Option<bool>,

    /// Table name to query from
    pub table_name: String,

    /// Start timestamp
    pub start: Option<i64>,

    /// End timestamp
    pub end: Option<i64>,
}

/// Parameters for searching memories by embedding
#[derive(Debug, Clone)]
pub struct SearchMemoriesParams {
    /// Table name to search in
    pub table_name: String,

    /// Filter by agent ID
    pub agent_id: Option<UUID>,

    /// Filter by room ID
    pub room_id: Option<UUID>,

    /// Filter by world ID
    pub world_id: Option<UUID>,

    /// Filter by entity ID
    pub entity_id: Option<UUID>,

    /// Embedding vector to search for
    pub embedding: Vec<f32>,

    /// Maximum number of results
    pub count: usize,

    /// Only return unique memories
    pub unique: Option<bool>,

    /// Minimum similarity threshold
    pub threshold: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_memory_creation() {
        let memory = Memory {
            id: Uuid::new_v4(),
            entity_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: Content {
                text: "Test memory".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: 12345,
            unique: Some(false),
            similarity: None,
        };

        assert_eq!(memory.content.text, "Test memory");
    }

    #[test]
    fn test_memory_query_default() {
        let query = MemoryQuery {
            table_name: "memories".to_string(),
            ..Default::default()
        };

        assert_eq!(query.table_name, "memories");
        assert!(query.entity_id.is_none());
    }
}
