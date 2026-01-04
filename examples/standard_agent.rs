//! Standard Agent Example - WITHOUT HIPAA compliance
//!
//! For organizations that don't need HIPAA features

use zoey_core::*;
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_memory::MemoryManagerPlugin;
use zoey_provider_openai::OpenAIPlugin;
use zoey_storage_sql::SqliteAdapter;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║     ZoeyOS Rust - Standard Agent (No HIPAA)          ║");
    println!("║         For General Purpose Use                        ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    // 1. Create standard character
    let character = Character {
        name: "StandardBot".to_string(),
        username: Some("standardbot".to_string()),
        bio: vec![
            "I am a helpful AI assistant for general use.".to_string(),
            "I use cloud AI services for best performance.".to_string(),
        ],
        ..Default::default()
    };

    println!("✓ Character created: {}", character.name);
    println!("  - Standard mode (no HIPAA)");
    println!("  - Cloud AI enabled\n");

    // 2. Database WITHOUT HIPAA features
    let adapter = SqliteAdapter::new(":memory:").await?;
    println!("✓ Database initialized (standard mode)");
    println!("  - HIPAA features: DISABLED");
    println!("  - Audit logging: Optional");
    println!("  - Encryption: Standard");
    println!("  - Retention: Configurable\n");

    // 3. Runtime with standard plugins (no compliance overhead)
    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(character),
        adapter: Some(Arc::new(adapter)),
        plugins: vec![
            Arc::new(BootstrapPlugin::new()), // Core functionality
            Arc::new(OpenAIPlugin::new()),    // Cloud AI (GPT-4)
            Arc::new(MemoryManagerPlugin::default()), // Memory management
                                              // Note: JudgmentPlugin NOT included - no PII scanning
                                              // Note: LocalLLMPlugin NOT included - cloud is fine
        ],
        ..Default::default()
    })
    .await?;

    println!("✓ Runtime created with standard plugins:");
    println!("  - Bootstrap (core actions/providers)");
    println!("  - OpenAI (GPT-4 for best quality)");
    println!("  - NO compliance overhead");
    println!("  - NO PII scanning");
    println!("  - Cloud AI enabled\n");

    // 4. Initialize
    {
        let mut rt = runtime.write().unwrap();
        rt.initialize(InitializeOptions::default()).await?;
    }
    println!("✓ Runtime initialized\n");

    // 5. Display configuration
    {
        let rt = runtime.read().unwrap();
        println!("═══ STANDARD CONFIGURATION ═══\n");
        println!("Agent: {}", rt.character.name);
        println!("Actions: {}", rt.get_actions().len());
        println!("Providers: {}", rt.get_providers().len());
        println!("Evaluators: {}", rt.get_evaluators().len());
        println!();

        println!("Available Actions:");
        for action in rt.get_actions().iter() {
            println!("  - {}", action.name());
        }
        println!();

        println!("Available Providers:");
        for provider in rt.get_providers().iter() {
            println!("  - {}", provider.name());
        }
        println!();
    }

    // 6. Feature comparison
    println!("═══ FEATURE COMPARISON ═══\n");
    println!("┌────────────────────────────┬──────────┬────────────┐");
    println!("│ Feature                    │ Standard │ Government │");
    println!("├────────────────────────────┼──────────┼────────────┤");
    println!("│ HIPAA Compliance           │    ❌    │     ✅     │");
    println!("│ PII Detection              │    ❌    │     ✅     │");
    println!("│ Audit Logging (required)   │    ❌    │     ✅     │");
    println!("│ Local LLM (required)       │    ❌    │     ✅     │");
    println!("│ Cloud AI                   │    ✅    │     ❌     │");
    println!("│ Planning Functors          │    ✅    │     ✅     │");
    println!("│ Circuit Breakers           │    ✅    │     ✅     │");
    println!("│ Rate Limiting              │    ✅    │     ✅     │");
    println!("│ Performance Optimized      │    ✅    │     ✅     │");
    println!("└────────────────────────────┴──────────┴────────────┘");
    println!();

    println!("═══ WHEN TO USE EACH MODE ═══\n");
    println!("STANDARD MODE (This Example):");
    println!("  ✓ General purpose chatbots");
    println!("  ✓ Customer service bots");
    println!("  ✓ Gaming NPCs");
    println!("  ✓ Personal assistants");
    println!("  ✓ Public-facing agents");
    println!("  ✓ Non-sensitive data\n");

    println!("GOVERNMENT MODE (government_compliant_agent.rs):");
    println!("  ✓ Healthcare providers");
    println!("  ✓ Government agencies");
    println!("  ✓ Financial institutions");
    println!("  ✓ Legal firms");
    println!("  ✓ Sensitive data");
    println!("  ✓ Air-gapped networks\n");

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║           ✨ FLEXIBILITY BUILT-IN ✨                   ║");
    println!("║                                                        ║");
    println!("║  Use compliance features only when you need them!      ║");
    println!("║  No overhead for general purpose use.                  ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    println!("Configuration:");
    println!("  • Add/remove plugins as needed");
    println!("  • Enable HIPAA only if required");
    println!("  • Use cloud AI for better quality (when allowed)");
    println!("  • Mix local and cloud as needed\n");

    Ok(())
}
