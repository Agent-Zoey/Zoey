use async_trait::async_trait;
use chrono::{DateTime, Utc};
use zoey_core::{types::*, Result};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

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

/// Render the Search plugin banner with settings
fn render_search_banner(rows: Vec<SettingRow>) {
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{yellow}+{line}+{reset}", line = "=".repeat(78), yellow = yellow, reset = reset);

    // ASCII Art Header - SEARCH with magnifying glass aesthetic
    println!(
        "{yellow}|{bold}  ____  _____    _    ____   ____ _   _                  {reset}{yellow}    {dim}.--.{reset}{yellow}    |{reset}",
        yellow = yellow, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{yellow}|{bold} / ___|| ____|  / \\  |  _ \\ / ___| | | |                 {reset}{yellow}   {dim}/    \\{reset}{yellow}   |{reset}",
        yellow = yellow, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{yellow}|{bold} \\___ \\|  _|   / _ \\ | |_) | |   | |_| |                 {reset}{yellow}   {dim}\\____/{reset}{yellow}   |{reset}",
        yellow = yellow, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{yellow}|{bold}  ___) | |___ / ___ \\|  _ <| |___|  _  |                 {reset}{yellow}    {dim}\\{reset}{yellow}      |{reset}",
        yellow = yellow, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{yellow}|{bold} |____/|_____/_/   \\_\\_| \\_\\\\____|_| |_|                 {reset}{yellow}     {dim}\\{reset}{yellow}     |{reset}",
        yellow = yellow, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{yellow}|{reset}", yellow = yellow, reset = reset);
    println!(
        "{yellow}|{inner}|{reset}",
        inner = pad(&format!("      {yellow}Web Search{reset}  {dim}?{reset}  {yellow}Caching{reset}  {dim}?{reset}  {yellow}Results{reset}  {dim}?{reset}  {yellow}DuckDuckGo{reset}",
            yellow = yellow, dim = dim, reset = reset), 78),
        yellow = yellow, reset = reset
    );

    // Separator
    println!("{yellow}+{line}+{reset}", line = "-".repeat(78), yellow = yellow, reset = reset);

    // Settings header
    println!(
        "{yellow}|{a}|{b}|{c}|{d}|{reset}",
        a = pad(&format!(" {bold}ENV VARIABLE{reset}", bold = bold, reset = reset), 34),
        b = pad(&format!(" {bold}VALUE{reset}", bold = bold, reset = reset), 20),
        c = pad(&format!(" {bold}STATUS{reset}", bold = bold, reset = reset), 12),
        d = pad(" ", 12),
        yellow = yellow, reset = reset
    );
    println!("{yellow}+{line}+{reset}", line = "-".repeat(78), yellow = yellow, reset = reset);

    // Settings rows
    if rows.is_empty() {
        println!(
            "{yellow}|{inner}|{reset}",
            inner = pad(&format!("  {dim}No configurable settings - uses DuckDuckGo by default{reset}", dim = dim, reset = reset), 78),
            yellow = yellow, reset = reset
        );
    } else {
        for row in &rows {
            let status_color = if row.is_default { dim } else { green };
            let status_text = if row.is_default { "default" } else { "custom" };
            let status_icon = if row.is_default { " " } else { "?" };

            println!(
                "{yellow}|{icon} {name}|{value}|{status}|{pad}|{reset}",
                icon = status_icon,
                name = pad(&row.env_var, 32),
                value = pad(&row.value, 20),
                status = pad(&format!("{status_color}{status_text}{reset}", status_color = status_color, status_text = status_text, reset = reset), 22),
                pad = pad("", 12),
                yellow = yellow, reset = reset
            );
        }
    }

    // Legend
    println!("{yellow}+{line}+{reset}", line = "-".repeat(78), yellow = yellow, reset = reset);
    println!(
        "{yellow}|{inner}|{reset}",
        inner = pad(&format!("  {green}?{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        yellow = yellow, reset = reset
    );
    println!("{yellow}+{line}+{reset}", line = "=".repeat(78), yellow = yellow, reset = reset);
}

pub struct SearchPlugin;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    id: Uuid,
    query: String,
    results: Vec<String>,
    created_at: DateTime<Utc>,
    ttl_seconds: i64,
}

