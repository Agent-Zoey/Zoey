//! Reply action - respond to a message

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

/// Reply action
pub struct ReplyAction;

#[async_trait]
impl Action for ReplyAction {
    fn name(&self) -> &str {
        "REPLY"
    }

    fn description(&self) -> &str {
        "Respond to the current message with generated text"
    }

    fn similes(&self) -> Vec<String> {
        vec![
            "RESPOND".to_string(),
            "ANSWER".to_string(),
            "REPLY_TO".to_string(),
        ]
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        // Reply is almost always valid
        Ok(true)
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        // In real implementation, would generate response using LLM
        let response_text = format!("Response to: {}", message.content.text);

        // Call callback if provided
        if let Some(cb) = callback {
            let response_content = Content {
                text: response_text.clone(),
                source: message.content.source.clone(),
                ..Default::default()
            };

            cb(response_content).await?;
        }

        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some(response_text),
            values: None,
            data: None,
            success: true,
            error: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reply_action() {
        let action = ReplyAction;
        assert_eq!(action.name(), "REPLY");
        assert_eq!(
            action.description(),
            "Respond to the current message with generated text"
        );

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
