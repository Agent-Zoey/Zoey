//! Basic agent example demonstrating ZoeyOS Rust core usage

use zoey_core::*;
use zoey_storage_sql::SqliteAdapter;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 ZoeyOS Rust Core - Basic Agent Example\n");

    // 1. Create character configuration
    let character = Character {
        id: None,
        name: "RustBot".to_string(),
        username: Some("rustbot".to_string()),
        bio: vec![
            "I am a helpful AI assistant implemented in Rust.".to_string(),
            "I demonstrate the performance and safety of Rust for AI agents.".to_string(),
        ],
        lore: vec!["Created to showcase ZoeyOS Rust implementation.".to_string()],
        knowledge: vec![
            "Rust is a systems programming language.".to_string(),
            "ZoeyOS provides a plugin-based agent framework.".to_string(),
        ],
        message_examples: vec![],
        post_examples: vec![],
        topics: vec![
            "rust".to_string(),
            "ai".to_string(),
            "programming".to_string(),
        ],
        style: CharacterStyle::default(),
        adjectives: vec![
            "helpful".to_string(),
            "efficient".to_string(),
            "reliable".to_string(),
        ],
        settings: std::collections::HashMap::new(),
        templates: None,
        plugins: vec![],
        clients: vec![],
        model_provider: Some("openai".to_string()),
    };

    println!("✓ Character created: {}", character.name);

    // 2. Create database adapter
    let adapter = SqliteAdapter::new(":memory:").await?;
    println!("✓ Database adapter initialized (SQLite in-memory)");

    // 3. Create runtime options
    let opts = RuntimeOpts {
        character: Some(character),
        adapter: Some(Arc::new(adapter)),
        plugins: vec![],
        settings: Some(std::collections::HashMap::new()),
        conversation_length: Some(32),
        ..Default::default()
    };

    // 4. Initialize runtime
    let runtime = AgentRuntime::new(opts).await?;
    println!("✓ Runtime created");

    // 5. Initialize the runtime
    {
        let mut rt = runtime.write().unwrap();
        rt.initialize(InitializeOptions::default()).await?;
    }
    println!("✓ Runtime initialized\n");

    // 6. Display runtime information
    {
        let rt = runtime.read().unwrap();
        println!("Agent Information:");
        println!("  ID: {}", rt.agent_id);
        println!("  Name: {}", rt.character.name);
        println!("  Bio: {}", rt.character.bio.join(" "));
        println!("  Actions: {}", rt.get_actions().len());
        println!("  Providers: {}", rt.get_providers().len());
        println!("  Services: {}", rt.get_services_count());
        println!();
    }

    // 7. Create a test memory
    let runtime_clone = Arc::clone(&runtime);
    let rt = runtime_clone.read().unwrap();

    let test_memory = Memory {
        id: uuid::Uuid::new_v4(),
        entity_id: uuid::Uuid::new_v4(),
        agent_id: rt.agent_id,
        room_id: uuid::Uuid::new_v4(),
        content: Content {
            text: "Hello, RustBot! How are you today?".to_string(),
            source: Some("test".to_string()),
            ..Default::default()
        },
        embedding: None,
        metadata: None,
        created_at: chrono::Utc::now().timestamp(),
        unique: Some(false),
        similarity: None,
    };

    println!("Test Message:");
    println!("  Text: {}", test_memory.content.text);
    println!("  Memory ID: {}", test_memory.id);
    println!();

    // 8. Compose state (demonstrate provider system)
    match rt.compose_state(&test_memory, None, false, false).await {
        Ok(state) => {
            println!("State Composed:");
            println!("  Values: {} entries", state.values.len());
            println!("  Data: {} entries", state.data.len());

            if !state.values.is_empty() {
                println!("  Sample values:");
                for (key, value) in state.values.iter().take(3) {
                    println!(
                        "    {}: {}",
                        key,
                        value.chars().take(50).collect::<String>()
                    );
                }
            }
            println!();
        }
        Err(e) => {
            println!(
                "  Note: State composition returned error (expected with no providers): {}",
                e
            );
            println!();
        }
    }

    // 9. Demonstrate UUID utilities
    println!("UUID Utilities:");
    let deterministic_uuid = create_unique_uuid(rt.agent_id, "test_channel");
    println!(
        "  Deterministic UUID for 'test_channel': {}",
        deterministic_uuid
    );

    let string_uuid = string_to_uuid("example_string");
    println!("  UUID from 'example_string': {}", string_uuid);
    println!();

    // 10. Demonstrate logging
    let logger = Logger::new(&rt.character.name);
    logger.info("Agent example completed successfully!");
    logger.success("All systems operational");

    println!("\n✨ Example completed successfully!\n");
    println!("Next steps:");
    println!("  1. Add plugins to extend functionality");
    println!("  2. Implement custom actions, providers, and evaluators");
    println!("  3. Connect to a real database (PostgreSQL recommended)");
    println!("  4. Integrate LLM providers (OpenAI, Anthropic, etc.)");
    println!("  5. Build custom message processing pipelines");

    Ok(())
}
