use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

pub struct SummarizeAndConfirmAction;

#[async_trait]
impl Action for SummarizeAndConfirmAction {
    fn name(&self) -> &str {
        "SUMMARIZE_AND_CONFIRM"
    }
    fn description(&self) -> &str {
        "Offer a one-sentence recap and ask for confirmation"
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(message.content.text.len() > 120)
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        _callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        let text = &message.content.text;
        let summary = if text.len() > 160 { &text[..160] } else { text };
        let prompt = format!("So, to confirm: {} — is that correct?", summary);
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
