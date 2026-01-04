//! Ignore action - skip responding to a message

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

/// Ignore action
pub struct IgnoreAction;

#[async_trait]
impl Action for IgnoreAction {
    fn name(&self) -> &str {
        "IGNORE"
    }

    fn description(&self) -> &str {
        "Do not respond to this message"
    }

    fn similes(&self) -> Vec<String> {
        vec![
            "SKIP".to_string(),
            "PASS".to_string(),
            "NO_RESPONSE".to_string(),
        ]
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        // Ignore is always valid
        Ok(true)
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        _callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        // Simply return success without generating response
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some("Ignored".to_string()),
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
    async fn test_ignore_action() {
        let action = IgnoreAction;
        assert_eq!(action.name(), "IGNORE");

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
        assert_eq!(r.text, Some("Ignored".to_string()));
    }
}
