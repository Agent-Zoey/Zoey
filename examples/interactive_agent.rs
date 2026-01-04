//! Interactive Agent - Talk to the AI agent!
//!
//! This is a full end-to-end example where you can chat with the agent.
//! Set OPENAI_API_KEY or ANTHROPIC_API_KEY environment variable to enable LLM.

use zoey_core::*;
use zoey_provider_anthropic::AnthropicPlugin;
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_provider_local::LocalLLMPlugin;
use zoey_provider_openai::OpenAIPlugin;
use zoey_storage_sql::SqliteAdapter;
use std::io::{self, Write};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    load_env().ok();

    // Initialize logging from RUST_LOG environment variable
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,zoey_core=debug".to_string()),
        )
        .init();

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║        ZoeyOS Rust Core - Interactive Agent          ║");
    println!("║              Talk to Your AI Agent!                    ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    // Load character from XML file
    let character_path = get_env_or("CHARACTER_FILE", "characters/zoey-bot.xml");

    println!("Loading character from: {}", character_path);

    let character = match load_character_from_xml(&character_path) {
        Ok(c) => {
            println!("✓ Character loaded from XML: {}", c.name);
            c
        }
        Err(e) => {
            println!("⚠️  Could not load character file: {}", e);
            println!("Using default character configuration...\n");

            // Fallback to default character
            Character {
        name: "ZoeyBot".to_string(),
        username: Some("zoeybot".to_string()),
        bio: vec![
            "I am ZoeyBot, a helpful AI assistant built in Rust.".to_string(),
            "I can help you with questions, have conversations, and demonstrate the power of Rust for AI.".to_string(),
        ],
        lore: vec![
            "Created as a demonstration of ZoeyOS Rust Core.".to_string(),
            "I showcase performance, safety, and flexibility.".to_string(),
        ],
        knowledge: vec![
            "Rust is a systems programming language focused on safety and performance.".to_string(),
            "ZoeyOS provides a flexible, plugin-based architecture for AI agents.".to_string(),
        ],
        style: CharacterStyle {
            all: vec![
                "Be helpful and friendly".to_string(),
                "Provide clear, concise answers".to_string(),
                "Show enthusiasm for Rust and AI".to_string(),
            ],
            chat: vec![
                "Use a conversational tone".to_string(),
                "Be approachable and warm".to_string(),
            ],
            post: vec![],
        },
        adjectives: vec![
            "helpful".to_string(),
            "intelligent".to_string(),
            "efficient".to_string(),
            "friendly".to_string(),
        ],
        topics: vec![
            "rust programming".to_string(),
            "ai agents".to_string(),
            "technology".to_string(),
        ],
                ..Default::default()
            }
        }
    };

    // Check for API keys from environment
    let has_openai = std::env::var("OPENAI_API_KEY").is_ok();
    let has_anthropic = std::env::var("ANTHROPIC_API_KEY").is_ok();
    let has_ollama = check_ollama_available().await;

    println!("\n✓ Character loaded: {}", character.name);
    println!("  Bio: {}", character.bio.first().unwrap_or(&String::new()));
    println!("  Topics: {}", character.topics.join(", "));
    println!();

    println!("LLM Availability:");
    println!(
        "  OpenAI: {}",
        if has_openai {
            "✓ Available"
        } else {
            "✗ Not configured"
        }
    );
    println!(
        "  Anthropic: {}",
        if has_anthropic {
            "✓ Available"
        } else {
            "✗ Not configured"
        }
    );
    println!(
        "  Ollama (Local): {}",
        if has_ollama {
            "✓ Available"
        } else {
            "✗ Not available"
        }
    );

    if !has_openai && !has_anthropic && !has_ollama {
        println!("\n⚠️  Warning: No LLM providers configured!");
        println!("  Set OPENAI_API_KEY or ANTHROPIC_API_KEY to enable cloud AI");
        println!("  Or install Ollama (https://ollama.ai) for local AI\n");
    }
    println!();

    // Initialize database
    let adapter = SqliteAdapter::new(":memory:").await?;
    println!("✓ Database initialized (SQLite in-memory)");

    // Select plugins based on available LLMs
    let mut plugins: Vec<Arc<dyn Plugin>> = vec![];

    // Always add bootstrap (core functionality)
    plugins.push(Arc::new(BootstrapPlugin::new()));

    // Add LLM plugins based on availability
    if has_ollama {
        println!("  + Local LLM plugin (Ollama) - Priority 200");
        plugins.push(Arc::new(LocalLLMPlugin::new()));
    }

    if has_openai {
        println!("  + OpenAI plugin (GPT-4) - Priority 100");
        plugins.push(Arc::new(OpenAIPlugin::new()));
    }

    if has_anthropic {
        println!("  + Anthropic plugin (Claude) - Priority 100");
        plugins.push(Arc::new(AnthropicPlugin::new()));
    }

    println!();

    // Merge character settings with environment overrides
    let mut settings = character.settings.clone();

    // Environment variables override character file settings
    let model = get_env_or("LOCAL_LLM_MODEL", "phi3:mini");
    let endpoint = get_env_or("LOCAL_LLM_ENDPOINT", "http://localhost:11434");
    let temperature = get_env_float("LOCAL_LLM_TEMPERATURE", 0.7);
    let max_tokens = get_env_int("LOCAL_LLM_MAX_TOKENS", 2000);

    settings.insert("LOCAL_LLM_MODEL".to_string(), serde_json::json!(model));
    settings.insert(
        "LOCAL_LLM_ENDPOINT".to_string(),
        serde_json::json!(endpoint),
    );
    settings.insert("temperature".to_string(), serde_json::json!(temperature));
    settings.insert("max_tokens".to_string(), serde_json::json!(max_tokens));

    // Display LLM configuration based on provider
    let provider = settings
        .get("model_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("local");

    println!("LLM Configuration:");
    println!("  Provider: {}", provider);

    match provider {
        "openai" => {
            let model = settings
                .get("OPENAI_MODEL")
                .and_then(|v| v.as_str())
                .unwrap_or("gpt-4");
            println!("  Model: {}", model);
            println!(
                "  Temperature: {}",
                settings
                    .get("temperature")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.7)
            );
            println!(
                "  Max Tokens: {}",
                settings
                    .get("max_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2000)
            );
        }
        "anthropic" => {
            let model = settings
                .get("ANTHROPIC_MODEL")
                .and_then(|v| v.as_str())
                .unwrap_or("claude-3-opus-20240229");
            println!("  Model: {}", model);
            println!(
                "  Temperature: {}",
                settings
                    .get("temperature")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.7)
            );
            println!(
                "  Max Tokens: {}",
                settings
                    .get("max_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2000)
            );
        }
        _ => {
            // Local LLM
            let model = settings
                .get("LOCAL_LLM_MODEL")
                .and_then(|v| v.as_str())
                .unwrap_or("phi3:mini");
            let endpoint = settings
                .get("LOCAL_LLM_ENDPOINT")
                .and_then(|v| v.as_str())
                .unwrap_or("http://localhost:11434");
            println!("  Model: {}", model);
            println!("  Endpoint: {}", endpoint);
        }
    }
    println!();

    // Create runtime
    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(character),
        adapter: Some(Arc::new(adapter)),
        plugins,
        settings: Some(settings),
        ..Default::default()
    })
    .await?;

    println!("✓ Runtime created with plugins");

    // Initialize runtime
    {
        let mut rt = runtime.write().unwrap();
        rt.initialize(InitializeOptions::default()).await?;
    }

    println!("✓ Runtime initialized\n");

    // Display agent capabilities
    {
        let rt = runtime.read().unwrap();
        println!("Agent Capabilities:");
        println!("  Actions: {}", rt.get_actions().len());
        for action in rt.get_actions().iter().take(10) {
            println!("    - {}: {}", action.name(), action.description());
        }
        println!();

        println!("  Providers: {}", rt.get_providers().len());
        for provider in rt.get_providers().iter().take(10) {
            let desc = provider
                .description()
                .unwrap_or_else(|| "No description".to_string());
            println!("    - {}: {}", provider.name(), desc);
        }
        println!();

        println!("  Evaluators: {}", rt.get_evaluators().len());
        for evaluator in rt.get_evaluators().iter() {
            println!("    - {}: {}", evaluator.name(), evaluator.description());
        }
        println!();
    }

    // Create message processor
    let processor = MessageProcessor::new(Arc::clone(&runtime));
    let agent_id = runtime.read().unwrap().agent_id;

    // Create a room for the conversation
    let room_id = create_unique_uuid(agent_id, "interactive_chat");
    let room = Room {
        id: room_id,
        agent_id: Some(agent_id),
        name: "Interactive Chat".to_string(),
        source: "console".to_string(),
        channel_type: ChannelType::Dm, // DM so agent always responds
        channel_id: Some("console".to_string()),
        server_id: None,
        world_id: create_unique_uuid(agent_id, "console_world"),
        metadata: std::collections::HashMap::new(),
        created_at: Some(chrono::Utc::now().timestamp()),
    };

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║                  Chat Started!                         ║");
    println!("║    Type your messages (Ctrl+C or 'exit' to quit)       ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    // Interactive loop
    let user_id = uuid::Uuid::new_v4();

    loop {
        // Prompt for user input
        print!("You: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        // Check for exit
        if input.is_empty() {
            continue;
        }

        if input == "exit" || input == "quit" {
            println!("\nGoodbye! 👋\n");
            break;
        }

        // Create memory from user input
        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: user_id,
            agent_id,
            room_id,
            content: Content {
                text: input.to_string(),
                source: Some("console".to_string()),
                channel_type: Some("DM".to_string()),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: Some(false),
            similarity: None,
        };

        // Process message
        print!("ZoeyBot: ");
        io::stdout().flush().unwrap();

        match processor.process_message(message, room.clone()).await {
            Ok(responses) => {
                if responses.is_empty() {
                    println!("(Agent chose not to respond)");
                } else {
                    for response in responses {
                        println!("{}", response.content.text);
                    }
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }

        println!();
    }

    Ok(())
}

/// Check if Ollama is available
async fn check_ollama_available() -> bool {
    match reqwest::get("http://localhost:11434/api/tags").await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}
