use zoey_core::observability::{set_global_cost_tracker, CostTracker, ObservabilityPlugin};
use zoey_core::IDatabaseAdapter;
use zoey_core::{
    agent_api::{AgentApiConfig, AgentApiServer},
    types::{InitializeOptions, StorageType},
    AgentRuntime, RuntimeOpts,
};
use zoey_ext_workflow::WorkflowPlugin;
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_knowledge::KnowledgePlugin;
use zoey_plugin_observability::ExplainabilityPlugin;
use zoey_plugin_x402_video::X402VideoPlugin;
use zoey_plugin_lifeengine::LifeEnginePlugin;
use zoey_provider_anthropic::AnthropicPlugin;
use zoey_provider_local::LocalLLMPlugin;
use zoey_provider_openai::OpenAIPlugin;
use zoey_storage_sql::{PostgresAdapter, SqliteAdapter};
use zoey_storage_mongo::MongoAdapter;
use zoey_storage_supabase::SupabaseAdapter;
use zoey_storage_vector::LocalVectorPlugin;

use std::collections::HashMap;
use std::sync::Arc;
// duplicate import removed
// use std::fs;
use clap::Parser;
use dotenvy::dotenv;
use zoey_adaptor_discord::{start_discord, DiscordConfig};
use zoey_adaptor_telegram::{start_telegram, TelegramConfig};
use zoey_adaptor_terminal::{TerminalAdaptor, TerminalConfig};
use zoey_adaptor_web::{SimpleUiConfig, SimpleUiServer};
use zoey_core::observability::{start_rest_api, RestApiConfig};
use zoey_core::types::agent::Character;
use zoey_core::utils::logger::init_logging;
use serde_json::json;
use serenity::model::gateway::GatewayIntents;

fn env_bool(key: &str) -> Option<bool> {
    std::env::var(key)
        .ok()
        .and_then(|s| match s.to_lowercase().as_str() {
            "true" | "1" | "yes" | "y" | "on" => Some(true),
            "false" | "0" | "no" | "n" | "off" => Some(false),
            _ => None,
        })
}


#[derive(Parser, Debug)]
struct Cli {
    #[arg(long, env = "ZOEY_LOG_LEVEL", default_value = "info")]
    log_level: String,
}

