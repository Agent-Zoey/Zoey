use async_trait::async_trait;
use chrono::Utc;
use zoey_core::{types::*, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use crate::connectors::*;

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

/// Render the RAG Connectors plugin banner with settings
fn render_rag_banner(rows: Vec<SettingRow>) {
    let purple = "\x1b[35m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{purple}+{line}+{reset}", line = "=".repeat(78), purple = purple, reset = reset);

    // ASCII Art Header - RAG with data flow aesthetic
    println!(
        "{purple}|{bold}  ____      _     ____    ____                            {reset}{purple}  {dim}>>===>{reset}{purple}  |{reset}",
        purple = purple, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{purple}|{bold} |  _ \\    / \\   / ___|  / ___|___  _ __  _ __   ___      {reset}{purple}  {dim}|DATA|{reset}{purple}  |{reset}",
        purple = purple, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{purple}|{bold} | |_) |  / _ \\ | |  _  | |   / _ \\| '_ \\| '_ \\ / __|     {reset}{purple}  {dim}<===<<{reset}{purple}  |{reset}",
        purple = purple, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{purple}|{bold} |  _ <  / ___ \\| |_| | | |__| (_) | | | | | | |\\__ \\     {reset}{purple}  {dim}|FLOW|{reset}{purple}  |{reset}",
        purple = purple, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{purple}|{bold} |_| \\_\\/_/   \\_\\\\____|  \\____\\___/|_| |_|_| |_||___/     {reset}{purple}  {dim}>>===>{reset}{purple}  |{reset}",
        purple = purple, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{purple}|{reset}", purple = purple, reset = reset);
    println!(
        "{purple}|{inner}|{reset}",
        inner = pad(&format!("    {yellow}Connectors{reset}  {dim}>{reset}  {yellow}Ingestion{reset}  {dim}>{reset}  {yellow}Pipelines{reset}  {dim}>{reset}  {yellow}Data Sources{reset}",
            yellow = yellow, dim = dim, reset = reset), 78),
        purple = purple, reset = reset
    );

    // Separator
    println!("{purple}+{line}+{reset}", line = "-".repeat(78), purple = purple, reset = reset);

    // Settings header
    println!(
        "{purple}|{a}|{b}|{c}|{d}|{reset}",
        a = pad(&format!(" {bold}ENV VARIABLE{reset}", bold = bold, reset = reset), 34),
        b = pad(&format!(" {bold}VALUE{reset}", bold = bold, reset = reset), 20),
        c = pad(&format!(" {bold}STATUS{reset}", bold = bold, reset = reset), 12),
        d = pad(" ", 12),
        purple = purple, reset = reset
    );
    println!("{purple}+{line}+{reset}", line = "-".repeat(78), purple = purple, reset = reset);

    // Settings rows
    if rows.is_empty() {
        println!(
            "{purple}|{inner}|{reset}",
            inner = pad(&format!("  {dim}Configure data sources via actions{reset}", dim = dim, reset = reset), 78),
            purple = purple, reset = reset
        );
    } else {
        for row in &rows {
            let status_color = if row.is_default { dim } else { green };
            let status_text = if row.is_default { "default" } else { "custom" };
            let status_icon = if row.is_default { " " } else { ">" };

            println!(
                "{purple}|{icon} {name}|{value}|{status}|{pad}|{reset}",
                icon = status_icon,
                name = pad(&row.env_var, 32),
                value = pad(&row.value, 20),
                status = pad(&format!("{status_color}{status_text}{reset}", status_color = status_color, status_text = status_text, reset = reset), 22),
                pad = pad("", 12),
                purple = purple, reset = reset
            );
        }
    }

    // Legend
    println!("{purple}+{line}+{reset}", line = "-".repeat(78), purple = purple, reset = reset);
    println!(
        "{purple}|{inner}|{reset}",
        inner = pad(&format!("  {green}>{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        purple = purple, reset = reset
    );
    println!("{purple}+{line}+{reset}", line = "=".repeat(78), purple = purple, reset = reset);
}

pub struct RagConnectorsPlugin;

#[async_trait]
impl Plugin for RagConnectorsPlugin {
    fn name(&self) -> &str {
        "rag-connectors"
    }
    fn description(&self) -> &str {
        "Connectors and ingestion pipelines for RAG"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        let rows = vec![
            SettingRow {
                name: "RAG_GITHUB_TOKEN".to_string(),
                value: if std::env::var("RAG_GITHUB_TOKEN").is_ok() { "(set)".to_string() } else { "(not set)".to_string() },
                is_default: std::env::var("RAG_GITHUB_TOKEN").is_err(),
                env_var: "RAG_GITHUB_TOKEN".to_string(),
            },
            SettingRow {
                name: "RAG_DEFAULT_CHUNK_SIZE".to_string(),
                value: std::env::var("RAG_DEFAULT_CHUNK_SIZE").unwrap_or_else(|_| "1000".to_string()),
                is_default: std::env::var("RAG_DEFAULT_CHUNK_SIZE").is_err(),
                env_var: "RAG_DEFAULT_CHUNK_SIZE".to_string(),
            },
            SettingRow {
                name: "RAG_OVERLAP_SIZE".to_string(),
                value: std::env::var("RAG_OVERLAP_SIZE").unwrap_or_else(|_| "200".to_string()),
                is_default: std::env::var("RAG_OVERLAP_SIZE").is_err(),
                env_var: "RAG_OVERLAP_SIZE".to_string(),
            },
            SettingRow {
                name: "RAG_AUTO_REFRESH".to_string(),
                value: std::env::var("RAG_AUTO_REFRESH").unwrap_or_else(|_| "false".to_string()),
                is_default: std::env::var("RAG_AUTO_REFRESH").is_err(),
                env_var: "RAG_AUTO_REFRESH".to_string(),
            },
        ];
        render_rag_banner(rows);
        Ok(())
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![
            Arc::new(AddSourceAction),
            Arc::new(ForceRefreshAction),
            Arc::new(SearchSourceAction),
        ]
    }
}

struct AddSourceAction;
struct ForceRefreshAction;
struct SearchSourceAction;

#[async_trait]
impl Action for AddSourceAction {
    fn name(&self) -> &str {
        "rag_add_source"
    }
    fn description(&self) -> &str {
        "Add a data source for ingestion"
    }
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }
    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
        options: Option<HandlerOptions>,
        _cb: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        use zoey_core::runtime_ref::downcast_runtime_ref;
        let rt_ref = downcast_runtime_ref(&runtime)
            .and_then(|r| r.try_upgrade())
            .unwrap();
        let (agent_id, adapter) = {
            let rt = rt_ref.read().unwrap();
            (rt.agent_id, rt.get_adapter())
        };
        if let Some(adapter) = adapter {
            let spec = SourceSpec {
                id: Uuid::new_v4(),
                kind: options
                    .as_ref()
                    .and_then(|o| o.custom.get("kind"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("web")
                    .to_string(),
                url: options
                    .as_ref()
                    .and_then(|o| o.custom.get("url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                auth_token: None,
                schedule_cron: None,
            };
            let comp = Component {
                id: spec.id,
                entity_id: agent_id,
                world_id: agent_id,
                source_entity_id: None,
                component_type: "rag-source".to_string(),
                data: serde_json::to_value(&spec).unwrap(),
                created_at: Some(Utc::now().timestamp()),
                updated_at: Some(Utc::now().timestamp()),
            };
            let _ = adapter.create_component(&comp).await;
            return Ok(Some(ActionResult {
                action_name: Some(self.name().to_string()),
                text: Some("Source added".to_string()),
                values: None,
                data: None,
                success: true,
                error: None,
            }));
        }
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some("No adapter configured".to_string()),
            values: None,
            data: None,
            success: false,
            error: None,
        }))
    }
}

#[async_trait]
impl Action for ForceRefreshAction {
    fn name(&self) -> &str {
        "rag_force_refresh"
    }
    fn description(&self) -> &str {
        "Fetch and ingest immediately"
    }
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }
    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
        options: Option<HandlerOptions>,
        _cb: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        use zoey_core::runtime_ref::downcast_runtime_ref;
        let rt_ref = downcast_runtime_ref(&runtime)
            .and_then(|r| r.try_upgrade())
            .unwrap();
        let (agent_id, adapter) = {
            let rt = rt_ref.read().unwrap();
            (rt.agent_id, rt.get_adapter())
        };
        if let Some(adapter) = adapter {
            if let Some(url) = options
                .as_ref()
                .and_then(|o| o.custom.get("url"))
                .and_then(|v| v.as_str())
            {
                let (title, text) = fetch_github_readme(url)
                    .await
                    .unwrap_or(("Document".to_string(), String::new()));
                let doc = IngestedDocument {
                    id: Uuid::new_v4(),
                    source_id: Uuid::new_v4(),
                    title,
                    text: text.clone(),
                    created_at: Utc::now(),
                };
                let mem = Memory {
                    id: doc.id,
                    entity_id: agent_id,
                    agent_id: agent_id,
                    room_id: agent_id,
                    content: Content {
                        text: doc.text.clone(),
                        ..Default::default()
                    },
                    embedding: None,
                    metadata: Some(MemoryMetadata {
                        memory_type: Some("rag-doc".to_string()),
                        entity_name: None,
                        data: HashMap::from([("title".to_string(), serde_json::json!(doc.title))]),
                    }),
                    created_at: Utc::now().timestamp(),
                    unique: Some(false),
                    similarity: None,
                };
                let _ = adapter.create_memory(&mem, "memories").await;
                info!("rag: ingested document");
                return Ok(Some(ActionResult {
                    action_name: Some(self.name().to_string()),
                    text: Some("Refreshed".to_string()),
                    values: None,
                    data: None,
                    success: true,
                    error: None,
                }));
            }
        }
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some("Missing url or adapter".to_string()),
            values: None,
            data: None,
            success: false,
            error: None,
        }))
    }
}

