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
        pad(
            &format!("{} Provider • Context • Summary {}", deco, deco),
            78
        ),
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

pub struct ContextSummaryProvider {
    pub message_count: usize,
    pub thought_count: usize,
}

impl Default for ContextSummaryProvider {
    fn default() -> Self {
        Self {
            message_count: 5,
            thought_count: 5,
        }
    }
}

#[async_trait]
impl Provider for ContextSummaryProvider {
    fn name(&self) -> &str {
        "context_summary"
    }
    fn description(&self) -> Option<String> {
        Some("Summarizes recent room thoughts and messages".to_string())
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
        INIT.call_once(|| {
            let rows = vec![
                SettingRow {
                    name: "CONTEXT_SUMMARY_MESSAGES".to_string(),
                    value: self.message_count.to_string(),
                    source: "default".to_string(),
                    change: "set via ContextSummaryProvider::default()".to_string(),
                },
                SettingRow {
                    name: "CONTEXT_SUMMARY_THOUGHTS".to_string(),
                    value: self.thought_count.to_string(),
                    source: "default".to_string(),
                    change: "set via ContextSummaryProvider::default()".to_string(),
                },
            ];
            render("context_summary", "\x1b[36m", "~", rows);
        });
        let mut result = ProviderResult::default();

        let rt_ref = zoey_core::runtime_ref::downcast_runtime_ref(&runtime)
            .and_then(|rf| rf.try_upgrade())
            .ok_or_else(|| zoey_core::ZoeyError::other("Runtime unavailable"))?;

        // Clone adapter to avoid holding lock across await
        let adapter_opt = rt_ref.read().unwrap().get_adapter();
        if let Some(adapter) = adapter_opt {
            // Fetch recent messages
            let messages = adapter
                .get_memories(MemoryQuery {
                    room_id: Some(message.room_id),
                    count: Some(self.message_count),
                    table_name: "messages".to_string(),
                    ..Default::default()
                })
                .await
                .unwrap_or_default();

            // Fetch recent thoughts
            let thoughts = adapter
                .get_memories(MemoryQuery {
                    room_id: Some(message.room_id),
                    count: Some(self.thought_count),
                    table_name: "thoughts".to_string(),
                    ..Default::default()
                })
                .await
                .unwrap_or_default();

            let mut parts: Vec<String> = Vec::new();
            if !messages.is_empty() {
                let slice = messages.iter().rev().take(self.message_count);
                let m = slice
                    .map(|m| m.content.text.clone())
                    .collect::<Vec<_>>()
                    .join(" \n");
                parts.push(format!("Messages: {}", m));
            }
            if !thoughts.is_empty() {
                let slice = thoughts.iter().rev().take(self.thought_count);
                let t = slice
                    .map(|m| m.content.text.clone())
                    .collect::<Vec<_>>()
                    .join(" \n");
                parts.push(format!("Thoughts: {}", t));
            }

            let summary = if parts.is_empty() {
                String::new()
            } else {
                parts.join(" \n")
            };
            result.values = Some({
                let mut v = std::collections::HashMap::new();
                v.insert("CONTEXT_LAST_THOUGHT".to_string(), summary.clone());
                // Provide LAST_PROMPT from runtime settings if present
                if let Some(rt) = zoey_core::runtime_ref::downcast_runtime_ref(&runtime)
                    .and_then(|rf| rf.try_upgrade())
                {
                    let key = format!("ui:lastPrompt:{}:last", message.room_id);
                    if let Some(val) = rt
                        .read()
                        .unwrap()
                        .get_setting(&key)
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                    {
                        v.insert("LAST_PROMPT".to_string(), val);
                    }
                }
                v
            });
            result.data = Some({
                let mut d = std::collections::HashMap::new();
                d.insert(
                    "context_summary".to_string(),
                    serde_json::json!({
                        "messages_count": messages.len(),
                        "thoughts_count": thoughts.len(),
                    }),
                );
                d
            });
        }

        Ok(result)
    }
    fn capabilities(&self) -> Option<Vec<String>> {
        Some(vec!["CHAT".to_string()])
    }
}