fn main() -> zoey_core::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    rt.block_on(async move {
    let cli = Cli::parse();
    std::env::set_var("RUST_LOG", &cli.log_level);
    std::env::set_var("ZOEY_LOG_LEVEL", &cli.log_level);
    init_logging();
    let _ = dotenv();
    // Ensure observability REST and logs are enabled before plugins initialize
    // Only set defaults if not already configured (allows multi-agent support)
    if std::env::var("OBSERVABILITY_ENABLED").is_err() { std::env::set_var("OBSERVABILITY_ENABLED", "true"); }
    if std::env::var("OBSERVABILITY_COST_TRACKING_ENABLED").is_err() { std::env::set_var("OBSERVABILITY_COST_TRACKING_ENABLED", "true"); }
    if std::env::var("OBSERVABILITY_REST_API_ENABLED").is_err() { std::env::set_var("OBSERVABILITY_REST_API_ENABLED", "true"); }
    if std::env::var("OBSERVABILITY_REST_API_HOST").is_err() { std::env::set_var("OBSERVABILITY_REST_API_HOST", "127.0.0.1"); }
    if std::env::var("OBSERVABILITY_REST_API_PORT").is_err() { std::env::set_var("OBSERVABILITY_REST_API_PORT", "9100"); }
    if std::env::var("AGENT_LOGS_ENABLED").is_err() { std::env::set_var("AGENT_LOGS_ENABLED", "true"); }
    // Create a baseline runtime with ZoeyAI character and Bootstrap plugin
    let character_path = std::env::var("CHARACTER_FILE").unwrap_or_else(|_| "characters/zoey-agent-sql.xml".to_string());
    let character = zoey_core::load_character_from_xml(&character_path).unwrap_or_else(|_| Character {
        name: "ZoeyAI".to_string(),
        bio: vec![
            "ZoeyAI: adaptable assistant with real-time reasoning and reflective evaluation".to_string(),
            "Grounded by a knowledge system and workflows; can explain steps and improve plans".to_string(),
        ],
        knowledge: vec![
            "Understands common CS concepts, ML basics, and Rust ecosystem patterns".to_string(),
            "Learns via feedback loops and curated ingestion, not unsupervised chat memory".to_string(),
        ],
        ..Default::default()
    });

    let (plugins, adapter_opt, normalized_plugins) = build_plugins_and_adapter(&character).await;
    let plugins_for_init = plugins.clone();

    // DB initialization must be performed by plugins; runner performs no DB writes here

    let mut opts = RuntimeOpts::new()
        .with_character(character.clone())
        .with_plugins(plugins);
    if let Some(adapter) = adapter_opt { opts = opts.with_adapter(adapter); }

    let runtime = AgentRuntime::new(opts).await?;

    // Character loader already stores settings; ensure uppercase keys too for model selection
    {
        let mut rt = runtime.write().unwrap();
        rt.set_setting("plugins", serde_json::json!(normalized_plugins), false);
        rt.set_setting("ui:provider_racing", serde_json::json!(false), false);
        rt.set_setting("ui:streaming", serde_json::json!(true), false);
        rt.set_setting("clients", serde_json::json!(character.clients.clone()), false);
        rt.set_setting("ui:fast_mode", serde_json::json!(true), false);
        rt.set_setting("ui:streaming", serde_json::json!(true), false);
        // Transfer character settings to runtime settings (XML settings -> runtime)
        for (key, value) in character.settings.iter() {
            rt.set_setting(key, value.clone(), false);
        }
        // Explicitly set model_provider from character struct
        if let Some(ref provider) = character.model_provider {
            rt.set_setting("model_provider", json!(provider), false);
            rt.set_setting("MODEL_PROVIDER", json!(provider), false);
        }
        if let Some(provider) = rt
            .get_setting("model_provider")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            rt.set_setting("MODEL_PROVIDER", json!(provider), false);
        }
        let model_opt = rt
            .get_setting("OPENAI_MODEL")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .or_else(|| rt.get_setting("openai_model").and_then(|v| v.as_str().map(|s| s.to_string())));
        if let Some(model) = model_opt {
            rt.set_setting("OPENAI_MODEL", json!(model), false);
        }

        if let Ok(openai_key) = std::env::var("OPENAI_API_KEY") {
            rt.set_setting("OPENAI_API_KEY", json!(openai_key), false);
            let character_mut = &mut rt.character;
            let _ = zoey_core::secrets::load_secret_from_env(character_mut, "OPENAI_API_KEY", "OPENAI_API_KEY");
        }
        if let Ok(anthropic_key) = std::env::var("ANTHROPIC_API_KEY") {
            rt.set_setting("ANTHROPIC_API_KEY", json!(anthropic_key), false);
            let character_mut = &mut rt.character;
            let _ = zoey_core::secrets::load_secret_from_env(character_mut, "ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY");
        }
        if let Ok(local_llm_max) = std::env::var("LOCAL_LLM_MAX_TOKENS") {
            if let Ok(n) = local_llm_max.parse::<usize>() {
                rt.set_setting("LOCAL_LLM_MAX_TOKENS", json!(n), false);
            }
        }
        if let Ok(ollama_model) = std::env::var("OLLAMA_MODEL") {
            rt.set_setting("LOCAL_LLM_MODEL", json!(ollama_model), false);
        }
        if let Ok(ollama_base) = std::env::var("OLLAMA_BASE_URL") {
            rt.set_setting("LOCAL_LLM_ENDPOINT", json!(ollama_base), false);
        }
        if let Some(tokens) = rt.get_setting("max_tokens").and_then(|v| v.as_u64()) {
            rt.set_setting("MAX_TOKENS", json!(tokens as usize), false);
        }
    }

    // Initialize runtime (migrations, adapter checks) and enable Observability REST
    {
        let mut rt = runtime.write().unwrap();
        rt.initialize(InitializeOptions::default()).await?;
        // Respect existing environment; only set defaults if not already configured
        if std::env::var("OBSERVABILITY_REST_API_ENABLED").is_err() {
            std::env::set_var("OBSERVABILITY_REST_API_ENABLED", "true");
        }
        if std::env::var("OBSERVABILITY_REST_API_HOST").is_err() {
            std::env::set_var("OBSERVABILITY_REST_API_HOST", "127.0.0.1");
        }
        if std::env::var("OBSERVABILITY_REST_API_PORT").is_err() {
            std::env::set_var("OBSERVABILITY_REST_API_PORT", "9100");
        }
    }

    // Initialize plugin `init` hooks for non-dashboard plugins
    {
        let rt_any: std::sync::Arc<dyn std::any::Any + Send + Sync> = runtime.clone();
        let filtered: Vec<std::sync::Arc<dyn zoey_core::Plugin>> = plugins_for_init
            .into_iter()
            .filter(|p| p.name() != "observability-dashboard")
            .collect();
        for plugin in filtered.iter() {
            println!("[runner] plugin:init:start name={}", plugin.name());
            let start = std::time::Instant::now();
            let res = plugin.init(HashMap::new(), rt_any.clone()).await;
            let dur = start.elapsed().as_millis();
            match res {
                Ok(()) => println!("[runner] plugin:init:ok name={} duration_ms={}", plugin.name(), dur),
                Err(e) => println!("[runner] plugin:init:err name={} duration_ms={} error={}", plugin.name(), dur, e),
            }
        }
    }

    // Initialize and start services (workaround for Arc limitation)
    // Note: Services are stored as Arc, so we can't call &mut methods directly
    // This is a placeholder - services should handle their own initialization/startup
    // via interior mutability or lazy initialization
    {
        let rt = runtime.read().unwrap();
        let services_count = rt.get_services_count();
        if services_count > 0 {
            println!("[runner] Registered {} service(s) (services should initialize/start themselves)", services_count);
        }
    }

    // Print runtime summary: providers, actions, evaluators, services
    {
        let rt = runtime.read().unwrap();
        let providers = rt.get_providers();
        let actions = rt.get_actions();
        let evaluators = rt.get_evaluators();
        let services_count = rt.get_services_count();
        
        let cyan = "\x1b[36m";
        let green = "\x1b[32m";
        let yellow = "\x1b[33m";
        let bold = "\x1b[1m";
        let dim = "\x1b[2m";
        let reset = "\x1b[0m";
        
        println!("{cyan}+==============================================================================+{reset}");
        println!("{cyan}|{bold}  ____  _   _ _   _ _____ ___ __  __ _____   ____  _   _ __  __ __  __    {reset}{cyan}|{reset}");
        println!("{cyan}|{bold} |  _ \\| | | | \\ | |_   _|_ _|  \\/  | ____| / ___|| | | |  \\/  |  \\/  |   {reset}{cyan}|{reset}");
        println!("{cyan}|{bold} | |_) | | | |  \\| | | |  | || |\\/| |  _|   \\___ \\| | | | |\\/| | |\\/| |   {reset}{cyan}|{reset}");
        println!("{cyan}|{bold} |  _ <| |_| | |\\  | | |  | || |  | | |___   ___) | |_| | |  | | |  | |   {reset}{cyan}|{reset}");
        println!("{cyan}|{bold} |_| \\_\\\\___/|_| \\_| |_| |___|_|  |_|_____| |____/ \\___/|_|  |_|_|  |_|   {reset}{cyan}|{reset}");
        println!("{cyan}|{reset}");
        println!("{cyan}+------------------------------------------------------------------------------+{reset}");
        println!("{cyan}|  {green}Providers ({}):{reset}", providers.len());
        for p in &providers {
            println!("{cyan}|    {dim}•{reset} {}", p.name());
        }
        println!("{cyan}|{reset}");
        println!("{cyan}|  {yellow}Actions ({}):{reset}", actions.len());
        for a in &actions {
            println!("{cyan}|    {dim}•{reset} {} {dim}- {}{reset}", a.name(), a.description());
        }
        println!("{cyan}|{reset}");
        println!("{cyan}|  {bold}Evaluators ({}):{reset}", evaluators.len());
        for e in &evaluators {
            println!("{cyan}|    {dim}•{reset} {}", e.name());
        }
        println!("{cyan}|{reset}");
        println!("{cyan}|  Services: {}{reset}", services_count);
        println!("{cyan}+==============================================================================+{reset}");
    }

    // Create shared CostTracker and start Observability REST API on port 9100
    let cost_tracker: Arc<CostTracker> = {
        let adapter_for_cost = { let rt = runtime.read().unwrap(); rt.get_adapter() };
        Arc::new(CostTracker::new(adapter_for_cost))
    };
    // Store in global for handler access
    set_global_cost_tracker(cost_tracker.clone());
    {
        let host = std::env::var("OBSERVABILITY_REST_API_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port: u16 = std::env::var("OBSERVABILITY_REST_API_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9100);
        
        // Check if port is already in use
        let in_use = std::net::TcpStream::connect(format!("{}:{}", host, port)).is_ok();
        if !in_use {
            let cfg = RestApiConfig::from_env();
            let tracker = cost_tracker.clone();
            println!("[runner] Starting Observability REST API on {}:{}", cfg.host, cfg.port);
            tokio::spawn(async move {
                if let Err(e) = start_rest_api(cfg, Some(tracker)).await {
                    eprintln!("[runner] Observability REST API error: {}", e);
                }
            });
        } else {
            println!("[runner] Observability REST API port {} already in use", port);
        }
    }

    if std::env::var("OBSERVABILITY_DASHBOARD_ENABLED").ok().and_then(|s| s.parse::<bool>().ok()).unwrap_or(false) {
        println!("[runner] Observability dashboard is an enterprise feature.");
        println!("[runner] Use REST API at http://127.0.0.1:9100 for metrics (consumer).");
    }

    // Start Agent API server
    let agent_host = std::env::var("AGENT_API_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let agent_port_chosen = {
        let preferred = std::env::var("AGENT_API_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(9090);
        let mut port = preferred;
        let limit = 200u16;
        let mut tried = 0u16;
        loop {
            match std::net::TcpListener::bind((agent_host.as_str(), port)) {
                Ok(l) => { drop(l); break port; }
                Err(_) => {
                    tried += 1;
                    if tried >= limit { break preferred; }
                    port = port.saturating_add(1);
                }
            }
        }
    };
    let mut api = AgentApiServer::new(AgentApiConfig { host: agent_host.clone(), port: agent_port_chosen, ..Default::default() }, runtime.clone());
    api.start().await?;

    // Start Simple UI server
    let logs_enabled = env_bool("UI_LOGS_ENABLED")
        .or_else(|| env_bool("SIMPLEUI_LOGS_ENABLED"))
        .or_else(|| env_bool("LOGS"))
        .unwrap_or(false);
    let ui_port_pref = std::env::var("SIMPLE_UI_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4000);
    let ui_host = std::env::var("SIMPLE_UI_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let ui_port = {
        let mut port = ui_port_pref;
        let mut tried = 0u16;
        let limit = 200u16;
        loop {
            match std::net::TcpListener::bind((ui_host.as_str(), port)) {
                Ok(l) => { drop(l); break port; }
                Err(_) => {
                    tried = tried.saturating_add(1);
                    if tried >= limit { break ui_port_pref; }
                    port = port.saturating_add(1);
                }
            }
        }
    };
    // Enable streaming by default for near-real-time feedback
    let streaming_enabled = { let rt = runtime.read().unwrap(); rt.get_setting("ui:streaming").and_then(|v| v.as_bool()).unwrap_or(true) };
    // For internal proxy, always use 127.0.0.1 to reach the local API server
    let api_base = format!("http://127.0.0.1:{}/agent", agent_port_chosen);
    let ui = SimpleUiServer::new(SimpleUiConfig {
        enabled: true,
        host: ui_host.clone(),
        port: ui_port,
        agent_api_url: api_base,
        use_streaming: streaming_enabled,
        token: None,
        logs_enabled,
    }, runtime.clone());
    ui.start().await?;

    println!("Agent API: http://{}:{}/agent\nSimple UI: http://{}:{}/", agent_host, agent_port_chosen, ui_host, ui_port);

    // DB initialization and observability are handled by plugins; no DB writes in the runner

    // Auto-start enabled adapters from character settings
    {
        let clients_list = { let rt = runtime.read().unwrap(); rt.character.clients.clone() };
        let wants_discord = clients_list.iter().any(|c| c.eq_ignore_ascii_case("discord"));
        let enable_discord = env_bool("DISCORD_ENABLED").unwrap_or(wants_discord);
        if enable_discord {
            let token = std::env::var("DISCORD_TOKEN").ok()
                .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string("DISCORD_TOKEN") });
            let app_id = std::env::var("DISCORD_APPLICATION_ID").ok()
                .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string("DISCORD_APPLICATION_ID") })
                .and_then(|s| s.parse::<u64>().ok());
            if let Some(token) = token {
                let parse_list = |key: &str| -> Option<Vec<u64>> {
                    std::env::var(key).ok()
                        .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string(key) })
                        .map(|s| s.split(',').filter_map(|x| x.trim().parse::<u64>().ok()).collect::<Vec<_>>())
                        .filter(|v| !v.is_empty())
                };
                let mut allowed_guilds = parse_list("DISCORD_ALLOWED_GUILDS");
                // Also support single DISCORD_GUILD_ID
                if let Some(single_guild) = std::env::var("DISCORD_GUILD_ID").ok()
                    .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string("DISCORD_GUILD_ID") })
                    .and_then(|s| s.trim().parse::<u64>().ok()) {
                    match &mut allowed_guilds {
                        Some(v) => { if !v.contains(&single_guild) { v.push(single_guild); } }
                        None => { allowed_guilds = Some(vec![single_guild]); }
                    }
                }
                let mut allowed_channels = parse_list("DISCORD_ALLOWED_CHANNELS");
                if let Some(single) = std::env::var("DISCORD_CHANNEL_ID").ok()
                    .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string("DISCORD_CHANNEL_ID") })
                    .and_then(|s| s.trim().parse::<u64>().ok()) {
                    match &mut allowed_channels {
                        Some(v) => { if !v.contains(&single) { v.push(single); } }
                        None => { allowed_channels = Some(vec![single]); }
                    }
                }
                let allowed_users = parse_list("DISCORD_ALLOWED_USERS");
                // Load voice config from character settings
                let voice_config = {
                    let rt = runtime.read().unwrap();
                    let settings = serde_json::to_value(&rt.character.settings).unwrap_or_default();
                    zoey_adaptor_discord::VoiceConfig::from_character_settings(&settings)
                };
                
                // Auto-start Moshi server if engine is "moshi"
                if voice_config.enabled && voice_config.engine == "moshi" {
                    println!("[runner] Voice engine is 'moshi' - starting Moshi server...");
                    let moshi_endpoint = voice_config.local_endpoint.clone()
                        .or_else(|| voice_config.stt_endpoint.clone())
                        .unwrap_or_else(|| "localhost:8998".to_string());
                    
                    // Extract host and port from endpoint
                    let endpoint_clean = moshi_endpoint
                        .trim_start_matches("wss://")
                        .trim_start_matches("ws://")
                        .trim_start_matches("https://")
                        .trim_start_matches("http://");
                    let parts: Vec<&str> = endpoint_clean.split(':').collect();
                    let host_str = parts.first().unwrap_or(&"127.0.0.1");
                    // Convert "localhost" to IP address for socket binding
                    let host = if *host_str == "localhost" { "127.0.0.1" } else { host_str }.to_string();
                    let port: u16 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(8998);
                    
                    // Start Moshi server in background
                    let bind_addr = format!("{}:{}", host, port);
                    tokio::spawn(async move {
                        println!("[runner] Starting Moshi server on {}", bind_addr);
                        // Use zoey-provider-voice's Moshi server
                        use zoey_provider_voice::MoshiServerBuilder;
                        match MoshiServerBuilder::new()
                            .bind(&bind_addr)
                            .build()
                            .await
                        {
                            Ok(mut server) => {
                                println!("[runner] Moshi server starting on {}", bind_addr);
                                if let Err(e) = server.start().await {
                                    eprintln!("[runner] Moshi server error: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("[runner] Failed to build Moshi server: {}", e);
                                eprintln!("[runner] Hint: Ensure Moshi models are downloaded. Run:");
                                eprintln!("[runner]   huggingface-cli download kyutai/moshiko-pytorch-bf16");
                            }
                        }
                    });
                    
                    // Give server time to start
                    println!("[runner] Waiting 2s for Moshi server to initialize...");
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    println!("[runner] Moshi server wait complete, continuing to Discord...");
                }
                
                println!("[runner] Setting up Discord config...");
                let config = DiscordConfig {
                    enabled: true,
                    token,
                    intents: GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES | GatewayIntents::DIRECT_MESSAGES | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILD_VOICE_STATES,
                    application_id: app_id,
                    allowed_guilds,
                    allowed_channels,
                    allowed_users,
                    voice: voice_config,
                };
                println!("[runner] Starting Discord adapter...");
                let _ = start_discord(runtime.clone(), config).await;
                println!("[runner] Discord adapter started");
            }
        }
    }

    // Telegram adaptor (feature: zoey-adaptor-telegram)
    {
        let clients_list = { let rt = runtime.read().unwrap(); rt.character.clients.clone() };
        let wants_telegram = clients_list.iter().any(|c| c.eq_ignore_ascii_case("telegram"));
        let enable_telegram = env_bool("TELEGRAM_ENABLED").unwrap_or(false);
        if enable_telegram {
            let token = std::env::var("TELEGRAM_BOT_TOKEN").ok()
                .or_else(|| std::env::var("TELEGRAM_APPLICATION_ID").ok()) // alias for compatibility
                .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string("TELEGRAM_BOT_TOKEN") });
            if let Some(token) = token {
                let parse_i64_list = |key: &str| -> Option<Vec<i64>> {
                    std::env::var(key).ok()
                        .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string(key) })
                        .map(|s| s.split(',').filter_map(|x| x.trim().parse::<i64>().ok()).collect::<Vec<_>>())
                        .filter(|v| !v.is_empty())
                };
                let parse_u64_list = |key: &str| -> Option<Vec<u64>> {
                    std::env::var(key).ok()
                        .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string(key) })
                        .map(|s| s.split(',').filter_map(|x| x.trim().parse::<u64>().ok()).collect::<Vec<_>>())
                        .filter(|v| !v.is_empty())
                };
                let mut allowed_chats = parse_i64_list("TELEGRAM_ALLOWED_CHATS");
                if let Some(single) = std::env::var("TELEGRAM_CHAT_ID").ok()
                    .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string("TELEGRAM_CHAT_ID") })
                    .and_then(|s| s.trim().parse::<i64>().ok()) {
                    match &mut allowed_chats {
                        Some(v) => { if !v.contains(&single) { v.push(single); } }
                        None => { allowed_chats = Some(vec![single]); }
                    }
                }
                let allowed_users = parse_u64_list("TELEGRAM_ALLOWED_USERS");
                let bot_username = std::env::var("TELEGRAM_BOT_USERNAME").ok()
                    .or_else(|| { let rt = runtime.read().unwrap(); rt.get_setting_string("TELEGRAM_BOT_USERNAME") });
                let mut voice_config = {
                    let rt = runtime.read().unwrap();
                    let settings = serde_json::to_value(&rt.character.settings).unwrap_or_default();
                    zoey_adaptor_telegram::VoiceConfig::from_character_settings(&settings)
                };
                if let Some(v) = std::env::var("TELEGRAM_VOICE_ENABLED").ok().and_then(|s| s.parse::<bool>().ok()) { voice_config.enabled = v; }
                if let Ok(engine) = std::env::var("TELEGRAM_VOICE_ENGINE") { voice_config.engine = engine; }
                if let Ok(model) = std::env::var("TELEGRAM_VOICE_MODEL") { voice_config.model = model; }
                if let Ok(voice_id) = std::env::var("TELEGRAM_VOICE_ID") { voice_config.voice_id = voice_id; }
                if let Ok(name) = std::env::var("TELEGRAM_VOICE_NAME") { voice_config.voice_name = name; }
                if let Some(speed) = std::env::var("TELEGRAM_VOICE_SPEED").ok().and_then(|s| s.parse::<f32>().ok()) { voice_config.speed = speed; }
                if let Ok(fmt) = std::env::var("TELEGRAM_VOICE_FORMAT") { voice_config.output_format = fmt; }
                if let Some(sr) = std::env::var("TELEGRAM_VOICE_SAMPLE_RATE").ok().and_then(|s| s.parse::<u32>().ok()) { voice_config.sample_rate = sr; }
                if let Some(stream) = std::env::var("TELEGRAM_VOICE_STREAMING").ok().and_then(|s| s.parse::<bool>().ok()) { voice_config.streaming = stream; }
                if let Ok(endpoint) = std::env::var("TELEGRAM_VOICE_LOCAL_ENDPOINT") { voice_config.local_endpoint = Some(endpoint); }
                if let Some(auto) = std::env::var("TELEGRAM_AUTO_VOICE").ok().and_then(|s| s.parse::<bool>().ok()) { voice_config.telegram.auto_voice = auto; }
                if let Some(maxlen) = std::env::var("TELEGRAM_VOICE_MAX_TEXT").ok().and_then(|s| s.parse::<usize>().ok()) { voice_config.telegram.max_text_length = maxlen; }
                if let Some(include) = std::env::var("TELEGRAM_VOICE_INCLUDE_TEXT").ok().and_then(|s| s.parse::<bool>().ok()) { voice_config.telegram.include_text = include; }
                if let Some(stt) = std::env::var("TELEGRAM_TRANSCRIBE_VOICE").ok().and_then(|s| s.parse::<bool>().ok()) { voice_config.telegram.transcribe_voice = stt; }
                let config = TelegramConfig {
                    enabled: true,
                    token,
                    allowed_chats,
                    allowed_users,
                    bot_username,
                    voice: voice_config,
                };
                let _ = start_telegram(runtime.clone(), config).await;
            }
        } else if wants_telegram {
            eprintln!("Telegram requested in character clients but TELEGRAM_ENABLED is not set; skipping Telegram adapter");
        }
    }

    // Terminal logging adaptor (feature: zoey-adaptor-terminal)
    {
        let clients_list = { let rt = runtime.read().unwrap(); rt.character.clients.clone() };
        let wants_terminal = clients_list.iter().any(|c| c.eq_ignore_ascii_case("terminal") || c.eq_ignore_ascii_case("zoey-adaptor-terminal"));
        let enable_terminal = env_bool("TERMINAL").or_else(|| env_bool("ZOEY_ADAPTOR_TERMINAL")).unwrap_or(wants_terminal);
        let target = std::env::var("TERMINAL_TARGET").ok()
            .or_else(|| std::env::var("ZOEY_TERMINAL_TARGET").ok())
            .unwrap_or_else(|| "zoey".to_string());
        if enable_terminal {
            let term = TerminalAdaptor::new(TerminalConfig { enabled: true, target_filter: Some(target) }, runtime.clone());
            term.start().await?;
        }
    }

    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = async {
            if let Some(ref mut s) = term { s.recv().await; }
        } => {},
    }
    zoey_adaptor_telegram::shutdown_telegram();
    api.stop().await?;
    Ok(())
    })
}

