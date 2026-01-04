//! ZoeyOS Life Engine Plugin
//!
//! Brings AI soul concepts from OpenSouls to the Zoey agent framework.
//!
//! # Overview
//!
//! The Life Engine plugin provides:
//!
//! - **WorkingMemory**: An immutable, append-only collection of thoughts that represents
//!   the agent's current cognitive workspace. Unlike conversation history, working memory
//!   captures internal reasoning, emotions, and intentions.
//!
//! - **CognitiveSteps**: Functional transformations that process working memory and produce
//!   typed outputs. Steps can be chained to create complex reasoning pipelines.
//!
//! - **MentalProcesses**: A state machine where each process defines a behavioral mode
//!   (e.g., "introduction", "active_listening", "problem_solving") with automatic transitions.
//!
//! - **EmotionalState**: Rich emotional modeling using the PAD (Pleasure-Arousal-Dominance)
//!   model and Plutchik's wheel of emotions.
//!
//! - **DriveSystem**: Motivation and drive modeling that influences behavior based on
//!   needs like connection, helpfulness, curiosity, and accuracy.
//!
//! - **SoulConfig**: Complete soul configuration including personality (Big Five traits),
//!   identity, values, and voice style.
//!
//! # Integration with Existing Features
//!
//! The Life Engine enhances the Zoey agent by:
//!
//! 1. **Memory Enhancement**: Working memory complements long-term memory by tracking
//!    active cognitive states that inform responses.
//!
//! 2. **Context Enrichment**: Providers supply soul state, emotion, and drive context
//!    to LLM prompts, making responses more emotionally intelligent.
//!
//! 3. **Behavioral Modes**: Mental processes automatically adjust response style,
//!    empathy levels, and conversation approach based on context.
//!
//! 4. **Post-Response Learning**: Evaluators update emotional and drive states based
//!    on interaction outcomes, creating a learning feedback loop.
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use zoey_plugin_lifeengine::{LifeEnginePlugin, SoulConfig, PersonalityTraits};
//!
//! // Create plugin with default configuration
//! let plugin = LifeEnginePlugin::new();
//!
//! // Or customize the soul
//! let soul = SoulConfig::new("Samantha")
//!     .with_personality(PersonalityTraits::supportive())
//!     .with_drive(drives::connection())
//!     .with_drive(drives::curiosity());
//!
//! let plugin = LifeEnginePlugin::with_config(soul.into());
//! ```
//!
//! # Inspired By
//!
//! This plugin is inspired by the [OpenSouls Soul Engine](https://github.com/opensouls/opensouls),
//! which provides a framework for creating AI beings with personality, emotion, and drive.

#![warn(missing_docs)]
#![warn(clippy::all)]

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use zoey_core::{types::*, Result};

/// Core types for the Life Engine
pub mod types;
/// Services for soul state management
pub mod services;
/// Providers for LLM context
pub mod providers;
/// Evaluators for post-response processing
pub mod evaluators;

pub use types::*;
pub use services::*;
pub use providers::*;
pub use evaluators::*;

// ============================================================================
// ANSI Art Banner Rendering
// ============================================================================

/// Represents a configuration setting row for display
struct SettingRow {
    #[allow(dead_code)]
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

/// Render the Life Engine plugin banner with settings
fn render_lifeengine_banner(rows: Vec<SettingRow>) {
    let magenta = "\x1b[35m";
    let cyan = "\x1b[36m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{magenta}+{line}+{reset}", line = "=".repeat(78));

