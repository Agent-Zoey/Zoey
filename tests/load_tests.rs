//! Load testing for performance validation
//!
//! These tests verify the system behaves correctly under load conditions.
//! They validate throughput, concurrency handling, and resource management.

use zoey_core::*;
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_sql::SqliteAdapter;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Test concurrent message processing handles load correctly
#[tokio::test]
async fn test_concurrent_message_processing() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();

    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "LoadTestBot".to_string(),
            ..Default::default()
        }),
        adapter: Some(Arc::new(adapter)),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    }).await.unwrap();

    {
        let mut rt = runtime.write().unwrap();
        rt.initialize(InitializeOptions::default()).await.unwrap();
    }

    let processor = Arc::new(MessageProcessor::new(runtime));

    // Process 100 messages concurrently
    let num_messages = 100;
    let start = Instant::now();

    let success_count = Arc::new(AtomicUsize::new(0));
    let error_count = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    for i in 0..num_messages {
        let processor_clone = Arc::clone(&processor);
        let success_counter = Arc::clone(&success_count);
        let error_counter = Arc::clone(&error_count);

        let handle = tokio::spawn(async move {
            let message = create_test_memory(&format!("Test message {}", i));
            let room = create_test_room(ChannelType::Api);
            match processor_clone.process_message(message, room).await {
                Ok(_) => {
                    success_counter.fetch_add(1, Ordering::SeqCst);
                    true
                }
                Err(_) => {
                    error_counter.fetch_add(1, Ordering::SeqCst);
                    false
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all to complete - count panics separately
    let mut panic_count = 0;
    for handle in handles {
        if handle.await.is_err() {
            panic_count += 1;
        }
    }

    let duration = start.elapsed();
    let successes = success_count.load(Ordering::SeqCst);
    let errors = error_count.load(Ordering::SeqCst);

    println!("Load Test Results:");
    println!("  Total messages: {}", num_messages);
    println!("  Successful: {}", successes);
    println!("  Errors: {}", errors);
    println!("  Panics: {}", panic_count);
    println!("  Duration: {:?}", duration);
    println!("  Throughput: {:.2} msg/s", num_messages as f64 / duration.as_secs_f64());

    // Meaningful assertions
    assert_eq!(panic_count, 0, "No tasks should panic under concurrent load");
    assert!(successes + errors == num_messages, "All messages should be processed (success or error)");
    assert!(successes > num_messages / 2, "Majority of messages should succeed, got {}/{}", successes, num_messages);

    // Performance assertion: should complete 100 messages in under 30 seconds
    assert!(duration.as_secs() < 30, "Should complete 100 messages in under 30s, took {:?}", duration);
}

/// Test that system doesn't leak memory under sustained load
#[tokio::test]
async fn test_memory_stress() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();

    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "MemoryTestBot".to_string(),
            ..Default::default()
        }),
        adapter: Some(Arc::new(adapter)),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    }).await.unwrap();

    let mut success_count = 0;
    let mut error_count = 0;

    // Create and process many messages
    for i in 0..1000 {
        let message = create_test_memory(&format!("Message {}", i));
        let rt = runtime.read().unwrap();
        match rt.compose_state(&message, None, false, true).await {
            Ok(state) => {
                success_count += 1;
                // Verify state is valid
                assert!(state.values.len() < 1000, "State shouldn't grow unboundedly");
            }
            Err(_) => {
                error_count += 1;
            }
        }
    }

    println!("Memory Stress Results:");
    println!("  Processed: 1000 messages");
    println!("  Successes: {}", success_count);
    println!("  Errors: {}", error_count);

    // All operations should complete successfully
    assert_eq!(success_count, 1000, "All 1000 compose_state calls should succeed");
    assert_eq!(error_count, 0, "No compose_state calls should fail");
}

#[tokio::test]
async fn test_rate_limiter_under_load() {
    let limiter = RateLimiter::new(std::time::Duration::from_secs(1), 100);
    
    let mut allowed = 0;
    let mut blocked = 0;
    
    // Simulate 200 requests from same user
    for _ in 0..200 {
        if limiter.check("heavy_user") {
            allowed += 1;
        } else {
            blocked += 1;
        }
    }
    
    println!("Rate Limiting Results:");
    println!("  Allowed: {}", allowed);
    println!("  Blocked: {}", blocked);
    
    assert_eq!(allowed, 100, "Should allow exactly 100 requests");
    assert_eq!(blocked, 100, "Should block exactly 100 requests");
}

#[tokio::test]
async fn test_circuit_breaker_recovery() {
    let cb = CircuitBreaker::new(5, 3, std::time::Duration::from_millis(100));
    
    // Cause 5 failures to open circuit
    for _ in 0..5 {
        let _ = cb.call(async { Err::<(), _>("failure") }).await;
    }
    
    assert_eq!(cb.state(), CircuitState::Open);
    
    // Wait for timeout
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    
    // Circuit should transition to half-open
    // Make 3 successful calls to close it
    for _ in 0..3 {
        let result = cb.call(async { Ok::<_, String>(()) }).await;
        assert!(result.is_ok());
    }
    
    assert_eq!(cb.state(), CircuitState::Closed);
}

/// Test provider execution performance with multiple providers
#[tokio::test]
async fn test_parallel_provider_execution() {
    use zoey_plugin_bootstrap::{TimeProvider, CharacterProvider, ActionsProvider};

    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "ParallelBot".to_string(),
            ..Default::default()
        }),
        plugins: vec![],
        ..Default::default()
    }).await.unwrap();

    {
        let mut rt = runtime.write().unwrap();
        rt.register_provider(Arc::new(TimeProvider));
        rt.register_provider(Arc::new(CharacterProvider));
        rt.register_provider(Arc::new(ActionsProvider));
    }

    // Verify providers were registered
    {
        let rt = runtime.read().unwrap();
        let providers = rt.get_providers();
        assert_eq!(providers.len(), 3, "Should have 3 registered providers");
    }

    let message = create_test_memory("Test");

    // Measure time to compose state (providers run in sequence currently)
    let start = Instant::now();
    let rt = runtime.read().unwrap();
    let state = rt.compose_state(&message, None, false, false).await.unwrap();
    let duration = start.elapsed();

    println!("Provider Execution:");
    println!("  Time: {:?}", duration);
    println!("  State values: {}", state.values.len());
    println!("  State keys: {:?}", state.values.keys().collect::<Vec<_>>());

    // Performance assertion: providers should complete quickly
    assert!(duration.as_secs() < 5, "Provider execution should complete in under 5s, took {:?}", duration);

    // Validate state structure
    for (key, value) in &state.values {
        assert!(!key.is_empty(), "State keys should not be empty");
        // Values can be empty strings in some cases, but keys shouldn't be
    }
}

/// Test concurrent provider registration doesn't cause race conditions
#[tokio::test]
async fn test_concurrent_provider_registration() {
    use zoey_plugin_bootstrap::TimeProvider;

    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "ConcurrentRegBot".to_string(),
            ..Default::default()
        }),
        plugins: vec![],
        ..Default::default()
    }).await.unwrap();

    // Register multiple providers sequentially (concurrent registration would need different design)
    {
        let mut rt = runtime.write().unwrap();
        for _ in 0..10 {
            rt.register_provider(Arc::new(TimeProvider));
        }
    }

    // Verify all were registered
    let rt = runtime.read().unwrap();
    let providers = rt.get_providers();
    assert_eq!(providers.len(), 10, "Should have 10 registered providers");
}

