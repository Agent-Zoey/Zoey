//! Knowledge types

use super::primitives::UUID;
use super::{Content, Memory, MemoryMetadata};
use serde::{Deserialize, Serialize};

/// Represents a single item of knowledge that can be processed and stored by the agent.
/// Knowledge items consist of content (text and optional structured data) and metadata.
/// These items are typically added to the agent's knowledge base via `AgentRuntime::add_knowledge`
/// and retrieved using `AgentRuntime::get_knowledge`.
/// The `id` is a unique identifier for the knowledge item, often derived from its source or content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeItem {
    /// A Universally Unique Identifier for this specific knowledge item.
    pub id: UUID,

    /// The actual content of the knowledge item, which must include text and can have other fields.
    pub content: Content,

    /// Optional metadata associated with this knowledge item, conforming to `MemoryMetadata`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MemoryMetadata>,
}

/// Represents an item within a directory listing, specifically for knowledge loading.
/// When an agent's `Character.knowledge` configuration includes a directory, this type
/// is used to specify the path to that directory and whether its contents should be treated as shared.
/// - `directory`: The path to the directory containing knowledge files.
/// - `shared`: An optional boolean (defaults to false) indicating if the knowledge from this directory is considered shared or private.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryItem {
    /// The path to the directory containing knowledge files.
    pub directory: String,

    /// If true, knowledge from this directory is considered shared; otherwise, it's private. Defaults to false.
    #[serde(default)]
    pub shared: bool,
}

impl KnowledgeItem {
    /// Create a new knowledge item
    pub fn new(id: UUID, content: Content) -> Self {
        Self {
            id,
            content,
            metadata: None,
        }
    }

    /// Create a knowledge item with metadata
    pub fn with_metadata(id: UUID, content: Content, metadata: MemoryMetadata) -> Self {
        Self {
            id,
            content,
            metadata: Some(metadata),
        }
    }

    /// Convert to memory
    pub fn to_memory(&self, agent_id: UUID, room_id: UUID, entity_id: UUID) -> Memory {
        Memory {
            id: self.id,
            agent_id,
            room_id,
            entity_id,
            content: self.content.clone(),
            embedding: None,
            created_at: chrono::Utc::now().timestamp_millis(),
            metadata: self.metadata.clone(),
            unique: Some(false),
            similarity: None,
        }
    }
}

impl DirectoryItem {
    /// Create a new directory item
    pub fn new(directory: String, shared: bool) -> Self {
        Self { directory, shared }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_knowledge_item_creation() {
        let id = Uuid::new_v4();
        let content = Content {
            text: "Test knowledge".to_string(),
            ..Default::default()
        };

        let item = KnowledgeItem::new(id, content.clone());
        assert_eq!(item.id, id);
        assert_eq!(item.content.text, "Test knowledge");
        assert!(item.metadata.is_none());
    }

    #[test]
    fn test_directory_item() {
        let dir = DirectoryItem::new("/path/to/knowledge".to_string(), true);
        assert_eq!(dir.directory, "/path/to/knowledge");
        assert!(dir.shared);
    }
}