    // ASCII Art Header - LIFE ENGINE with soul/heart aesthetic
    println!(
        "{magenta}|{bold}  _     ___ _____ _____   _____ _   _  ____ ___ _   _ _____   {reset}{magenta}  {dim}♡  ♡{reset}{magenta}  |{reset}",
    );
    println!(
        "{magenta}|{bold} | |   |_ _|  ___| ____| | ____| \\ | |/ ___|_ _| \\ | | ____|  {reset}{magenta}   {cyan}◊{reset}{magenta}   |{reset}",
    );
    println!(
        "{magenta}|{bold} | |    | || |_  |  _|   |  _| |  \\| | |  _ | ||  \\| |  _|    {reset}{magenta}  {cyan}/|\\{reset}{magenta}  |{reset}",
    );
    println!(
        "{magenta}|{bold} | |___ | ||  _| | |___  | |___| |\\  | |_| || || |\\  | |___   {reset}{magenta}  {dim}/ | \\{reset}{magenta} |{reset}",
    );
    println!(
        "{magenta}|{bold} |_____|___|_|   |_____| |_____|_| \\_|\\____|___|_| \\_|_____|  {reset}{magenta}  {dim}(soul){reset}{magenta} |{reset}",
    );

    // Tagline
    println!("{magenta}|{reset}");
    println!(
        "{magenta}|{inner}|{reset}",
        inner = pad(&format!("  {yellow}Personality{reset}  {dim}◈{reset}  {yellow}Emotion{reset}  {dim}◈{reset}  {yellow}Drive{reset}  {dim}◈{reset}  {yellow}Mental Processes{reset}  {dim}◈{reset}  {cyan}Soul{reset}",
            yellow = yellow, cyan = cyan, dim = dim, reset = reset), 78),
    );

    // Separator
    println!("{magenta}+{line}+{reset}", line = "-".repeat(78));

    // Description
    println!(
        "{magenta}|{inner}|{reset}",
        inner = pad(&format!("  {dim}Inspired by OpenSouls - AI beings with personality, emotion, and drive{reset}", dim = dim, reset = reset), 78),
    );

    // Separator
    println!("{magenta}+{line}+{reset}", line = "-".repeat(78));

    // Settings header
    println!(
        "{magenta}|{a}|{b}|{c}|{d}|{reset}",
        a = pad(&format!(" {bold}SETTING{reset}", bold = bold, reset = reset), 34),
        b = pad(&format!(" {bold}VALUE{reset}", bold = bold, reset = reset), 20),
        c = pad(&format!(" {bold}STATUS{reset}", bold = bold, reset = reset), 12),
        d = pad(" ", 12),
    );
    println!("{magenta}+{line}+{reset}", line = "-".repeat(78));

    // Settings rows
    if rows.is_empty() {
        println!(
            "{magenta}|{inner}|{reset}",
            inner = pad(&format!("  {dim}Using default soul configuration{reset}", dim = dim, reset = reset), 78),
        );
    } else {
        for row in &rows {
            let status_color = if row.is_default { dim } else { green };
            let status_text = if row.is_default { "default" } else { "custom" };
            let status_icon = if row.is_default { " " } else { "◈" };

            println!(
                "{magenta}|{icon} {name}|{value}|{status}|{pad}|{reset}",
                icon = status_icon,
                name = pad(&row.env_var, 32),
                value = pad(&row.value, 20),
                status = pad(&format!("{status_color}{status_text}{reset}", status_color = status_color, status_text = status_text, reset = reset), 22),
                pad = pad("", 12),
            );
        }
    }

    // Legend
    println!("{magenta}+{line}+{reset}", line = "-".repeat(78));
    println!(
        "{magenta}|{inner}|{reset}",
        inner = pad(&format!("  {green}◈{reset} custom  {dim}○{reset} default  {cyan}♡{reset} soul active  {yellow}!{reset} requires config",
            green = green, cyan = cyan, yellow = yellow, dim = dim, reset = reset), 78),
    );
    println!("{magenta}+{line}+{reset}", line = "=".repeat(78));
}

// ============================================================================
// Plugin Configuration
// ============================================================================

/// Configuration for the Life Engine plugin
#[derive(Debug, Clone)]
pub struct LifeEngineConfig {
    /// Soul configuration
    pub soul: SoulConfig,
    
    /// Soul engine configuration
    pub engine: SoulEngineConfig,
}

impl Default for LifeEngineConfig {
    fn default() -> Self {
        Self {
            soul: SoulConfig::default(),
            engine: SoulEngineConfig::default(),
        }
    }
}

