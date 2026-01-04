#![allow(missing_docs)]
use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

pub struct RecallProvider;

#[async_trait]
impl Provider for RecallProvider {
    fn name(&self) -> &str {
        "recall"
    }
    fn description(&self) -> Option<String> {
        Some("Detects recall intents and summarizes previous relevant prompts".to_string())
    }
    fn dynamic(&self) -> bool {
        true
    }

    async fn get(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        let mut result = ProviderResult::default();

        let text = message.content.text.to_lowercase();
        let recall_triggers = [
            "what was my last",
            "previous question",
            "earlier you said",
            "before that",
            "what did i ask",
            "what did we discuss",
            "remind me",
            "last time",
        ];

        let is_recall = recall_triggers.iter().any(|kw| text.contains(kw));
        if !is_recall {
            return Ok(result);
        }

        // Access runtime via RuntimeRef
        let rt_ref = zoey_core::downcast_runtime_ref(&runtime)
            .ok_or_else(|| zoey_core::ZoeyError::other("Runtime unavailable"))?;

        // Fetch last and previous prompts via RuntimeRef getters
        let last_key = format!("ui:lastPrompt:{}:last", message.room_id);
        let prev_key = format!("ui:lastPrompt:{}:prev", message.room_id);
        let last = rt_ref
            .get_setting(&last_key)
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let prev = rt_ref
            .get_setting(&prev_key)
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        let summary = match (prev.clone(), last.clone()) {
            (Some(p), Some(l)) => format!("Previous: {}\nLast: {}", p, l),
            (Some(p), None) => format!("Previous: {}", p),
            (None, Some(l)) => format!("Last: {}", l),
            (None, None) => String::new(),
        };

        result.values = Some({
            let mut v = std::collections::HashMap::new();
            v.insert("RECALL_SUMMARY".to_string(), summary);
            v.insert("PREV_PROMPT".to_string(), prev.clone().unwrap_or_default());
            v.insert("LAST_PROMPT".to_string(), last.unwrap_or_default());
            v
        });

        result.data = Some({
            let mut d = std::collections::HashMap::new();
            d.insert(
                "recall".to_string(),
                serde_json::json!({
                    "triggered": true,
                    "room_id": message.room_id,
                }),
            );
            d
        });

        Ok(result)
    }
}
