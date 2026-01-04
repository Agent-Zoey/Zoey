/*!
# Explainability Plugin

Plugin interface for integrating explainability features with ZoeyAI.
*/

use crate::source_attribution::SourceType;
use crate::{AuditLogManager, ExplainabilityEngine};
use async_trait::async_trait;
use zoey_core::types::{Evaluator, Memory, State};
use zoey_core::{ZoeyError, Plugin, Result};
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

/// Render the Observability plugin banner with settings
fn render_observability_banner(rows: Vec<SettingRow>) {
    let cyan = "\x1b[36m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{cyan}+{line}+{reset}", line = "=".repeat(78), cyan = cyan, reset = reset);

    // ASCII Art Header - OBSERVABILITY with telescope/eye aesthetic
    println!(
        "{cyan}|{bold}   ___  ____ ____  _____ ______     __   _    ____ ___ _     {reset}{cyan}  {dim}.--. {reset}{cyan} |{reset}",
        cyan = cyan, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{cyan}|{bold}  / _ \\| __ ) ___|| ____|  _ \\ \\   / /  / \\  | __ )_ _| |    {reset}{cyan} {dim}( oo ){reset}{cyan} |{reset}",
        cyan = cyan, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{cyan}|{bold} | | | |  _ \\___ \\|  _| | |_) \\ \\ / /  / _ \\ |  _ \\| || |    {reset}{cyan}  {dim}`--'{reset}{cyan}  |{reset}",
        cyan = cyan, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{cyan}|{bold} | |_| | |_) |__) | |___|  _ < \\ V /  / ___ \\| |_) | || |___ {reset}{cyan}  {dim}/||\\{reset}{cyan}  |{reset}",
        cyan = cyan, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{cyan}|{bold}  \\___/|____/____/|_____|_| \\_\\ \\_/  /_/   \\_\\____/___|_____|{reset}{cyan}  {dim}====={reset}{cyan}  |{reset}",
        cyan = cyan, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{cyan}|{reset}", cyan = cyan, reset = reset);
    println!(
        "{cyan}|{inner}|{reset}",
        inner = pad(&format!("  {yellow}Reasoning{reset}  {dim}~{reset}  {yellow}Attribution{reset}  {dim}~{reset}  {yellow}Confidence{reset}  {dim}~{reset}  {yellow}Audit Trails{reset}",
            yellow = yellow, dim = dim, reset = reset), 78),
        cyan = cyan, reset = reset
    );

    // Separator
    println!("{cyan}+{line}+{reset}", line = "-".repeat(78), cyan = cyan, reset = reset);

    // Settings header
    println!(
        "{cyan}|{a}|{b}|{c}|{d}|{reset}",
        a = pad(&format!(" {bold}ENV VARIABLE{reset}", bold = bold, reset = reset), 34),
        b = pad(&format!(" {bold}VALUE{reset}", bold = bold, reset = reset), 20),
        c = pad(&format!(" {bold}STATUS{reset}", bold = bold, reset = reset), 12),
        d = pad(" ", 12),
        cyan = cyan, reset = reset
    );
    println!("{cyan}+{line}+{reset}", line = "-".repeat(78), cyan = cyan, reset = reset);

    // Settings rows
    for row in &rows {
        let status_color = if row.is_default { dim } else { green };
        let status_text = if row.is_default { "default" } else { "custom" };
        let status_icon = if row.is_default { " " } else { "~" };

        println!(
            "{cyan}|{icon} {name}|{value}|{status}|{pad}|{reset}",
            icon = status_icon,
            name = pad(&row.env_var, 32),
            value = pad(&row.value, 20),
            status = pad(&format!("{status_color}{status_text}{reset}", status_color = status_color, status_text = status_text, reset = reset), 22),
            pad = pad("", 12),
            cyan = cyan, reset = reset
        );
    }

    // Legend
    println!("{cyan}+{line}+{reset}", line = "-".repeat(78), cyan = cyan, reset = reset);
    println!(
        "{cyan}|{inner}|{reset}",
        inner = pad(&format!("  {green}~{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        cyan = cyan, reset = reset
    );
    println!("{cyan}+{line}+{reset}", line = "=".repeat(78), cyan = cyan, reset = reset);
}

use tokio::sync::RwLock;

/// The explainability plugin
pub struct ExplainabilityPlugin {
    engine: Arc<RwLock<ExplainabilityEngine>>,
    audit_manager: Arc<RwLock<AuditLogManager>>,
}

impl ExplainabilityPlugin {
    /// Create a new explainability plugin
    pub fn new() -> Self {
        Self {
            engine: Arc::new(RwLock::new(ExplainabilityEngine::new())),
            audit_manager: Arc::new(RwLock::new(AuditLogManager::new())),
        }
    }

    /// Get the explainability engine
    pub fn engine(&self) -> Arc<RwLock<ExplainabilityEngine>> {
        self.engine.clone()
    }

    /// Get the audit log manager
    pub fn audit_manager(&self) -> Arc<RwLock<AuditLogManager>> {
        self.audit_manager.clone()
    }
}

impl Default for ExplainabilityPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for ExplainabilityPlugin {
    fn name(&self) -> &str {
        "explainability"
    }

    fn description(&self) -> &str {
        "Provides reasoning chains, source attribution, confidence scoring, and audit trails"
    }

    fn dependencies(&self) -> Vec<String> {
        vec![]
    }

    fn priority(&self) -> i32 {
        100 // High priority for compliance features
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        let rows = vec![
            SettingRow {
                name: "OBS_REST_PORT".to_string(),
                value: std::env::var("OBS_REST_PORT").unwrap_or_else(|_| "8080".to_string()),
                is_default: std::env::var("OBS_REST_PORT").is_err(),
                env_var: "OBS_REST_PORT".to_string(),
            },
            SettingRow {
                name: "OBS_TRACE_LEVEL".to_string(),
                value: std::env::var("OBS_TRACE_LEVEL").unwrap_or_else(|_| "info".to_string()),
                is_default: std::env::var("OBS_TRACE_LEVEL").is_err(),
                env_var: "OBS_TRACE_LEVEL".to_string(),
            },
            SettingRow {
                name: "OBS_AUDIT_ENABLED".to_string(),
                value: std::env::var("OBS_AUDIT_ENABLED").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("OBS_AUDIT_ENABLED").is_err(),
                env_var: "OBS_AUDIT_ENABLED".to_string(),
            },
            SettingRow {
                name: "OBS_REASONING_CHAINS".to_string(),
                value: std::env::var("OBS_REASONING_CHAINS").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("OBS_REASONING_CHAINS").is_err(),
                env_var: "OBS_REASONING_CHAINS".to_string(),
            },
            SettingRow {
                name: "OBS_SOURCE_ATTRIBUTION".to_string(),
                value: std::env::var("OBS_SOURCE_ATTRIBUTION").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("OBS_SOURCE_ATTRIBUTION").is_err(),
                env_var: "OBS_SOURCE_ATTRIBUTION".to_string(),
            },
        ];
        render_observability_banner(rows);
        Ok(())
    }

    fn actions(&self) -> Vec<Arc<dyn zoey_core::Action>> {
        vec![]
    }

    fn providers(&self) -> Vec<Arc<dyn zoey_core::Provider>> {
        vec![]
    }

    fn evaluators(&self) -> Vec<Arc<dyn zoey_core::Evaluator>> {
        vec![Arc::new(ChatExplainabilityEvaluator::new(
            self.engine.clone(),
        ))]
    }

    fn services(&self) -> Vec<Arc<dyn zoey_core::Service>> {
        vec![]
    }

    fn models(&self) -> std::collections::HashMap<String, zoey_core::ModelHandler> {
        std::collections::HashMap::new()
    }

    fn events(&self) -> std::collections::HashMap<String, Vec<zoey_core::EventHandler>> {
        std::collections::HashMap::new()
    }

    fn routes(&self) -> Vec<zoey_core::Route> {
        vec![]
    }

    fn schema(&self) -> Option<serde_json::Value> {
        None
    }
}

struct ChatExplainabilityEvaluator {
    engine: Arc<RwLock<ExplainabilityEngine>>,
}

impl ChatExplainabilityEvaluator {
    fn new(engine: Arc<RwLock<ExplainabilityEngine>>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl Evaluator for ChatExplainabilityEvaluator {
    fn name(&self) -> &str {
        "chat_explainability"
    }
    fn description(&self) -> &str {
        "Records explainability context and audit log for each chat turn"
    }
    fn always_run(&self) -> bool {
        true
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
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        did_respond: bool,
        responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        use crate::source_attribution::SourceType;
        use crate::{
            ConfidenceScore, ExplainabilityContext, ReasoningChain, ReasoningStepType, Source,
            SourceAttribution,
        };
        let mut chain = ReasoningChain::new("chat_turn".to_string());
        chain.add_step(
            ReasoningStepType::Observation,
            message.content.text.clone(),
            1.0,
        );
        if did_respond {
            if let Some(rs) = responses {
                if let Some(first) = rs.first() {
                    chain.add_step(
                        ReasoningStepType::Conclusion,
                        first.content.text.clone(),
                        0.8,
                    );
                }
            }
        }
        let mut ctx = ExplainabilityContext::new(chain);
        let source = Source::new(SourceType::UserInput, "User message");
        let attribution = SourceAttribution::new(source, 1.0, 1.0);
        ctx.add_source(attribution);
        ctx.set_confidence(ConfidenceScore::new(
            ctx.reasoning_chain.overall_confidence(),
        ));
        let mut engine = self.engine.write().await;
        engine
            .record(&ctx)
            .map_err(|e| ZoeyError::other(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_creation() {
        let plugin = ExplainabilityPlugin::new();
        assert_eq!(plugin.name(), "explainability");
        assert_eq!(plugin.priority(), 100);
    }

    #[tokio::test]
    async fn test_evaluator_available() {
        let plugin = ExplainabilityPlugin::new();
        let evals = plugin.evaluators();
        assert!(evals.iter().any(|e| e.name() == "chat_explainability"));
    }
}