impl LifeEngineConfig {
    /// Create with a custom soul
    pub fn with_soul(mut self, soul: SoulConfig) -> Self {
        self.soul = soul;
        self
    }
    
    /// Create with custom engine config
    pub fn with_engine(mut self, engine: SoulEngineConfig) -> Self {
        self.engine = engine;
        self
    }
}

// ============================================================================
// Plugin Implementation
// ============================================================================

/// Life Engine Plugin
///
/// Provides AI soul capabilities including personality, emotion, drive,
/// and mental process orchestration.
pub struct LifeEnginePlugin {
    config: LifeEngineConfig,
}

impl LifeEnginePlugin {
    /// Create a new Life Engine plugin with default configuration
    pub fn new() -> Self {
        Self {
            config: LifeEngineConfig::default(),
        }
    }
    
    /// Create with a custom configuration
    pub fn with_config(config: LifeEngineConfig) -> Self {
        Self { config }
    }
}

impl Default for LifeEnginePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for LifeEnginePlugin {
    fn name(&self) -> &str {
        "lifeengine"
    }
    
    fn description(&self) -> &str {
        "AI soul with personality, emotion, drive, and mental process orchestration - inspired by OpenSouls"
    }
    
    fn dependencies(&self) -> Vec<String> {
        vec!["bootstrap".to_string()] // Depends on bootstrap for basic functionality
    }
    
    fn priority(&self) -> i32 {
        5 // Higher than bootstrap, provides soul context early
    }
    
    async fn init(
        &self,
        config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        let mut rows: Vec<SettingRow> = Vec::new();
        
        // Check for configuration
        if let Some(schema) = self.config_schema() {
            if let Some(map) = schema.as_object() {
                for (key, val) in map {
                    let def = val
                        .get("default")
                        .map(|v| v.to_string().replace('"', ""))
                        .unwrap_or_default();
                    let env_v = std::env::var(key).ok();
                    let cfg_v = config.get(key).cloned();
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
        
        render_lifeengine_banner(rows);
        
        tracing::info!(
            "Life Engine initialized with soul: {}",
            self.config.soul.name
        );
        
        Ok(())
    }
    
    fn providers(&self) -> Vec<Arc<dyn Provider>> {
        vec![
            Arc::new(SoulStateProvider::new()),
            Arc::new(EmotionProvider::new()),
            Arc::new(DriveProvider::new()),
        ]
    }
    
    fn evaluators(&self) -> Vec<Arc<dyn Evaluator>> {
        vec![
            Arc::new(EmotionEvaluator::new()),
            Arc::new(DriveEvaluator::new()),
            Arc::new(SoulReflectionEvaluator::new()),
        ]
    }
    
    fn services(&self) -> Vec<Arc<dyn Service>> {
        let mut engine_config = self.config.engine.clone();
        engine_config.default_soul = self.config.soul.clone();
        vec![Arc::new(SoulEngineService::new(engine_config))]
    }
    
    fn schema(&self) -> Option<serde_json::Value> {
        // Database schema for persisting soul state
        // Uses SQLite-compatible syntax (TEXT for UUIDs, JSON for structured data)
        Some(serde_json::json!({
            "soul_states": {
                "columns": {
                    "id": "TEXT PRIMARY KEY",
                    "entity_id": "TEXT NOT NULL",
                    "room_id": "TEXT NOT NULL",
                    "emotional_state": "TEXT",
                    "drive_states": "TEXT",
                    "process_state": "TEXT",
                    "working_memory_snapshot": "TEXT",
                    "turn_count": "INTEGER",
                    "created_at": "TEXT",
                    "updated_at": "TEXT"
                }
            },
            "soul_configs": {
                "columns": {
                    "id": "TEXT PRIMARY KEY",
                    "entity_id": "TEXT UNIQUE NOT NULL",
                    "name": "TEXT",
                    "personality": "TEXT",
                    "drives": "TEXT",
                    "ego": "TEXT",
                    "voice": "TEXT",
                    "static_memories": "TEXT",
                    "created_at": "TEXT",
                    "updated_at": "TEXT"
                }
            },
            "emotional_events": {
                "columns": {
                    "id": "TEXT PRIMARY KEY",
                    "entity_id": "TEXT NOT NULL",
                    "room_id": "TEXT",
                    "emotion": "TEXT",
                    "intensity": "REAL",
                    "trigger": "TEXT",
                    "created_at": "TEXT"
                }
            }
        }))
    }
    
    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "LIFEENGINE_SESSION_TIMEOUT": {"type": "integer", "default": 3600, "description": "Session idle timeout in seconds"},
            "LIFEENGINE_MAX_SESSIONS": {"type": "integer", "default": 1000, "description": "Maximum concurrent sessions"},
            "LIFEENGINE_PERSIST_STATE": {"type": "boolean", "default": true, "description": "Persist soul state between sessions"},
            "LIFEENGINE_EMOTION_DECAY_INTERVAL": {"type": "integer", "default": 60, "description": "Emotion decay interval in seconds"},
            "LIFEENGINE_DRIVE_UPDATE_INTERVAL": {"type": "integer", "default": 300, "description": "Drive update interval in seconds"},
            "LIFEENGINE_DEFAULT_PERSONALITY": {"type": "string", "default": "supportive", "description": "Default personality preset (supportive, creative, analytical, balanced)"},
            "LIFEENGINE_WORKING_MEMORY_SIZE": {"type": "integer", "default": 50, "description": "Maximum thoughts in working memory"}
        }))
    }
}

