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
            &format!("{} Provider • Dialogue • Rolling {}", deco, deco),
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

pub struct DialogueSummaryProvider {
    pub recent_turns: usize,
}

impl Default for DialogueSummaryProvider {
    fn default() -> Self {
        Self { recent_turns: 6 }
    }
}

#[async_trait]
impl Provider for DialogueSummaryProvider {
    fn name(&self) -> &str {
        "dialogue_summary"
    }
    fn description(&self) -> Option<String> {
        Some("Rolling summary of recent dialogue".to_string())
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
            let rows = vec![SettingRow {
                name: "DIALOGUE_SUMMARY_TURNS".to_string(),
                value: self.recent_turns.to_string(),
                source: "default".to_string(),
                change: "set via DialogueSummaryProvider::default()".to_string(),
            }];
            render("dialogue_summary", "\x1b[35m", "#", rows);
        });
        let rt_ref = zoey_core::runtime_ref::downcast_runtime_ref(&runtime)
            .and_then(|rf| rf.try_upgrade())
            .ok_or_else(|| zoey_core::ZoeyError::other("Runtime unavailable"))?;

        let adapter_opt = rt_ref.read().unwrap().get_adapter();
        let mut summary = String::new();
        if let Some(adapter) = adapter_opt {
            let messages = adapter
                .get_memories(MemoryQuery {
                    room_id: Some(message.room_id),
                    count: Some(self.recent_turns),
                    table_name: "messages".to_string(),
                    ..Default::default()
                })
                .await
                .unwrap_or_default();

            if !messages.is_empty() {
                let slice = messages.iter().rev().take(self.recent_turns);
                let joined = slice
                    .map(|m| m.content.text.clone())
                    .collect::<Vec<_>>()
                    .join(" ");
                summary = joined;
            }
        }

        let mut result = ProviderResult::default();
        result.values = Some({
            let mut v = std::collections::HashMap::new();
            v.insert("DIALOGUE_SUMMARY".to_string(), summary.clone());
            v.insert("CONTEXT_LAST_THOUGHT".to_string(), summary.clone());
            v
        });
        result.data = Some({
            let mut d = std::collections::HashMap::new();
            d.insert(
                "dialogue_summary".to_string(),
                serde_json::json!({
                    "turns": self.recent_turns,
                    "length": summary.len(),
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
