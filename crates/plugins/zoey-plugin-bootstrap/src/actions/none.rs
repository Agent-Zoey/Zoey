//! None action - continue conversation without specific action

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

/// None action
pub struct NoneAction;

#[async_trait]
impl Action for NoneAction {
    fn name(&self) -> &str {
        "NONE"
    }

    fn description(&self) -> &str {
        "Continue the conversation without taking a specific action"
    }

    fn similes(&self) -> Vec<String> {
        vec!["CONTINUE".to_string(), "NOTHING".to_string()]
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
        _message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        _callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some("No specific action taken".to_string()),
            values: None,
            data: None,
            success: true,
            error: None,
        }))
    }
}
