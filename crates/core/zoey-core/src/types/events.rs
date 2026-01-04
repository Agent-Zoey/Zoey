//! Event types for the event system

use super::{Content, Memory, Room, World, UUID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Event type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    /// Message received
    MessageReceived,
    /// Message sent
    MessageSent,
    /// Reaction received
    ReactionReceived,
    /// Post generated
    PostGenerated,
    /// World joined
    WorldJoined,
    /// World connected
    WorldConnected,
    /// Entity joined
    EntityJoined,
    /// Entity left
    EntityLeft,
    /// Action started
    ActionStarted,
    /// Action completed
    ActionCompleted,
    /// Evaluator started
    EvaluatorStarted,
    /// Evaluator completed
    EvaluatorCompleted,
    /// Run started
    RunStarted,
    /// Run ended
    RunEnded,
    /// Run timeout
    RunTimeout,
    /// Control message
    ControlMessage,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Message event payload
#[derive(Debug, Clone)]
pub struct MessagePayload {
    /// Runtime reference (type-erased)
    pub runtime: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    /// The message
    pub message: Memory,
}

/// Invoke event payload
#[derive(Debug, Clone)]
pub struct InvokePayload {
    /// Runtime reference (type-erased)
    pub runtime: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    /// Callback function
    pub callback: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
    /// World ID
    pub world_id: UUID,
    /// User/entity ID
    pub user_id: UUID,
    /// Room ID
    pub room_id: UUID,
    /// Source platform
    pub source: String,
}

/// World event payload
#[derive(Clone)]
pub struct WorldPayload {
    /// Runtime reference (type-erased)
    pub runtime: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    /// World information
    pub world: World,
    /// Rooms in the world
    pub rooms: Vec<Room>,
    /// Entities in the world
    pub entities: Vec<super::Entity>,
    /// Source platform
    pub source: String,
    /// Completion callback
    pub on_complete: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}

impl std::fmt::Debug for WorldPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldPayload")
            .field("runtime", &"<runtime>")
            .field("world", &self.world)
            .field("rooms", &self.rooms)
            .field("entities", &self.entities)
            .field("source", &self.source)
            .field(
                "on_complete",
                &if self.on_complete.is_some() {
                    "<function>"
                } else {
                    "None"
                },
            )
            .finish()
    }
}

/// Entity event payload
#[derive(Debug, Clone)]
pub struct EntityPayload {
    /// Runtime reference (type-erased)
    pub runtime: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    /// Entity ID
    pub entity_id: UUID,
    /// World ID
    pub world_id: UUID,
    /// Room ID
    pub room_id: UUID,
    /// Source platform
    pub source: String,
    /// Metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Action event payload
#[derive(Debug, Clone)]
pub struct ActionEventPayload {
    /// Runtime reference (type-erased)
    pub runtime: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    /// Room ID
    pub room_id: UUID,
    /// World information
    pub world: Option<World>,
    /// Content
    pub content: Option<Content>,
    /// Message ID
    pub message_id: Option<UUID>,
}

/// Evaluator event payload
#[derive(Debug, Clone)]
pub struct EvaluatorEventPayload {
    /// Runtime reference (type-erased)
    pub runtime: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    /// Evaluator name
    pub evaluator_name: String,
    /// Evaluator ID
    pub evaluator_id: UUID,
    /// Error if failed
    pub error: Option<String>,
}

/// Run event payload
#[derive(Debug, Clone)]
pub struct RunEventPayload {
    /// Runtime reference (type-erased)
    pub runtime: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    /// Run ID
    pub run_id: UUID,
    /// Status
    pub status: String,
    /// Message ID
    pub message_id: Option<UUID>,
    /// Room ID
    pub room_id: Option<UUID>,
    /// Entity ID
    pub entity_id: Option<UUID>,
    /// Start time
    pub start_time: Option<i64>,
    /// End time
    pub end_time: Option<i64>,
    /// Duration in milliseconds
    pub duration: Option<i64>,
    /// Error if failed
    pub error: Option<String>,
    /// Source
    pub source: Option<String>,
}

/// Generic event payload type
#[derive(Debug, Clone)]
pub enum EventPayload {
    /// Message event
    Message(MessagePayload),
    /// Invoke event
    Invoke(InvokePayload),
    /// World event
    World(WorldPayload),
    /// Entity event
    Entity(EntityPayload),
    /// Action event
    Action(ActionEventPayload),
    /// Evaluator event
    Evaluator(EvaluatorEventPayload),
    /// Run event
    Run(RunEventPayload),
    /// Generic payload
    Generic(HashMap<String, serde_json::Value>),
}

/// Event handler function type
pub type EventHandler = std::sync::Arc<
    dyn Fn(EventPayload) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_display() {
        assert_eq!(EventType::MessageReceived.to_string(), "MessageReceived");
    }
}