#[async_trait]
impl Plugin for SearchPlugin {
    fn name(&self) -> &str {
        "search"
    }
    fn description(&self) -> &str {
        "Web search with caching"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        let rows = vec![
            SettingRow {
                name: "SEARCH_CACHE_TTL".to_string(),
                value: std::env::var("SEARCH_CACHE_TTL").unwrap_or_else(|_| "3600".to_string()),
                is_default: std::env::var("SEARCH_CACHE_TTL").is_err(),
                env_var: "SEARCH_CACHE_TTL".to_string(),
            },
            SettingRow {
                name: "SEARCH_MAX_RESULTS".to_string(),
                value: std::env::var("SEARCH_MAX_RESULTS").unwrap_or_else(|_| "10".to_string()),
                is_default: std::env::var("SEARCH_MAX_RESULTS").is_err(),
                env_var: "SEARCH_MAX_RESULTS".to_string(),
            },
            SettingRow {
                name: "SEARCH_PROVIDER".to_string(),
                value: std::env::var("SEARCH_PROVIDER").unwrap_or_else(|_| "duckduckgo".to_string()),
                is_default: std::env::var("SEARCH_PROVIDER").is_err(),
                env_var: "SEARCH_PROVIDER".to_string(),
            },
        ];
        render_search_banner(rows);
        Ok(())
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![Arc::new(WebSearchAction), Arc::new(CacheStatusAction)]
    }
}

async fn search_web(query: &str) -> anyhow::Result<Vec<String>> {
    let url = format!("https://duckduckgo.com/html/?q={}", query);
    let resp = reqwest::get(&url).await?.text().await?;
    let re = Regex::new(r#"<a[^>]+class=\"result__a\"[^>]*>(.*?)</a>"#).unwrap();
    let mut titles = Vec::new();
    for cap in re.captures_iter(&resp) {
        titles.push(cap[1].to_string());
    }
    Ok(titles)
}

struct WebSearchAction;
struct CacheStatusAction;

#[async_trait]
impl Action for WebSearchAction {
    fn name(&self) -> &str {
        "web_search"
    }
    fn description(&self) -> &str {
        "Perform web search and cache results"
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
        state: &State,
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
                .unwrap_or_else(|| {
                    state
                        .get_value("MESSAGE_TEXT")
                        .map(|s| s.as_str())
                        .unwrap_or("")
                });
            let results = search_web(q).await.unwrap_or_default();
            let entry = CacheEntry {
                id: Uuid::new_v4(),
                query: q.to_string(),
                results: results.clone(),
                created_at: Utc::now(),
                ttl_seconds: 3600,
            };
            let comp = Component {
                id: entry.id,
                entity_id: agent_id,
                world_id: agent_id,
                source_entity_id: None,
                component_type: "search-cache".to_string(),
                data: serde_json::to_value(&entry).unwrap(),
                created_at: Some(entry.created_at.timestamp()),
                updated_at: Some(entry.created_at.timestamp()),
            };
            let _ = adapter.create_component(&comp).await;
            let mut data = HashMap::new();
            data.insert(
                "results".to_string(),
                serde_json::to_value(results).unwrap(),
            );
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

#[async_trait]
impl Action for CacheStatusAction {
    fn name(&self) -> &str {
        "cache_status"
    }
    fn description(&self) -> &str {
        "Show recent search cache entries"
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
        _options: Option<HandlerOptions>,
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
            let comps = adapter
                .get_components(agent_id, None, None)
                .await
                .unwrap_or_default();
            let entries: Vec<serde_json::Value> = comps
                .into_iter()
                .filter(|c| c.component_type == "search-cache")
                .map(|c| c.data)
                .collect();
            let mut data = HashMap::new();
            data.insert("cache".to_string(), serde_json::to_value(entries).unwrap());
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