#[async_trait]
impl Action for SearchSourceAction {
    fn name(&self) -> &str {
        "rag_search"
    }
    fn description(&self) -> &str {
        "Search ingested documents"
    }
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }
    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
        options: Option<HandlerOptions>,
        _cb: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        use zoey_core::runtime_ref::downcast_runtime_ref;
        let rt_ref = downcast_runtime_ref(&runtime)
            .and_then(|r| r.try_upgrade())
            .unwrap();
        let (agent_id, adapter) = {
            let rt = rt_ref.read().unwrap();
            (rt.agent_id, rt.get_adapter())
        };
        if let Some(adapter) = adapter {
            let q = options
                .as_ref()
                .and_then(|o| o.custom.get("q"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let params = MemoryQuery {
                agent_id: Some(agent_id),
                room_id: None,
                count: Some(10),
                table_name: "memories".to_string(),
                ..Default::default()
            };
            let res = adapter.get_memories(params).await.unwrap_or_default();
            let mut data = HashMap::new();
            data.insert("results".to_string(), serde_json::to_value(res).unwrap());
            return Ok(Some(ActionResult {
                action_name: Some(self.name().to_string()),
                text: None,
                values: None,
                data: Some(data),
                success: true,
                error: None,
            }));
        }
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some("No adapter configured".to_string()),
            values: None,
            data: None,
            success: false,
            error: None,
        }))
    }
}
