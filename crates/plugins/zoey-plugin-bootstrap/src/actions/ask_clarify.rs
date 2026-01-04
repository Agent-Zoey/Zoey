use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

pub struct AskClarifyAction;

#[async_trait]
impl Action for AskClarifyAction {
    fn name(&self) -> &str {
        "ASK_CLARIFY"
    }
    fn description(&self) -> &str {
        "Ask a brief clarifying question when ambiguity is detected"
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        let t = message.content.text.to_lowercase();
        let short = t.split_whitespace().count() <= 3;
        let questionish = t.contains('?')
            || t.starts_with("what")
            || t.starts_with("how")
            || t.starts_with("why");
        Ok(short || questionish)
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        _callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        let prompt = format!(
            "Could you clarify what you need regarding: {}?",
            message.content.text
        );
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some(prompt),
            values: None,
            data: None,
            success: true,
            error: None,
        }))
    }
}
