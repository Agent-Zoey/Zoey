//! Follow/unfollow room actions

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

/// Follow room action
pub struct FollowRoomAction;

#[async_trait]
impl Action for FollowRoomAction {
    fn name(&self) -> &str {
        "FOLLOW_ROOM"
    }

    fn description(&self) -> &str {
        "Follow a room to receive updates"
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        _callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        // In real implementation, would update database
        tracing::info!("Following room: {}", message.room_id);

        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some(format!("Now following room {}", message.room_id)),
            values: None,
            data: Some({
                let mut data = std::collections::HashMap::new();
                data.insert("room_id".to_string(), serde_json::json!(message.room_id));
                data.insert("following".to_string(), serde_json::json!(true));
                data
            }),
            success: true,
            error: None,
        }))
    }
}

/// Unfollow room action
pub struct UnfollowRoomAction;

#[async_trait]
impl Action for UnfollowRoomAction {
    fn name(&self) -> &str {
        "UNFOLLOW_ROOM"
    }

    fn description(&self) -> &str {
        "Unfollow a room to stop receiving updates"
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        _callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        // In real implementation, would update database
        tracing::info!("Unfollowing room: {}", message.room_id);

        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some(format!("Stopped following room {}", message.room_id)),
            values: None,
            data: Some({
                let mut data = std::collections::HashMap::new();
                data.insert("room_id".to_string(), serde_json::json!(message.room_id));
                data.insert("following".to_string(), serde_json::json!(false));
                data
            }),
            success: true,
            error: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_follow_room_action() {
        let action = FollowRoomAction;
        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content::default(),
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        let state = State::new();
        let result = action
            .handler(Arc::new(()), &message, &state, None, None)
            .await
            .unwrap();

        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.success);
        assert!(r.data.is_some());
    }

    #[tokio::test]
    async fn test_unfollow_room_action() {
        let action = UnfollowRoomAction;
        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content::default(),
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        let state = State::new();
        let result = action
            .handler(Arc::new(()), &message, &state, None, None)
            .await
            .unwrap();

        assert!(result.is_some());
        assert!(result.unwrap().success);
    }
}
