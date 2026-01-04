//! Send message action - send a message to a specific target

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

/// Send message action
pub struct SendMessageAction;

#[async_trait]
impl Action for SendMessageAction {
    fn name(&self) -> &str {
        "SEND_MESSAGE"
    }

    fn description(&self) -> &str {
        "Send a message to a specific room or entity"
    }

    fn similes(&self) -> Vec<String> {
        vec![
            "MESSAGE".to_string(),
            "SEND".to_string(),
            "POST".to_string(),
        ]
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        // Validate that we have a target (room_id in message)
        Ok(!message.room_id.is_nil())
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        // Extract target information from options or state
        let target = TargetInfo {
            room_id: message.room_id,
            entity_id: Some(message.entity_id),
            source: message
                .content
                .source
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            metadata: std::collections::HashMap::new(),
        };

        // Create message content
        let content = Content {
            text: format!("Sending message to room {}", message.room_id),
            source: message.content.source.clone(),
            ..Default::default()
        };

        // Call callback if provided
        if let Some(cb) = callback {
            cb(content.clone()).await?;
        }

        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some(format!("Message sent to {}", target.room_id)),
            values: None,
            data: Some({
                let mut data = std::collections::HashMap::new();
                data.insert("target_room".to_string(), serde_json::json!(target.room_id));
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
    async fn test_send_message_action() {
        let action = SendMessageAction;
        assert_eq!(action.name(), "SEND_MESSAGE");

        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "Hello!".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        let state = State::new();
        let valid = action
            .validate(Arc::new(()), &message, &state)
            .await
            .unwrap();
        assert!(valid);

        let result = action
            .handler(Arc::new(()), &message, &state, None, None)
            .await
            .unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().success);
    }
}
