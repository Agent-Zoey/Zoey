//! Time provider - supplies current date/time to the agent

use async_trait::async_trait;
use chrono::{Datelike, Timelike, Utc};
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
        pad(&format!("{} Provider • Time • UTC {}", deco, deco), 78),
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

/// Time provider
pub struct TimeProvider;

#[async_trait]
impl Provider for TimeProvider {
    fn name(&self) -> &str {
        "time"
    }

    fn description(&self) -> Option<String> {
        Some("Provides current date and time information".to_string())
    }

    fn dynamic(&self) -> bool {
        true // Always re-evaluate to get current time
    }

    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        INIT.call_once(|| {
            let rows = vec![
                SettingRow {
                    name: "TIME_DYNAMIC".to_string(),
                    value: "true".to_string(),
                    source: "code".to_string(),
                    change: "Set via provider code".to_string(),
                },
                SettingRow {
                    name: "TIME_ZONE".to_string(),
                    value: "UTC".to_string(),
                    source: "code".to_string(),
                    change: "Use local conversion in templates".to_string(),
                },
            ];
            render("time", "\x1b[36m", "+", rows);
        });
        let now = Utc::now();

        let time_str = format!(
            "Current date and time: {} {}, {} at {}:{:02} UTC",
            now.weekday(),
            now.format("%B %e"),
            now.year(),
            now.hour(),
            now.minute()
        );

        let mut result = ProviderResult::default();
        result.text = Some(time_str);

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_time_provider() {
        let provider = TimeProvider;
        assert_eq!(provider.name(), "time");
        assert!(provider.dynamic());

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
        assert!(result.text.unwrap().contains("Current date and time"));
    }
    // removed misplaced free function duplicate
}
