//! ZoeyOS Bootstrap Plugin
//!
//! Provides essential actions, providers, and evaluators for basic agent functionality.

#![warn(missing_docs)]
#![warn(clippy::all)]

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

/// Render the Bootstrap plugin banner with settings
fn render_bootstrap_banner(rows: Vec<SettingRow>) {
    let cyan = "\x1b[36m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{cyan}+{line}+{reset}", line = "=".repeat(78), cyan = cyan, reset = reset);

    // ASCII Art Header - BOOTSTRAP with gear/cog aesthetic
    println!(
        "{cyan}|{bold}  ____   ___   ___ _____ ____ _____ ____      _     ____  {reset}{cyan}   [*] [*]   |{reset}",
        cyan = cyan, bold = bold, reset = reset
    );
    println!(
        "{cyan}|{bold} | __ ) / _ \\ / _ \\_   _/ ___|_   _|  _ \\    / \\   |  _ \\ {reset}{cyan}   {dim}o---o{reset}{cyan}   |{reset}",
        cyan = cyan, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{cyan}|{bold} |  _ \\| | | | | | || | \\___ \\ | | | |_) |  / _ \\  | |_) |{reset}{cyan}  {dim}/|   |\\{reset}{cyan}  |{reset}",
        cyan = cyan, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{cyan}|{bold} | |_) | |_| | |_| || |  ___) || | |  _ <  / ___ \\ |  __/ {reset}{cyan}  {dim}o-----o{reset}{cyan}  |{reset}",
        cyan = cyan, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{cyan}|{bold} |____/ \\___/ \\___/ |_| |____/ |_| |_| \\_\\/_/   \\_\\|_|    {reset}{cyan}   {dim}[===]{reset}{cyan}   |{reset}",
        cyan = cyan, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{cyan}|{reset}", cyan = cyan, reset = reset);
    println!(
        "{cyan}|{inner}|{reset}",
        inner = pad(&format!("  {yellow}Essential Providers{reset}  {dim}*{reset}  {yellow}Actions{reset}  {dim}*{reset}  {yellow}Evaluators{reset}  {dim}*{reset}  {yellow}Core Foundations{reset}",
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
    if rows.is_empty() {
        println!(
            "{cyan}|{inner}|{reset}",
            inner = pad(&format!("  {dim}No configurable settings - plugin runs with defaults{reset}", dim = dim, reset = reset), 78),
            cyan = cyan, reset = reset
        );
    } else {
        for row in &rows {
            let status_color = if row.is_default { dim } else { green };
            let status_text = if row.is_default { "default" } else { "custom" };
            let status_icon = if row.is_default { " " } else { "*" };

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
    }

    // Legend
    println!("{cyan}+{line}+{reset}", line = "-".repeat(78), cyan = cyan, reset = reset);
    println!(
        "{cyan}|{inner}|{reset}",
        inner = pad(&format!("  {green}*{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        cyan = cyan, reset = reset
    );
    println!("{cyan}+{line}+{reset}", line = "=".repeat(78), cyan = cyan, reset = reset);
}

// Module declarations
pub mod actions;
pub mod evaluators;
pub mod functors;
pub mod providers;

// Re-exports
pub use actions::*;
pub use zoey_core;
pub use evaluators::*;
pub use functors::*;
pub use providers::*;

/// Bootstrap plugin implementation
pub struct BootstrapPlugin;

impl BootstrapPlugin {
    /// Create a new bootstrap plugin
    pub fn new() -> Self {
        Self
    }
}

impl Default for BootstrapPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for BootstrapPlugin {
    fn name(&self) -> &str {
        "bootstrap"
    }

    fn description(&self) -> &str {
        "Essential actions, providers, and evaluators for basic agent functionality"
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![
            Arc::new(ReplyAction),
            Arc::new(IgnoreAction),
            Arc::new(NoneAction),
            Arc::new(SendMessageAction),
            Arc::new(FollowRoomAction),
            Arc::new(UnfollowRoomAction),
            Arc::new(AskClarifyAction),
            Arc::new(SummarizeAndConfirmAction),
        ]
    }

    fn providers(&self) -> Vec<Arc<dyn Provider>> {
        vec![
            // Functors (run first for planning)
            Arc::new(ReactionPlannerFunctor), // Plans reaction before execution
            Arc::new(OutputPlannerFunctor),   // Plans output before reply
            // Regular providers
            Arc::new(TimeProvider),
            Arc::new(CharacterProvider),
            Arc::new(ActionsProvider),
            Arc::new(EntitiesProvider),
            Arc::new(RecentMessagesProvider::default()),
            Arc::new(ContextSummaryProvider::default()),
            Arc::new(DialogueSummaryProvider::default()),
            Arc::new(SessionCuesProvider),
            Arc::new(RecallProvider),
        ]
    }

    fn evaluators(&self) -> Vec<Arc<dyn Evaluator>> {
        vec![
            Arc::new(ReflectionEvaluator),
            Arc::new(FactExtractionEvaluator),
            Arc::new(GoalTrackingEvaluator),
            Arc::new(DirectAnswerEvaluator),
            Arc::new(BrevityEvaluator::default()),
            Arc::new(crate::evaluators::review::ConversationReviewEvaluator::default()),
        ]
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        // Bootstrap has no configurable settings - it's the foundation
        let rows: Vec<SettingRow> = Vec::new();
        render_bootstrap_banner(rows);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap_plugin_creation() {
        let plugin = BootstrapPlugin::new();
        assert_eq!(plugin.name(), "bootstrap");
        assert_eq!(
            plugin.description(),
            "Essential actions, providers, and evaluators for basic agent functionality"
        );
    }

    #[test]
    fn test_bootstrap_plugin_components() {
        let plugin = BootstrapPlugin::new();

        let actions = plugin.actions();
        assert_eq!(actions.len(), 8);

        let providers = plugin.providers();
        assert_eq!(providers.len(), 11);

        let evaluators = plugin.evaluators();
        assert_eq!(evaluators.len(), 6);
    }
}
