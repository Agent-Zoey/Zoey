//! Advanced agent example with streaming, circuit breakers, and health checks

use zoey_core::*;
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_storage_sql::SqliteAdapter;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 ZoeyOS Rust Core - Advanced Agent Example\n");

    // 1. Create character with advanced configuration
    let character = Character {
        name: "AdvancedBot".to_string(),
        username: Some("advancedbot".to_string()),
        bio: vec![
            "I am an advanced AI assistant with resilience features.".to_string(),
            "I can handle failures gracefully and stream responses.".to_string(),
        ],
        lore: vec!["Built with production-grade features.".to_string()],
        knowledge: vec!["I understand circuit breakers and health monitoring.".to_string()],
        ..Default::default()
    };

    println!("✓ Character created: {}", character.name);

    // 2. Create database with health monitoring
    let adapter = SqliteAdapter::new(":memory:").await?;
    println!("✓ Database adapter initialized");

    // 3. Create runtime with plugins
    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(character),
        adapter: Some(Arc::new(adapter)),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    })
    .await?;

    // 4. Initialize with health checks
    {
        let mut rt = runtime.write().unwrap();
        rt.initialize(InitializeOptions::default()).await?;
    }
    println!("✓ Runtime initialized with bootstrap plugin\n");

    // 5. Demonstrate health checking
    println!("=== Health Monitoring ===");
    let health_checker = HealthChecker::new();

    // Check database health
    let db_health = health_checker
        .check("database", async {
            // Simulated database check
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<_, String>(())
        })
        .await;
    println!("  Database health: {:?}", db_health);

    // Check LLM health
    let llm_health = health_checker
        .check("llm", async {
            // Simulated LLM check
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok::<_, String>(())
        })
        .await;
    println!("  LLM health: {:?}", llm_health);

    // Overall health
    println!("  Overall health: {:?}", health_checker.overall_health());
    println!();

    // 6. Demonstrate circuit breaker
    println!("=== Circuit Breaker ===");
    let circuit_breaker = CircuitBreaker::new(3, 2, Duration::from_secs(5));

    // Successful calls
    for i in 1..=3 {
        let result = circuit_breaker
            .call(async { Ok::<_, String>(format!("Success {}", i)) })
            .await;
        println!("  Call {}: {:?}", i, result);
    }
    println!("  Circuit state: {:?}", circuit_breaker.state());
    println!();

    // 7. Demonstrate retry logic
    println!("=== Retry with Backoff ===");
    let mut attempt = 0;
    let retry_config = RetryConfig {
        max_retries: 3,
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(2),
        multiplier: 2.0,
    };

    let result = retry_with_backoff(retry_config, || {
        attempt += 1;
        println!("  Attempt {}", attempt);
        Box::pin(async move {
            if attempt < 2 {
                Err("Simulated failure")
            } else {
                Ok("Success!")
            }
        })
    })
    .await;
    println!("  Final result: {:?}", result);
    println!();

    // 8. Demonstrate streaming
    println!("=== Streaming Response ===");
    let (sender, receiver) = create_text_stream(10);

    // Spawn task to send chunks
    tokio::spawn(async move {
        let handler = StreamHandler::new(sender);
        handler
            .send_chunk("Hello".to_string(), false)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        handler
            .send_chunk(" from".to_string(), false)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        handler
            .send_chunk(" streaming".to_string(), false)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        handler.finish("!".to_string()).await.unwrap();
    });

    // Collect streamed response
    let full_response = collect_stream(receiver).await?;
    println!("  Streamed response: {}", full_response);
    println!();

    // 9. Demonstrate rate limiting
    println!("=== Rate Limiting ===");
    let rate_limiter = RateLimiter::new(Duration::from_secs(60), 5);

    for i in 1..=7 {
        let allowed = rate_limiter.check("user1");
        println!(
            "  Request {}: {} (remaining: {})",
            i,
            if allowed {
                "✓ Allowed"
            } else {
                "✗ Blocked"
            },
            rate_limiter.remaining("user1")
        );
    }
    println!();

    // 10. Demonstrate input validation and sanitization
    println!("=== Input Validation & Security ===");

    let long_input = "x".repeat(10000);
    let inputs: Vec<(&str, bool)> = vec![
        ("Valid input", true),
        (&long_input, false),
        ("Bad\0input", false),
        ("Control\x01chars\x02", true), // Can be sanitized
    ];

    for (input, should_be_valid) in inputs {
        let valid = validate_input(input, 1000).is_ok();
        let sanitized = sanitize_input(input);
        println!("  Input: {:?}", input.chars().take(20).collect::<String>());
        println!("    Valid: {} (expected: {})", valid, should_be_valid);
        println!(
            "    Sanitized: {:?}",
            sanitized.chars().take(20).collect::<String>()
        );
    }
    println!();

    // 11. Display component counts
    {
        let rt = runtime.read().unwrap();
        println!("=== Agent Components ===");
        println!("  Actions: {}", rt.get_actions().len());
        println!("  Providers: {}", rt.get_providers().len());
        println!("  Evaluators: {}", rt.get_evaluators().len());
        println!("  Services: {}", rt.get_services_count());
        println!();

        // List actions
        println!("  Available Actions:");
        for action in rt.get_actions().iter() {
            println!("    - {}: {}", action.name(), action.description());
        }
        println!();

        // List providers
        println!("  Available Providers:");
        for provider in rt.get_providers().iter() {
            println!(
                "    - {}: {}",
                provider.name(),
                provider
                    .description()
                    .unwrap_or_else(|| "No description".to_string())
            );
        }
        println!();

        // List evaluators
        println!("  Available Evaluators:");
        for evaluator in rt.get_evaluators().iter() {
            println!("    - {}: {}", evaluator.name(), evaluator.description());
        }
    }

    println!("\n✨ Advanced features demonstration complete!\n");
    println!("Production Features Demonstrated:");
    println!("  ✓ Health monitoring");
    println!("  ✓ Circuit breakers");
    println!("  ✓ Retry logic with exponential backoff");
    println!("  ✓ Streaming responses");
    println!("  ✓ Rate limiting");
    println!("  ✓ Input validation & sanitization");
    println!("  ✓ Component management");

    Ok(())
}
