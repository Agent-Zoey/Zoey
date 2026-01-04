//! Primitive types used throughout ZoeyOS

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// UUID type (re-export for convenience)
pub type UUID = Uuid;

/// Generic metadata type
pub type Metadata = HashMap<String, serde_json::Value>;

/// Role enum for permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Role {
    /// No role
    None,
    /// Guest role
    Guest,
    /// Member role
    Member,
    /// Moderator role
    Moderator,
    /// Admin role
    Admin,
    /// Owner role
    Owner,
}

/// Content type for messages and media
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ContentType {
    /// Text content
    Text,
    /// Image content
    Image,
    /// Audio content
    Audio,
    /// Video content
    Video,
    /// Document content
    Document,
    /// Unknown/other content
    Unknown,
}

/// Media attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    /// URL or path to media
    pub url: String,
    /// Content type
    pub content_type: ContentType,
    /// Optional title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional text content (for documents)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Content of a message or interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    /// Main text content
    pub text: String,

    /// Source of the content (e.g., "discord", "twitter", "client_chat")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Channel type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_type: Option<String>,

    /// Optional thought process
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<String>,

    /// Actions to be executed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<String>>,

    /// Providers used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<String>>,

    /// Attachments (images, documents, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<Media>>,

    /// Whether this is a simple response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simple: Option<bool>,

    /// Additional metadata
    #[serde(flatten)]
    pub metadata: Metadata,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            text: String::new(),
            source: None,
            channel_type: None,
            thought: None,
            actions: None,
            providers: None,
            attachments: None,
            simple: None,
            metadata: HashMap::new(),
        }
    }
}

/// Control message for UI/frontend interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlMessage {
    /// Room ID this control message applies to
    pub room_id: UUID,

    /// Payload containing action details
    pub payload: ControlPayload,
}

/// Control message payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlPayload {
    /// Action to perform (e.g., "enable", "disable")
    pub action: String,

    /// Target UI element or feature
    pub target: String,

    /// Additional data
    #[serde(flatten)]
    pub data: Metadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_default() {
        let content = Content::default();
        assert_eq!(content.text, "");
        assert!(content.source.is_none());
    }

    #[test]
    fn test_role_serialization() {
        let role = Role::Admin;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"ADMIN\"");
    }

    #[test]
    fn test_content_type_serialization() {
        let ct = ContentType::Image;
        let json = serde_json::to_string(&ct).unwrap();
        assert_eq!(json, "\"IMAGE\"");
    }
}
