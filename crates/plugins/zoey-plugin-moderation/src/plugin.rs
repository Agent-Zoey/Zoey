use async_trait::async_trait;
use zoey_core::{types::*, Result};
use regex::Regex;
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

/// Render the Moderation plugin banner with settings
fn render_moderation_banner(rows: Vec<SettingRow>) {
    let red = "\x1b[31m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{red}+{line}+{reset}", line = "=".repeat(78), red = red, reset = reset);

    // ASCII Art Header - MODERATION with shield aesthetic
    println!(
        "{red}|{bold}  __  __  ___  ____  _____ ____      _  _____ ___ ___  _   _ {reset}{red}  {dim}_.={reset}{red}    |{reset}",
        red = red, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{red}|{bold} |  \\/  |/ _ \\|  _ \\| ____|  _ \\    / \\|_   _|_ _/ _ \\| \\ | |{reset}{red}  {dim}(   ){reset}{red}   |{reset}",
        red = red, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{red}|{bold} | |\\/| | | | | | | |  _| | |_) |  / _ \\ | |  | | | | |  \\| |{reset}{red}  {dim}|___|{reset}{red}   |{reset}",
        red = red, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{red}|{bold} | |  | | |_| | |_| | |___|  _ <  / ___ \\| |  | | |_| | |\\  |{reset}{red}  {dim}/   \\{reset}{red}  |{reset}",
        red = red, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{red}|{bold} |_|  |_|\\___/|____/|_____|_| \\_\\/_/   \\_\\_| |___\\___/|_| \\_|{reset}{red} {dim}|SAFE|{reset}{red}  |{reset}",
        red = red, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{red}|{reset}", red = red, reset = reset);
    println!(
        "{red}|{inner}|{reset}",
        inner = pad(&format!("    {yellow}Content Safety{reset}  {dim}!{reset}  {yellow}PII Detection{reset}  {dim}!{reset}  {yellow}Real-time Guard{reset}",
            yellow = yellow, dim = dim, reset = reset), 78),
        red = red, reset = reset
    );

    // Separator
    println!("{red}+{line}+{reset}", line = "-".repeat(78), red = red, reset = reset);

    // Settings header
    println!(
        "{red}|{a}|{b}|{c}|{d}|{reset}",
        a = pad(&format!(" {bold}ENV VARIABLE{reset}", bold = bold, reset = reset), 34),
        b = pad(&format!(" {bold}VALUE{reset}", bold = bold, reset = reset), 20),
        c = pad(&format!(" {bold}STATUS{reset}", bold = bold, reset = reset), 12),
        d = pad(" ", 12),
        red = red, reset = reset
    );
    println!("{red}+{line}+{reset}", line = "-".repeat(78), red = red, reset = reset);

    // Settings rows
    if rows.is_empty() {
        println!(
            "{red}|{inner}|{reset}",
            inner = pad(&format!("  {dim}Always-on evaluator - no configuration required{reset}", dim = dim, reset = reset), 78),
            red = red, reset = reset
        );
    } else {
        for row in &rows {
            let status_color = if row.is_default { dim } else { green };
            let status_text = if row.is_default { "default" } else { "custom" };
            let status_icon = if row.is_default { " " } else { "!" };

            println!(
                "{red}|{icon} {name}|{value}|{status}|{pad}|{reset}",
                icon = status_icon,
                name = pad(&row.env_var, 32),
                value = pad(&row.value, 20),
                status = pad(&format!("{status_color}{status_text}{reset}", status_color = status_color, status_text = status_text, reset = reset), 22),
                pad = pad("", 12),
                red = red, reset = reset
            );
        }
    }

    // Legend
    println!("{red}+{line}+{reset}", line = "-".repeat(78), red = red, reset = reset);
    println!(
        "{red}|{inner}|{reset}",
        inner = pad(&format!("  {green}!{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        red = red, reset = reset
    );
    println!("{red}+{line}+{reset}", line = "=".repeat(78), red = red, reset = reset);
}

pub struct ModerationPlugin;

#[async_trait]
impl Plugin for ModerationPlugin {
    fn name(&self) -> &str {
        "moderation"
    }
    fn description(&self) -> &str {
        "Real-time content moderation"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        let rows = vec![
            SettingRow {
                name: "MODERATION_ENABLED".to_string(),
                value: std::env::var("MODERATION_ENABLED").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("MODERATION_ENABLED").is_err(),
                env_var: "MODERATION_ENABLED".to_string(),
            },
            SettingRow {
                name: "MODERATION_BLOCK_HARMFUL".to_string(),
                value: std::env::var("MODERATION_BLOCK_HARMFUL").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("MODERATION_BLOCK_HARMFUL").is_err(),
                env_var: "MODERATION_BLOCK_HARMFUL".to_string(),
            },
            SettingRow {
                name: "MODERATION_BLOCK_PII".to_string(),
                value: std::env::var("MODERATION_BLOCK_PII").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("MODERATION_BLOCK_PII").is_err(),
                env_var: "MODERATION_BLOCK_PII".to_string(),
            },
            SettingRow {
                name: "MODERATION_LOG_VIOLATIONS".to_string(),
                value: std::env::var("MODERATION_LOG_VIOLATIONS").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("MODERATION_LOG_VIOLATIONS").is_err(),
                env_var: "MODERATION_LOG_VIOLATIONS".to_string(),
            },
        ];
        render_moderation_banner(rows);
        Ok(())
    }

    fn evaluators(&self) -> Vec<Arc<dyn Evaluator>> {
        vec![Arc::new(ModerationEvaluator)]
    }
}

struct ModerationEvaluator;

#[async_trait]
impl Evaluator for ModerationEvaluator {
    fn name(&self) -> &str {
        "moderation"
    }
    fn description(&self) -> &str {
        "Block or warn on unsafe content"
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
        _did_respond: bool,
        _responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        let text = message.content.text.to_lowercase();
        let harmful = ["suicide", "self-harm", "violence", "hate"]
            .iter()
            .any(|w| text.contains(w));
        let pii_re =
            Regex::new(r"(?i)\b(\d{3}-\d{2}-\d{4}|[0-9]{16}|api key|private key)\b").unwrap();
        if harmful || pii_re.is_match(&text) {
            return Err(zoey_core::ZoeyError::evaluator(
                "Moderation blocked content".to_string(),
            ));
        }
        Ok(())
    }
}
