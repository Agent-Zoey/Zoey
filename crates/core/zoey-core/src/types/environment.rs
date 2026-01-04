//! Environment types: Entity, Room, World, etc.

use super::primitives::{Metadata, UUID};
use serde::{Deserialize, Serialize};

/// Channel type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChannelType {
    /// Direct message
    Dm,
    /// Voice direct message
    VoiceDm,
    /// Group direct message
    GroupDm,
    /// Guild/server text channel
    GuildText,
    /// Guild voice channel
    GuildVoice,
    /// Thread channel
    Thread,
    /// Feed/timeline channel
    Feed,
    /// Self-channel (internal)
    #[serde(rename = "SELF")]
    SelfChannel,
    /// API channel
    Api,
    /// World (server-level)
    World,
    /// Unknown channel type
    Unknown,
}

/// Entity (user, bot, character) in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    /// Unique identifier
    pub id: UUID,

    /// Agent ID this entity belongs to
    pub agent_id: UUID,

    /// Display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional username
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// Optional email
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Optional avatar URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,

    /// Metadata (components, status, etc.)
    #[serde(default)]
    pub metadata: Metadata,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

/// Room (channel, conversation space)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    /// Unique identifier
    pub id: UUID,

    /// Agent ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<UUID>,

    /// Room name
    pub name: String,

    /// Source platform (e.g., "discord", "twitter")
    pub source: String,

    /// Channel type
    #[serde(rename = "type")]
    pub channel_type: ChannelType,

    /// Platform-specific channel ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,

    /// Platform-specific server ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,

    /// World ID this room belongs to
    pub world_id: UUID,

    /// Metadata
    #[serde(default)]
    pub metadata: Metadata,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

/// World (server, workspace, top-level container)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    /// Unique identifier
    pub id: UUID,

    /// World name
    pub name: String,

    /// Agent ID
    pub agent_id: UUID,

    /// Platform-specific server ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,

    /// Metadata (ownership, roles, settings, etc.)
    #[serde(default)]
    pub metadata: Metadata,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

/// Participant in a room
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    /// Entity ID
    pub entity_id: UUID,

    /// Room ID
    pub room_id: UUID,

    /// Join timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<i64>,

    /// Metadata
    #[serde(default)]
    pub metadata: Metadata,
}

/// Relationship between entities
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    /// Source entity ID
    pub entity_id_a: UUID,

    /// Target entity ID
    pub entity_id_b: UUID,

    /// Relationship type (e.g., "friend", "follow", "block")
    #[serde(rename = "type")]
    pub relationship_type: String,

    /// Agent ID
    pub agent_id: UUID,

    /// Metadata
    #[serde(default)]
    pub metadata: Metadata,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

/// Component attached to an entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Component ID
    pub id: UUID,

    /// Entity this component is attached to
    pub entity_id: UUID,

    /// World ID
    pub world_id: UUID,

    /// Optional source entity (for perspective-specific components)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_entity_id: Option<UUID>,

    /// Component type (e.g., "inventory", "stats", "appearance")
    #[serde(rename = "type")]
    pub component_type: String,

    /// Component data
    pub data: serde_json::Value,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,

    /// Update timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_channel_type_serialization() {
        let ct = ChannelType::Dm;
        let json = serde_json::to_string(&ct).unwrap();
        assert_eq!(json, "\"DM\"");
    }

    #[test]
    fn test_entity_creation() {
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

        assert_eq!(entity.name, Some("Test User".to_string()));
    }
}
