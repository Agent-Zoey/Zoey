use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

pub struct SessionCuesProvider;

#[async_trait]
impl Provider for SessionCuesProvider {
    fn name(&self) -> &str {
        "session_cues"
    }
    fn description(&self) -> Option<String> {
        Some("Provides LAST_PROMPT and basic session cues".to_string())
    }
    fn dynamic(&self) -> bool {
        true
    }

    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        let mut result = ProviderResult::default();
        result.values = Some({
            let mut v = std::collections::HashMap::new();
            v.insert("LAST_PROMPT".to_string(), message.content.text.clone());
            // Provide PREV_PROMPT from runtime settings if present
            if let Some(rt_ref) = zoey_core::runtime_ref::downcast_runtime_ref(&_runtime) {
                if let Some(rt) = rt_ref.try_upgrade() {
                    let prev_key = format!("ui:lastPrompt:{}:prev", message.room_id);
                    if let Some(val) = rt
                        .read()
                        .unwrap()
                        .get_setting(&prev_key)
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                    {
                        v.insert("PREV_PROMPT".to_string(), val);
                    }
                }
            }
            v.insert("ROOM_ID".to_string(), message.room_id.to_string());
            v
        });
        result.data = Some({
            let mut d = std::collections::HashMap::new();
            d.insert(
                "session_cues".to_string(),
                serde_json::json!({
                    "room_id": message.room_id,
                    "message_id": message.id,
                    "entity_id": message.entity_id,
                }),
            );
            d
        });
        Ok(result)
    }
    fn capabilities(&self) -> Option<Vec<String>> {
        Some(vec!["CHAT".to_string()])
    }
}