async fn build_plugins_and_adapter(
    character: &Character,
) -> (
    Vec<Arc<dyn zoey_core::Plugin>>,
    Option<Arc<dyn zoey_core::IDatabaseAdapter + Send + Sync>>,
    Vec<String>,
) {
    let mut plugins: Vec<Arc<dyn zoey_core::Plugin>> = Vec::new();
    let mut normalized: Vec<String> = Vec::new();
    let mut adapter: Option<Arc<dyn zoey_core::IDatabaseAdapter + Send + Sync>> = None;
    let (token_map, plugin_cfg) = load_plugin_registry();

    let names = character.plugins.clone();
    let mut has_bootstrap = false;

    for raw in names {
        let name = raw.trim().to_lowercase();
        let normalized_id = token_map
            .get(&name)
            .cloned()
            .unwrap_or_else(|| name.clone());
        let mapped = match normalized_id.as_str() {
            "zoey-plugin-bootstrap" | "bootstrap" => {
                has_bootstrap = true;
                normalized.push("bootstrap".to_string());
                Some(Arc::new(BootstrapPlugin::new()) as Arc<dyn zoey_core::Plugin>)
            }
            "zoey-plugin-explainability" | "explainability" => {
                normalized.push("explainability".to_string());
                Some(Arc::new(ExplainabilityPlugin::new()) as Arc<dyn zoey_core::Plugin>)
            }
            "zoey-plugin-workflow" | "workflow" => {
                normalized.push("workflow".to_string());
                Some(Arc::new(WorkflowPlugin::new()) as Arc<dyn zoey_core::Plugin>)
            }
            "zoey-plugin-knowledge" | "knowledge" => {
                normalized.push("knowledge".to_string());
                Some(Arc::new(KnowledgePlugin::new()) as Arc<dyn zoey_core::Plugin>)
            }
            "zoey-plugin-ml" | "ml" => {
                normalized.push("ml".to_string());
                None
            }
            "zoey-plugin-openai" | "openai" => {
                if std::env::var("OPENAI_API_KEY").is_ok() {
                    normalized.push("openai".to_string());
                    Some(Arc::new(OpenAIPlugin::new()) as Arc<dyn zoey_core::Plugin>)
                } else {
                    None
                }
            }
            "zoey-plugin-anthropic" | "anthropic" => {
                if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                    normalized.push("anthropic".to_string());
                    Some(Arc::new(AnthropicPlugin::new()) as Arc<dyn zoey_core::Plugin>)
                } else {
                    None
                }
            }
            "zoey-plugin-local-llm" | "local-llm" => {
                normalized.push("local-llm".to_string());
                Some(Arc::new(LocalLLMPlugin::new()) as Arc<dyn zoey_core::Plugin>)
            }
            "zoey-plugin-local-vector" | "local-vector" => {
                normalized.push("local-vector".to_string());
                let data_dir = plugin_cfg
                    .get("local-vector")
                    .and_then(|v| v.get("data_dir"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("./data/vectors");
                match LocalVectorPlugin::new(data_dir) {
                    Ok(p) => Some(Arc::new(p) as Arc<dyn zoey_core::Plugin>),
                    Err(_) => None,
                }
            }
            "zoey-plugin-adaptive" | "adaptive" => {
                normalized.push("adaptive".to_string());
                None
            }
            "zoey-plugin-production" | "production" => {
                normalized.push("production".to_string());
                None
            }
            "zoey-plugin-observability" | "observability" => {
                normalized.push("observability".to_string());
                Some(Arc::new(ObservabilityPlugin::new()) as Arc<dyn zoey_core::Plugin>)
            }
            "zoey-ext-observability-dashboard" | "observability-dashboard" | "obsdash" => {
                normalized.push("observability-dashboard".to_string());
                None
            }
            "zoey-plugin-judgement" | "zoey-plugin-judgment" | "zoey-plugin-compliance" | "judgement" | "judgment" | "compliance" => {
                normalized.push("judgment".to_string());
                None
            }
            "zoey-plugin-x402-video" | "x402-video" | "x402video" => {
                normalized.push("x402-video".to_string());
                // Load config from environment
                let plugin = X402VideoPlugin::from_env();
                Some(Arc::new(plugin) as Arc<dyn zoey_core::Plugin>)
            }
            "zoey-plugin-lifeengine" | "lifeengine" => {
                normalized.push("lifeengine".to_string());
                Some(Arc::new(LifeEnginePlugin::new()) as Arc<dyn zoey_core::Plugin>)
            }
            "zoey-ext-mcp" | "mcp" => {
                normalized.push("mcp".to_string());
                None
            }
            "zoey-plugin-sql" | "sql" | "zoey-storage-sql" | "zoey-storage-mongo" | "zoey-storage-supabase" | "zoey-storage-database" | "storage" | "database" => {
                normalized.push("storage".to_string());
                // Use storage config from character, with env var fallback
                let storage_config = &character.storage;
                
                adapter = match storage_config.adapter {
                    StorageType::Sqlite => {
                        let url = storage_config.url.clone()
                            .or_else(|| std::env::var("DATABASE_URL").ok())
                            .or_else(|| plugin_cfg.get("sql").and_then(|v| v.get("database_url")).and_then(|v| v.as_str()).map(|s| s.to_string()))
                            .unwrap_or_else(|| ":memory:".to_string());
                        match SqliteAdapter::new(&url).await {
                            Ok(mut sqlite) => match sqlite.initialize(None).await {
                                Ok(_) => {
                                    tracing::info!("Initialized SQLite storage: {}", if url == ":memory:" { "in-memory" } else { &url });
                                    Some(Arc::new(sqlite) as Arc<dyn zoey_core::IDatabaseAdapter + Send + Sync>)
                                }
                                Err(e) => {
                                    tracing::error!("Failed to initialize SQLite: {}", e);
                                    None
                                }
                            },
                            Err(e) => {
                                tracing::error!("Failed to create SQLite adapter: {}", e);
                                None
                            }
                        }
                    }
                    StorageType::Postgres => {
                        let url = storage_config.url.clone()
                            .or_else(|| std::env::var("DATABASE_URL").ok())
                            .or_else(|| plugin_cfg.get("sql").and_then(|v| v.get("database_url")).and_then(|v| v.as_str()).map(|s| s.to_string()));
                        match url {
                            Some(url) => {
                                match PostgresAdapter::new(&url).await {
                                    Ok(mut pg) => match pg.initialize(None).await {
                                        Ok(_) => {
                                            tracing::info!("Initialized PostgreSQL storage");
                                            Some(Arc::new(pg) as Arc<dyn zoey_core::IDatabaseAdapter + Send + Sync>)
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to initialize PostgreSQL: {}", e);
                                            None
                                        }
                                    },
                                    Err(e) => {
                                        tracing::error!("Failed to create PostgreSQL adapter: {}", e);
                                        None
                                    }
                                }
                            }
                            None => {
                                tracing::error!("PostgreSQL selected but no DATABASE_URL provided");
                                None
                            }
                        }
                    }
                    StorageType::Mongo => {
                        let url = storage_config.url.clone()
                            .or_else(|| std::env::var("MONGODB_URL").ok())
                            .or_else(|| std::env::var("MONGO_URL").ok());
                        let db_name = storage_config.database.clone()
                            .or_else(|| std::env::var("MONGODB_DATABASE").ok())
                            .unwrap_or_else(|| "zoey".to_string());
                        match url {
                            Some(url) => {
                                match MongoAdapter::new(&url, &db_name).await {
                                    Ok(mut mongo) => match mongo.initialize(None).await {
                                        Ok(_) => {
                                            tracing::info!("Initialized MongoDB storage: {}", db_name);
                                            Some(Arc::new(mongo) as Arc<dyn zoey_core::IDatabaseAdapter + Send + Sync>)
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to initialize MongoDB: {}", e);
                                            None
                                        }
                                    },
                                    Err(e) => {
                                        tracing::error!("Failed to create MongoDB adapter: {}", e);
                                        None
                                    }
                                }
                            }
                            None => {
                                tracing::error!("MongoDB selected but no MONGODB_URL provided");
                                None
                            }
                        }
                    }
                    StorageType::Supabase => {
                        let url = storage_config.url.clone()
                            .or_else(|| std::env::var("SUPABASE_URL").ok());
                        let api_key = storage_config.api_key.clone()
                            .or_else(|| std::env::var("SUPABASE_KEY").ok())
                            .or_else(|| std::env::var("SUPABASE_ANON_KEY").ok());
                        match (url, api_key) {
                            (Some(url), Some(key)) => {
                                let config = zoey_storage_supabase::SupabaseConfig::new(url, key);
                                match SupabaseAdapter::new(config).await {
                                    Ok(mut supabase) => match supabase.initialize(None).await {
                                        Ok(_) => {
                                            tracing::info!("Initialized Supabase storage");
                                            Some(Arc::new(supabase) as Arc<dyn zoey_core::IDatabaseAdapter + Send + Sync>)
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to initialize Supabase: {}", e);
                                            None
                                        }
                                    },
                                    Err(e) => {
                                        tracing::error!("Failed to create Supabase adapter: {}", e);
                                        None
                                    }
                                }
                            }
                            _ => {
                                tracing::error!("Supabase selected but SUPABASE_URL or SUPABASE_KEY not provided");
                                None
                            }
                        }
                    }
                };
                None
            }
            "discord" => {
                normalized.push("discord".to_string());
                None
            }
            "telegram" => {
                normalized.push("telegram".to_string());
                None
            }
            _ => None,
        };
        if let Some(p) = mapped {
            plugins.push(p);
        }
    }

    if !has_bootstrap {
        plugins.push(Arc::new(BootstrapPlugin::new()));
        normalized.push("bootstrap".to_string());
    }

    // Ensure observability is enabled for REST and metrics when runner is used
    let obs_enabled = env_bool("OBSERVABILITY_ENABLED").unwrap_or(true);
    if obs_enabled && !normalized.iter().any(|n| n == "observability") {
        plugins.push(Arc::new(ObservabilityPlugin::new()));
        normalized.push("observability".to_string());
    }

    // Ensure provider plugin based on character model_provider even if not specified in XML
    if let Some(provider) = character.model_provider.as_deref() {
        let already_has = |id: &str| normalized.iter().any(|n| n == id);
        match provider {
            "openai" => {
                if !already_has("openai") && std::env::var("OPENAI_API_KEY").is_ok() {
                    plugins.push(Arc::new(OpenAIPlugin::new()));
                    normalized.push("openai".to_string());
                }
            }
            "anthropic" => {
                if !already_has("anthropic") && std::env::var("ANTHROPIC_API_KEY").is_ok() {
                    plugins.push(Arc::new(AnthropicPlugin::new()));
                    normalized.push("anthropic".to_string());
                }
            }
            "ollama" | "local" => {
                // "ollama" is an alias for local LLM provider (Ollama backend)
                if !already_has("local-llm") {
                    plugins.push(Arc::new(LocalLLMPlugin::new()));
                    normalized.push("local-llm".to_string());
                }
            }
            _ => {}
        }
    }

    (plugins, adapter, normalized)
}

fn load_plugin_registry() -> (
    std::collections::HashMap<String, String>,
    std::collections::HashMap<String, serde_json::Value>,
) {
    let mut tokens = std::collections::HashMap::new();
    let mut cfg = std::collections::HashMap::new();
    let path = std::env::var("PLUGIN_REGISTRY_FILE")
        .unwrap_or_else(|_| "config/plugin-registry.json".to_string());
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(obj) = json.get("tokens").and_then(|v| v.as_object()) {
                for (k, v) in obj.iter() {
                    if let Some(s) = v.as_str() {
                        tokens.insert(k.to_lowercase(), s.to_string());
                    }
                }
            }
            if let Some(obj) = json.get("plugins").and_then(|v| v.as_object()) {
                for (k, v) in obj.iter() {
                    cfg.insert(k.to_string(), v.clone());
                }
            }
        }
    }
    (tokens, cfg)
}
