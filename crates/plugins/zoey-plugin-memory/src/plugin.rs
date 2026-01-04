use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// ANSI Art Banner Rendering
// ============================================================================

/// Represents a configuration setting row for display
struct SettingRow {
    name: String,
    value: String,
    is_default: bool,
    env_var: String,
}

/// Pad string to width, truncating if necessary
fn pad(s: &str, w: usize) -> String {
    let mut out = s.to_string();
    if out.len() > w {
        out.truncate(w);
    }
    let pad_len = if w > out.len() { w - out.len() } else { 0 };
    out + &" ".repeat(pad_len)
}

/// Render the Memory plugin banner with settings
fn render_memory_banner(rows: Vec<SettingRow>) {
    let green = "\x1b[32m";
    let yellow = "\x1b[33m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{green}+{line}+{reset}", line = "=".repeat(78), green = green, reset = reset);

    // ASCII Art Header - MEMORY with brain/circuit aesthetic
    println!(
        "{green}|{bold}  __  __ _____ __  __  ___  ______   __                  {reset}{green}   {dim}_.--._  {reset}{green}|{reset}",
        green = green, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{green}|{bold} |  \\/  | ____|  \\/  |/ _ \\|  _ \\ \\ / /                 {reset}{green}  {dim}(  o  o){reset}{green}|{reset}",
        green = green, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{green}|{bold} | |\\/| |  _| | |\\/| | | | | |_) \\ V /                  {reset}{green}   {dim}`--+--'{reset}{green}|{reset}",
        green = green, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{green}|{bold} | |  | | |___| |  | | |_| |  _ < | |                   {reset}{green}   {dim}/|   |\\{reset}{green}|{reset}",
        green = green, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{green}|{bold} |_|  |_|_____|_|  |_|\\___/|_| \\_\\|_|                   {reset}{green}  {dim}/ |___| \\{reset}{green}|{reset}",
        green = green, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{green}|{reset}", green = green, reset = reset);
    println!(
        "{green}|{inner}|{reset}",
        inner = pad(&format!("    {yellow}Tiering{reset}  {dim}@{reset}  {yellow}Budgeting{reset}  {dim}@{reset}  {yellow}Recency{reset}  {dim}@{reset}  {yellow}Importance Scoring{reset}",
            yellow = yellow, dim = dim, reset = reset), 78),
        green = green, reset = reset
    );

    // Separator
    println!("{green}+{line}+{reset}", line = "-".repeat(78), green = green, reset = reset);

    // Settings header
    println!(
        "{green}|{a}|{b}|{c}|{d}|{reset}",
        a = pad(&format!(" {bold}ENV VARIABLE{reset}", bold = bold, reset = reset), 34),
        b = pad(&format!(" {bold}VALUE{reset}", bold = bold, reset = reset), 20),
        c = pad(&format!(" {bold}STATUS{reset}", bold = bold, reset = reset), 12),
        d = pad(" ", 12),
        green = green, reset = reset
    );
    println!("{green}+{line}+{reset}", line = "-".repeat(78), green = green, reset = reset);

    // Settings rows
    if rows.is_empty() {
        println!(
            "{green}|{inner}|{reset}",
            inner = pad(&format!("  {dim}Configure via config_schema settings{reset}", dim = dim, reset = reset), 78),
            green = green, reset = reset
        );
    } else {
        for row in &rows {
            let status_color = if row.is_default { dim } else { green };
            let status_text = if row.is_default { "default" } else { "custom" };
            let status_icon = if row.is_default { " " } else { "@" };

            println!(
                "{green}|{icon} {name}|{value}|{status}|{pad}|{reset}",
                icon = status_icon,
                name = pad(&row.env_var, 32),
                value = pad(&row.value, 20),
                status = pad(&format!("{status_color}{status_text}{reset}", status_color = status_color, status_text = status_text, reset = reset), 22),
                pad = pad("", 12),
                green = green, reset = reset
            );
        }
    }

    // Legend
    println!("{green}+{line}+{reset}", line = "-".repeat(78), green = green, reset = reset);
    println!(
        "{green}|{inner}|{reset}",
        inner = pad(&format!("  {green}@{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        green = green, reset = reset
    );
    println!("{green}+{line}+{reset}", line = "=".repeat(78), green = green, reset = reset);
}

use crate::evaluators::{LongTermExtractionEvaluator, SummarizationEvaluator};
use crate::{ContextMemoriesProvider, MemoryPolicy, TieredMemoryService};

pub struct MemoryManagerPlugin {
    policy: MemoryPolicy,
}

impl Default for MemoryManagerPlugin {
    fn default() -> Self {
        Self {
            policy: MemoryPolicy::default(),
        }
    }
}

#[async_trait]
impl Plugin for MemoryManagerPlugin {
    fn name(&self) -> &str {
        "memory-manager"
    }
    fn description(&self) -> &str {
        "Tiered memory, budgeting, recency/importance scoring"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        let mut rows: Vec<SettingRow> = Vec::new();
        if let Some(schema) = self.config_schema() {
            if let Some(map) = schema.as_object() {
                for (key, val) in map {
                    let def = val
                        .get("default")
                        .map(|v| v.to_string().replace('"', ""))
                        .unwrap_or_default();
                    let env_v = std::env::var(key).ok();
                    let cfg_v = _config.get(key).cloned();
                    let (value, is_default) = if let Some(v) = cfg_v {
                        (v, false)
                    } else if let Some(v) = env_v {
                        (v, false)
                    } else {
                        (def, true)
                    };
                    rows.push(SettingRow {
                        name: key.clone(),
                        value,
                        is_default,
                        env_var: key.clone(),
                    });
                }
            }
        }
        render_memory_banner(rows);
        Ok(())
    }

    fn providers(&self) -> Vec<Arc<dyn Provider>> {
        vec![Arc::new(ContextMemoriesProvider::default())]
    }

    fn services(&self) -> Vec<Arc<dyn Service>> {
        vec![Arc::new(TieredMemoryService::new(self.policy.clone()))]
    }

    fn evaluators(&self) -> Vec<Arc<dyn Evaluator>> {
        vec![
            Arc::new(SummarizationEvaluator),
            Arc::new(LongTermExtractionEvaluator),
        ]
    }

    fn schema(&self) -> Option<serde_json::Value> {
        // Dynamic migrations for long_term_memories, session_summaries, memory_access_logs
        Some(serde_json::json!({
            "long_term_memories": {
                "columns": {
                    "id": "UUID",
                    "entity_id": "UUID",
                    "agent_id": "UUID",
                    "room_id": "UUID",
                    "content": "TEXT",
                    "metadata": "JSONB",
                    "created_at": "TIMESTAMP",
                    "unique_flag": "BOOLEAN"
                }
            },
            "session_summaries": {
                "columns": {
                    "id": "UUID",
                    "entity_id": "UUID",
                    "agent_id": "UUID",
                    "room_id": "UUID",
                    "content": "TEXT",
                    "metadata": "JSONB",
                    "created_at": "TIMESTAMP",
                    "unique_flag": "BOOLEAN"
                }
            },
            "memory_access_logs": {
                "columns": {
                    "id": "UUID",
                    "agent_id": "UUID",
                    "entity_id": "UUID",
                    "room_id": "UUID",
                    "operation": "TEXT",
                    "created_at": "TIMESTAMP",
                    "metadata": "JSONB"
                }
            }
        }))
    }

    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "MEMORY_SUMMARIZATION_THRESHOLD": {"type": "integer", "default": 16},
            "MEMORY_SUMMARIZATION_INTERVAL": {"type": "integer", "default": 10},
            "MEMORY_RETAIN_RECENT": {"type": "integer", "default": 10},
            "MEMORY_CONFIDENCE_THRESHOLD": {"type": "number", "default": 0.85},
            "MEMORY_SUMMARY_MODEL": {"type": "string", "default": "TEXT_LARGE"},
            "MEMORY_RETENTION_HALFLIFE_DAYS": {"type": "number", "default": 30.0},
            "MEMORY_IMPORTANCE_THRESHOLD": {"type": "number", "default": 0.4},
            "MEMORY_CONTEXT_BUDGET_TOKENS": {"type": "integer", "default": 2000}
        }))
    }
}
