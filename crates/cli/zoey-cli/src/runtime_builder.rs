//! Builds an AgentRuntime with the appropriate plugins and providers
//! based on CLI flags and environment.

use std::sync::{Arc, RwLock};

use zoey_core::types::agent::Character;
use zoey_core::{AgentRuntime, RuntimeOpts};
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_observability::ExplainabilityPlugin;
use zoey_plugin_knowledge::KnowledgePlugin;
use zoey_provider_local::LocalLLMPlugin;
use zoey_provider_openai::OpenAIPlugin;
use zoey_provider_anthropic::AnthropicPlugin;
use zoey_storage_sql::SqliteAdapter;
use zoey_storage_vector::LocalVectorPlugin;

/// Build an AgentRuntime configured from CLI args and environment.
pub async fn build(
    character_path: &str,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> anyhow::Result<Arc<RwLock<AgentRuntime>>> {
    // Load character
    let character = zoey_core::load_character_from_xml(character_path)
        .unwrap_or_else(|_| Character {
            name: "Zoey".to_string(),
            bio: vec!["Zoey: a helpful, privacy-first AI assistant.".to_string()],
            knowledge: vec![],
            ..Default::default()
        });

    // Assemble plugins based on available API keys and config
    let mut plugins: Vec<Arc<dyn zoey_core::Plugin>> = Vec::new();

    // Always load bootstrap
    plugins.push(Arc::new(BootstrapPlugin::new()));

    // Observability
    plugins.push(Arc::new(ExplainabilityPlugin::new()));

    // Knowledge (if configured)
    plugins.push(Arc::new(KnowledgePlugin::new()));

    // Providers — load all available, runtime will pick based on model_provider
    if std::env::var("OPENAI_API_KEY").is_ok() {
        plugins.push(Arc::new(OpenAIPlugin::new()));
    }
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        plugins.push(Arc::new(AnthropicPlugin::new()));
    }
    // Always load local (Ollama)
    plugins.push(Arc::new(LocalLLMPlugin::new()));

    // Vector storage
    if let Ok(p) = LocalVectorPlugin::new("./data/vectors") {
        plugins.push(Arc::new(p));
    }

    // Build opts
    let mut opts = RuntimeOpts::new()
        .with_character(character.clone())
        .with_plugins(plugins);

    // Database adapter
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());
    if let Ok(sqlite) = SqliteAdapter::new(&db_url).await {
        opts = opts.with_adapter(Arc::new(sqlite));
    }

    let runtime = AgentRuntime::new(opts).await?;

    // Apply overrides
    {
        let mut rt = runtime.write().unwrap();

        // Provider override
        if let Some(provider) = provider_override {
            rt.set_setting("model_provider", serde_json::json!(provider), false);
            rt.set_setting("MODEL_PROVIDER", serde_json::json!(provider), false);
        } else if let Some(ref provider) = character.model_provider {
            rt.set_setting("model_provider", serde_json::json!(provider), false);
            rt.set_setting("MODEL_PROVIDER", serde_json::json!(provider), false);
        }

        // Model override
        if let Some(model) = model_override {
            rt.set_setting("LOCAL_LLM_MODEL", serde_json::json!(model), false);
            rt.set_setting("OLLAMA_MODEL", serde_json::json!(model), false);
        }

        // Transfer character settings
        for (key, value) in character.settings.iter() {
            rt.set_setting(key, value.clone(), false);
        }

        // Env vars
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            rt.set_setting("OPENAI_API_KEY", serde_json::json!(key), false);
        }
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            rt.set_setting("ANTHROPIC_API_KEY", serde_json::json!(key), false);
        }
        if let Ok(model) = std::env::var("OLLAMA_MODEL") {
            rt.set_setting("LOCAL_LLM_MODEL", serde_json::json!(model), false);
        }
        if let Ok(base) = std::env::var("OLLAMA_BASE_URL") {
            rt.set_setting("LOCAL_LLM_ENDPOINT", serde_json::json!(base), false);
        }
    }

    Ok(runtime)
}
