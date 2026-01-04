//! Entities provider - supplies information about entities in the conversation

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;
use std::sync::Once;

struct SettingRow {
    name: String,
    value: String,
    source: String,
    change: String,
}
fn pad(s: &str, w: usize) -> String {
    let mut out = s.to_string();
    if out.len() > w {
        out.truncate(w);
    }
    let pad_len = if w > out.len() { w - out.len() } else { 0 };
    out + &" ".repeat(pad_len)
}
fn render(provider: &str, color: &str, deco: &str, rows: Vec<SettingRow>) {
    let reset = "\x1b[0m";
    let top = format!("{}+{}+{}", color, "-".repeat(78), reset);
    let title = format!(" {} ", provider.to_uppercase());
    let d1 = pad(&format!("{}{}{}", deco, deco, deco), 25);
    let d2 = pad(&format!("{}{}{}", deco, deco, deco), 25);
    let line1 = format!("{}|{}|{}", color, pad(&(d1 + &title + &d2), 78), reset);
    let line2 = format!(
        "{}|{}|{}",
        color,
        pad(&format!("{} Provider • Entities • IDs {}", deco, deco), 78),
        reset
    );
    let sep = format!("{}+{}+{}", color, "=".repeat(78), reset);
    let header = format!(
        "{}|{}|{}|{}|{}|{}",
        color,
        pad("Setting", 24),
        pad("Value", 20),
        pad("Source", 10),
        pad("Change", 24),
        reset
    );
    let mid = format!("{}+{}+{}", color, "-".repeat(78), reset);
    tracing::info!("{}", top);
    tracing::info!("{}", line1);
    tracing::info!("{}", line2);
    tracing::info!("{}", sep);
    tracing::info!("{}", header);
    tracing::info!("{}", mid);
    if rows.is_empty() {
        let row = format!(
            "{}|{}|{}|{}|{}|{}",
            color,
            pad("<none>", 24),
            pad("-", 20),
            pad("-", 10),
            pad("Use code defaults", 24),
            reset
        );
        tracing::info!("{}", row);
    } else {
        for r in rows {
            let row = format!(
                "{}|{}|{}|{}|{}|{}",
                color,
                pad(&r.name, 24),
                pad(&r.value, 20),
                pad(&r.source, 10),
                pad(&r.change, 24),
                reset
            );
            tracing::info!("{}", row);
        }
    }
    let bottom = format!("{}+{}+{}", color, "-".repeat(78), reset);
    tracing::info!("{}", bottom);
}
static INIT: Once = Once::new();

/// Entities provider
pub struct EntitiesProvider;

#[async_trait]
impl Provider for EntitiesProvider {
    fn name(&self) -> &str {
        "entities"
    }

    fn description(&self) -> Option<String> {
        Some("Provides information about entities in the current context".to_string())
    }

    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        INIT.call_once(|| {
            let rows = vec![SettingRow {
                name: "ENTITIES_DYNAMIC".to_string(),
                value: "false".to_string(),
                source: "code".to_string(),
                change: "N/A".to_string(),
            }];
            render("entities", "\x1b[32m", "%", rows);
        });
        // In real implementation, would fetch entities from database
        let mut result = ProviderResult::default();

        result.text = Some(format!(
            "Entities in conversation:\n\
            - Entity ID: {}\n\
            - Room ID: {}\n\
            - Additional entities: [Would be fetched from database]",
            message.entity_id, message.room_id
        ));

        // Set template values
        result.values = Some({
            let mut values = std::collections::HashMap::new();
            values.insert("ENTITY_NAME".to_string(), "User".to_string()); // Would fetch from DB
            values.insert("senderId".to_string(), message.entity_id.to_string());
            values.insert("senderName".to_string(), "User".to_string());
            values.insert("MESSAGE_TEXT".to_string(), message.content.text.clone());
            values.insert("messageText".to_string(), message.content.text.clone());
            values
        });

        result.data = Some({
            let mut data = std::collections::HashMap::new();
            data.insert(
                "entity_id".to_string(),
                serde_json::json!(message.entity_id),
            );
            data.insert("room_id".to_string(), serde_json::json!(message.room_id));
            data
        });

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_entities_provider() {
        let provider = EntitiesProvider;
        assert_eq!(provider.name(), "entities");

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
        let result = provider.get(Arc::new(()), &message, &state).await.unwrap();

        assert!(result.text.is_some());
        assert!(result.data.is_some());
    }
}
