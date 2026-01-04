/*!
# Knowledge Plugin

Plugin interface for integrating knowledge management with LauraAI.
*/

use async_trait::async_trait;
use zoey_core::{ZoeyError, Plugin};
use std::any::Any;
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

/// Render the Knowledge plugin banner with settings
fn render_knowledge_banner(rows: Vec<SettingRow>) {
    let magenta = "\x1b[35m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border with book corners
    println!("{magenta}+{line}+{reset}", line = "=".repeat(78), magenta = magenta, reset = reset);

    // ASCII Art Header - KNOWLEDGE with book/library aesthetic
    println!(
        "{magenta}|{bold}  _  ___   _  _____        ___     _____ ____   ____ _____  {reset}{magenta}  {dim}[|||]{reset}{magenta}   |{reset}",
        magenta = magenta, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{magenta}|{bold} | |/ / \\ | |/ _ \\ \\      / / |   | ____|  _ \\ / ___| ____| {reset}{magenta}  {dim}[|||]{reset}{magenta}   |{reset}",
        magenta = magenta, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{magenta}|{bold} | ' /|  \\| | | | \\ \\ /\\ / /| |   |  _| | | | | |  _|  _|   {reset}{magenta}  {dim}[|||]{reset}{magenta}   |{reset}",
        magenta = magenta, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{magenta}|{bold} | . \\| |\\  | |_| |\\ V  V / | |___| |___| |_| | |_| | |___  {reset}{magenta} {dim}/____\\{reset}{magenta}  |{reset}",
        magenta = magenta, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{magenta}|{bold} |_|\\_\\_| \\_|\\___/  \\_/\\_/  |_____|_____|____/ \\____|_____| {reset}{magenta} {dim}|DOCS|{reset}{magenta}  |{reset}",
        magenta = magenta, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{magenta}|{reset}", magenta = magenta, reset = reset);
    println!(
        "{magenta}|{inner}|{reset}",
        inner = pad(&format!("    {yellow}Graph{reset}  {dim}*{reset}  {yellow}Ingestion{reset}  {dim}*{reset}  {yellow}Retrieval{reset}  {dim}*{reset}  {yellow}Domain Intelligence{reset}",
            yellow = yellow, dim = dim, reset = reset), 78),
        magenta = magenta, reset = reset
    );

    // Separator
    println!("{magenta}+{line}+{reset}", line = "-".repeat(78), magenta = magenta, reset = reset);

    // Settings header
    println!(
        "{magenta}|{a}|{b}|{c}|{d}|{reset}",
        a = pad(&format!(" {bold}ENV VARIABLE{reset}", bold = bold, reset = reset), 34),
        b = pad(&format!(" {bold}VALUE{reset}", bold = bold, reset = reset), 20),
        c = pad(&format!(" {bold}STATUS{reset}", bold = bold, reset = reset), 12),
        d = pad(" ", 12),
        magenta = magenta, reset = reset
    );
    println!("{magenta}+{line}+{reset}", line = "-".repeat(78), magenta = magenta, reset = reset);

    // Settings rows
    for row in &rows {
        let status_color = if row.is_default { dim } else { green };
        let status_text = if row.is_default { "default" } else { "custom" };
        let status_icon = if row.is_default { " " } else { "*" };

        println!(
            "{magenta}|{icon} {name}|{value}|{status}|{pad}|{reset}",
            icon = status_icon,
            name = pad(&row.env_var, 32),
            value = pad(&row.value, 20),
            status = pad(&format!("{status_color}{status_text}{reset}", status_color = status_color, status_text = status_text, reset = reset), 22),
            pad = pad("", 12),
            magenta = magenta, reset = reset
        );
    }

    // Legend
    println!("{magenta}+{line}+{reset}", line = "-".repeat(78), magenta = magenta, reset = reset);
    println!(
        "{magenta}|{inner}|{reset}",
        inner = pad(&format!("  {green}*{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        magenta = magenta, reset = reset
    );
    println!("{magenta}+{line}+{reset}", line = "=".repeat(78), magenta = magenta, reset = reset);
}

/// The knowledge management plugin
pub struct KnowledgePlugin {
    config: crate::KnowledgeConfig,
}

impl KnowledgePlugin {
    /// Create a new knowledge plugin
    pub fn new() -> Self {
        Self {
            config: crate::KnowledgeConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: crate::KnowledgeConfig) -> Self {
        Self { config }
    }
}

impl Default for KnowledgePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for KnowledgePlugin {
    fn name(&self) -> &str {
        "knowledge"
    }

    fn description(&self) -> &str {
        "Comprehensive knowledge management: document ingestion, knowledge graphs, and advanced retrieval"
    }

    fn dependencies(&self) -> Vec<String> {
        vec![]
    }

    fn priority(&self) -> i32 {
        50 // Medium priority
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn Any + Send + Sync>,
    ) -> Result<(), ZoeyError> {
        let cfg = &self.config;
        let default_cfg = crate::KnowledgeConfig::default();

        let rows = vec![
            SettingRow {
                name: "enable_pdf".to_string(),
                value: format!("{}", cfg.enable_pdf),
                is_default: cfg.enable_pdf == default_cfg.enable_pdf,
                env_var: "KNOWLEDGE_ENABLE_PDF".to_string(),
            },
            SettingRow {
                name: "enable_markdown".to_string(),
                value: format!("{}", cfg.enable_markdown),
                is_default: cfg.enable_markdown == default_cfg.enable_markdown,
                env_var: "KNOWLEDGE_ENABLE_MARKDOWN".to_string(),
            },
            SettingRow {
                name: "enable_csv".to_string(),
                value: format!("{}", cfg.enable_csv),
                is_default: cfg.enable_csv == default_cfg.enable_csv,
                env_var: "KNOWLEDGE_ENABLE_CSV".to_string(),
            },
            SettingRow {
                name: "enable_json".to_string(),
                value: format!("{}", cfg.enable_json),
                is_default: cfg.enable_json == default_cfg.enable_json,
                env_var: "KNOWLEDGE_ENABLE_JSON".to_string(),
            },
            SettingRow {
                name: "max_document_size".to_string(),
                value: format!("{}MB", cfg.max_document_size / (1024 * 1024)),
                is_default: cfg.max_document_size == default_cfg.max_document_size,
                env_var: "KNOWLEDGE_MAX_DOC_SIZE".to_string(),
            },
            SettingRow {
                name: "enable_entity_extraction".to_string(),
                value: format!("{}", cfg.enable_entity_extraction),
                is_default: cfg.enable_entity_extraction == default_cfg.enable_entity_extraction,
                env_var: "KNOWLEDGE_ENTITY_EXTRACTION".to_string(),
            },
            SettingRow {
                name: "semantic_weight".to_string(),
                value: format!("{:.2}", cfg.semantic_weight),
                is_default: (cfg.semantic_weight - default_cfg.semantic_weight).abs() < 0.001,
                env_var: "KNOWLEDGE_SEMANTIC_WEIGHT".to_string(),
            },
            SettingRow {
                name: "lexical_weight".to_string(),
                value: format!("{:.2}", cfg.lexical_weight),
                is_default: (cfg.lexical_weight - default_cfg.lexical_weight).abs() < 0.001,
                env_var: "KNOWLEDGE_LEXICAL_WEIGHT".to_string(),
            },
            SettingRow {
                name: "graph_weight".to_string(),
                value: format!("{:.2}", cfg.graph_weight),
                is_default: (cfg.graph_weight - default_cfg.graph_weight).abs() < 0.001,
                env_var: "KNOWLEDGE_GRAPH_WEIGHT".to_string(),
            },
        ];
        render_knowledge_banner(rows);
        Ok(())
    }

    fn actions(&self) -> Vec<Arc<dyn zoey_core::Action>> {
        vec![]
    }

    fn providers(&self) -> Vec<Arc<dyn zoey_core::Provider>> {
        vec![]
    }

    fn evaluators(&self) -> Vec<Arc<dyn zoey_core::Evaluator>> {
        vec![]
    }

    fn services(&self) -> Vec<Arc<dyn zoey_core::Service>> {
        vec![]
    }

    fn models(&self) -> HashMap<String, zoey_core::ModelHandler> {
        HashMap::new()
    }

    fn events(&self) -> HashMap<String, Vec<zoey_core::EventHandler>> {
        HashMap::new()
    }

    fn routes(&self) -> Vec<zoey_core::Route> {
        vec![]
    }

    fn schema(&self) -> Option<serde_json::Value> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_creation() {
        let plugin = KnowledgePlugin::new();
        assert_eq!(plugin.name(), "knowledge");
        assert_eq!(plugin.priority(), 50);
    }
}
