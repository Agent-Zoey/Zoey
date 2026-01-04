//! Recent messages provider - supplies recent conversation history

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
        pad(&format!("{} Provider • History • Count {}", deco, deco), 78),
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

/// Recent messages provider
pub struct RecentMessagesProvider {
    /// Number of recent messages to retrieve
    count: usize,
}

impl RecentMessagesProvider {
    /// Create a new recent messages provider
    pub fn new(count: usize) -> Self {
        Self { count }
    }
}

impl Default for RecentMessagesProvider {
    fn default() -> Self {
        Self::new(10)
    }
}

#[async_trait]
impl Provider for RecentMessagesProvider {
    fn name(&self) -> &str {
        "recent_messages"
    }

    fn description(&self) -> Option<String> {
        Some(format!(
            "Provides the {} most recent messages in the conversation",
            self.count
        ))
    }

    fn dynamic(&self) -> bool {
        true // Always fetch fresh messages
    }

    async fn get(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        INIT.call_once(|| {
            let rows = vec![SettingRow {
                name: "RECENT_MESSAGES_COUNT".to_string(),
                value: self.count.to_string(),
                source: "default".to_string(),
                change: "set via RecentMessagesProvider::new(N)".to_string(),
            }];
            render("recent_messages", "\x1b[34m", "=", rows);
        });
        // In real implementation, would fetch from database
        let mut result = ProviderResult::default();

        // Try to obtain last and previous prompts from runtime settings
        let mut recent_lines: Vec<String> = Vec::new();
        if let Some(rt_ref) = zoey_core::runtime_ref::downcast_runtime_ref(&runtime) {
            if let Some(rt) = rt_ref.try_upgrade() {
                let last_key = format!("ui:lastPrompt:{}:last", message.room_id);
                let prev_key = format!("ui:lastPrompt:{}:prev", message.room_id);
                let r = rt.read().unwrap();
                let last = r
                    .get_setting(&last_key)
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                let prev = r
                    .get_setting(&prev_key)
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                if let Some(p) = prev {
                    recent_lines.push(format!("Previous: {}", p));
                }
                if let Some(l) = last {
                    recent_lines.push(format!("Last: {}", l));
                }
            }
        }
        recent_lines.push(format!("Current: {}", message.content.text));
        let recent_msg_text = format!(
            "Recent Messages (room {}):\n{}",
            message.room_id,
            recent_lines.join("\n")
        );

        result.text = Some(recent_msg_text.clone());

        // Set RECENT_MESSAGES for template
        // Compact only when UI_COMPACT_CONTEXT flag indicates high context pressure
        let compact_mode = _state
            .get_value("UI_COMPACT_CONTEXT")
            .map(|v| v == "true")
            .unwrap_or(false);
        let compacted = if compact_mode {
            recent_lines
                .iter()
                .rev()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            recent_lines.join("\n")
        };
        result.values = Some({
            let mut values = std::collections::HashMap::new();
            values.insert("RECENT_MESSAGES".to_string(), compacted);
            values.insert(
                "recentMessages".to_string(),
                "No previous messages".to_string(),
            );
            values
        });

        result.data = Some({
            let mut data = std::collections::HashMap::new();
            data.insert("room_id".to_string(), serde_json::json!(message.room_id));
            data.insert("count".to_string(), serde_json::json!(self.count));
            data.insert(
                "recent_messages".to_string(),
                serde_json::json!(recent_lines),
            );
            data
        });

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_recent_messages_provider() {
        let provider = RecentMessagesProvider::new(5);
        assert_eq!(provider.name(), "recent_messages");
        assert!(provider.dynamic());

        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "Test message".to_string(),
                ..Default::default()
            },
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
    // removed misplaced free function duplicate
}