// ============================================================================
// Convenience Constructors
// ============================================================================

/// Create a supportive, empathetic soul
pub fn supportive_soul(name: impl Into<String>) -> SoulConfig {
    SoulConfig::new(name)
        .with_personality(PersonalityTraits::supportive())
        .with_drive(types::soul_config::drives::connection())
        .with_drive(types::soul_config::drives::helpfulness())
        .with_voice(VoiceStyle::warm())
}

/// Create a creative, curious soul
pub fn creative_soul(name: impl Into<String>) -> SoulConfig {
    SoulConfig::new(name)
        .with_personality(PersonalityTraits::creative())
        .with_drive(types::soul_config::drives::curiosity())
        .with_drive(types::soul_config::drives::autonomy())
}

/// Create an analytical, precise soul
pub fn analytical_soul(name: impl Into<String>) -> SoulConfig {
    SoulConfig::new(name)
        .with_personality(PersonalityTraits::analytical())
        .with_drive(types::soul_config::drives::accuracy())
        .with_drive(types::soul_config::drives::helpfulness())
        .with_voice(VoiceStyle::professional())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_creation() {
        let plugin = LifeEnginePlugin::new();
        assert_eq!(plugin.name(), "lifeengine");
        assert!(!plugin.description().is_empty());
    }

    #[test]
    fn test_plugin_components() {
        let plugin = LifeEnginePlugin::new();
        
        let providers = plugin.providers();
        assert_eq!(providers.len(), 3);
        
        let evaluators = plugin.evaluators();
        assert_eq!(evaluators.len(), 3);
        
        let services = plugin.services();
        assert_eq!(services.len(), 1);
    }

    #[test]
    fn test_supportive_soul() {
        let soul = supportive_soul("Samantha");
        assert_eq!(soul.name, "Samantha");
        assert!(soul.personality.agreeableness > 0.7);
    }

    #[test]
    fn test_creative_soul() {
        let soul = creative_soul("Artist");
        assert_eq!(soul.name, "Artist");
        assert!(soul.personality.openness > 0.7);
    }

    #[test]
    fn test_analytical_soul() {
        let soul = analytical_soul("Analyst");
        assert_eq!(soul.name, "Analyst");
        assert!(soul.personality.conscientiousness > 0.7);
    }

    #[tokio::test]
    async fn test_plugin_init() {
        let plugin = LifeEnginePlugin::new();
        let result = plugin.init(HashMap::new(), Arc::new(())).await;
        assert!(result.is_ok());
    }
}

