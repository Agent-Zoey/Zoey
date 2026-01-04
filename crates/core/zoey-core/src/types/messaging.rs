//! Messaging types

use super::{Content, UUID};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Target information for sending messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInfo {
    /// Target room ID
    pub room_id: UUID,

    /// Target entity ID (optional, for DMs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<UUID>,

    /// Source platform
    pub source: String,

    /// Additional metadata
    #[serde(flatten)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// Send handler function type
pub type SendHandlerFunction = Arc<
    dyn Fn(
            SendHandlerParams,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::Result<()>> + Send>>
        + Send
        + Sync,
>;

/// Parameters for send handler
#[derive(Clone)]
pub struct SendHandlerParams {
    /// Target information
    pub target: TargetInfo,

    /// Content to send
    pub content: Content,

    /// Runtime reference (type-erased)
    pub runtime: Arc<dyn std::any::Any + Send + Sync>,
}

/// Message service interface for managing message operations
pub trait IMessageService: Send + Sync {
    /// Send a message to a target
    fn send_message(
        &self,
        target: TargetInfo,
        content: Content,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::Result<()>> + Send + '_>>;

    /// Get recent messages for a room
    fn get_recent_messages(
        &self,
        room_id: UUID,
        count: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = crate::Result<Vec<super::Memory>>> + Send + '_>,
    >;
}

/// Mention context for message processing
#[derive(Debug, Clone, Default)]
pub struct MentionContext {
    /// Whether this is a direct mention
    pub is_mention: bool,

    /// Whether this is a reply
    pub is_reply: bool,

    /// Entity IDs mentioned
    pub mentioned_entities: Vec<UUID>,
}

/// Message processing result
#[derive(Debug, Clone)]
pub struct MessageProcessingResult {
    /// Whether processing was successful
    pub success: bool,

    /// Generated responses
    pub responses: Vec<super::Memory>,

    /// Error if any
    pub error: Option<String>,
}

/// Message processing options
#[derive(Debug, Clone, Default)]
pub struct MessageProcessingOptions {
    /// Maximum retries
    pub max_retries: Option<usize>,

    /// Timeout duration in ms
    pub timeout_duration: Option<u64>,

    /// Enable multi-step processing
    pub use_multi_step: Option<bool>,

    /// Maximum multi-step iterations
    pub max_multi_step_iterations: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_target_info() {
        let target = TargetInfo {
            room_id: Uuid::new_v4(),
            entity_id: None,
            source: "test".to_string(),
            metadata: std::collections::HashMap::new(),
        };

        assert_eq!(target.source, "test");
        assert!(target.entity_id.is_none());
    }

    #[test]
    fn test_mention_context() {
        let mut context = MentionContext::default();
        assert!(!context.is_mention);
        assert!(!context.is_reply);

        context.is_mention = true;
        assert!(context.is_mention);
    }
}
