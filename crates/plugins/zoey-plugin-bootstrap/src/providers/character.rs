//! Character provider - supplies character information to the agent

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
        pad(&format!("{} Provider • Character • Bio {}", deco, deco), 78),
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

/// Character provider
pub struct CharacterProvider;

#[async_trait]
impl Provider for CharacterProvider {
    fn name(&self) -> &str {
        "character"
    }

    fn description(&self) -> Option<String> {
        Some("Provides character bio, lore, and personality".to_string())
    }

    async fn get(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        INIT.call_once(|| {
            let rows = vec![SettingRow {
                name: "CHARACTER_SOURCE".to_string(),
                value: "runtime".to_string(),
                source: "code".to_string(),
                change: "Set via character XML".to_string(),
            }];
            render("character", "\x1b[35m", "#", rows);
        });
        let mut result = ProviderResult::default();

        // Try to downcast to RuntimeRef to access character information
        if let Some(runtime_ref) = zoey_core::downcast_runtime_ref(&runtime) {
            // Access runtime through weak reference if available
            if let Some(rt_arc) = runtime_ref.try_upgrade() {
                let rt = rt_arc.read().unwrap();

                // Set CHARACTER key for template
                let character_info = format!(
                    "Name: {}\n\
                    Bio: {}\n\
                    Lore: {}\n\
                    Style: {}\n\
                    Adjectives: {}\n\
                    Topics: {}",
                    rt.character.name,
                    rt.character.bio.join("\n"),
                    rt.character.lore.join("\n"),
                    rt.character.style.all.join("\n- "),
                    rt.character.adjectives.join(", "),
                    rt.character.topics.join(", ")
                );

                result.text = Some(character_info.clone());

                // Set values for template substitution
                result.values = Some({
                    let mut values = std::collections::HashMap::new();
                    values.insert("CHARACTER".to_string(), character_info);
                    values.insert("AGENT_NAME".to_string(), rt.character.name.clone());
                    values.insert("bio".to_string(), rt.character.bio.join("\n"));
                    values
                });

                return Ok(result);
            }
        }

        // Fallback if runtime not available
        result.text = Some(format!(
            "Character Information:\n\
            [Runtime reference not available - using template response]"
        ));

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_character_provider() {
        let provider = CharacterProvider;
        assert_eq!(provider.name(), "character");

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
    }
}
