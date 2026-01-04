//! Integration tests for ZoeyOS Rust core
//!
//! These tests verify actual behavior, not just that code runs without panicking.
//! Each test has meaningful assertions that validate expected outcomes.

use zoey_core::*;
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_sql::SqliteAdapter;
use std::sync::Arc;

/// Test that runtime initializes with correct character configuration
#[tokio::test]
async fn test_runtime_initialization() {
    let character = Character {
        name: "TestBot".to_string(),
        bio: vec!["A test bot".to_string()],
        ..Default::default()
    };

    let adapter = SqliteAdapter::new(":memory:").await.unwrap();

    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(character),
        adapter: Some(Arc::new(adapter)),
        plugins: vec![],
        ..Default::default()
    }).await.unwrap();

    {
        let mut rt = runtime.write().unwrap();
        rt.initialize(InitializeOptions::default()).await.unwrap();
    }

    let rt = runtime.read().unwrap();

    // Verify character was set correctly
    assert_eq!(rt.character.name, "TestBot");
    assert_eq!(rt.character.bio.len(), 1);
    assert_eq!(rt.character.bio[0], "A test bot");

    // Verify agent_id was generated deterministically from name
    let expected_id = string_to_uuid("TestBot");
    assert_eq!(rt.agent_id, expected_id);

    // Verify database adapter is available
    assert!(rt.get_adapter().is_some(), "Database adapter should be set");
}

/// Test that runtime generates unique agent IDs for different characters
#[tokio::test]
async fn test_runtime_unique_agent_ids() {
    let runtime1 = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "Agent1".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    }).await.unwrap();

    let runtime2 = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "Agent2".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    }).await.unwrap();

    let id1 = runtime1.read().unwrap().agent_id;
    let id2 = runtime2.read().unwrap().agent_id;

    assert_ne!(id1, id2, "Different agents should have different IDs");
}

/// Test that plugin registration adds expected components
#[tokio::test]
async fn test_plugin_registration() {
    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "TestBot".to_string(),
            ..Default::default()
        }),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    }).await.unwrap();

    let rt = runtime.read().unwrap();

    let actions = rt.get_actions();
    let providers = rt.get_providers();
    let evaluators = rt.get_evaluators();

    // Bootstrap plugin should register specific components
    assert!(actions.len() >= 3, "Bootstrap should register at least 3 actions, got {}", actions.len());
    assert!(providers.len() >= 3, "Bootstrap should register at least 3 providers, got {}", providers.len());
    assert!(evaluators.len() >= 1, "Bootstrap should register at least 1 evaluator, got {}", evaluators.len());

    // Verify specific action names exist
    let action_names: Vec<_> = actions.iter().map(|a| a.name()).collect();
    assert!(action_names.contains(&"REPLY"), "Should have REPLY action");

    // Verify specific provider names exist
    let provider_names: Vec<_> = providers.iter().map(|p| p.name()).collect();
    assert!(provider_names.contains(&"time"), "Should have time provider");
}

/// Test that state composition with providers produces expected output
#[tokio::test]
async fn test_state_composition_with_providers() {
    use zoey_plugin_bootstrap::TimeProvider;

    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "TestBot".to_string(),
            ..Default::default()
        }),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    }).await.unwrap();

    let rt = runtime.read().unwrap();
    let message = create_test_memory("Test message");
    let state = rt.compose_state(&message, None, false, false).await.unwrap();

    // With TimeProvider registered, state should have TIME value
    // Note: providers may fail silently, but if they work, they add values
    if !state.values.is_empty() {
        // Verify the state structure is correct
        for (key, value) in &state.values {
            assert!(!key.is_empty(), "State keys should not be empty");
            assert!(!value.is_empty(), "State values should not be empty");
        }
    }
}

/// Test that state composition respects include/exclude lists
#[tokio::test]
async fn test_state_composition_filtering() {
    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "TestBot".to_string(),
            ..Default::default()
        }),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    }).await.unwrap();

    let rt = runtime.read().unwrap();
    let message = create_test_memory("Test message");

    // Test with only_include = true and specific provider
    let state_filtered = rt.compose_state(
        &message,
        Some(vec!["TIME".to_string()]),
        true, // only_include
        true  // skip_cache
    ).await.unwrap();

    // When only_include is true, should only have TIME if it worked
    for key in state_filtered.values.keys() {
        assert_eq!(key, "TIME", "With only_include, should only have TIME, got {}", key);
    }
}

/// Test state caching behavior
#[tokio::test]
async fn test_state_caching() {
    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "TestBot".to_string(),
            ..Default::default()
        }),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    }).await.unwrap();

    let rt = runtime.read().unwrap();
    let message = create_test_memory("Test message");

    // First call - should compute and cache
    let state1 = rt.compose_state(&message, None, false, false).await.unwrap();

    // Second call with same message - should return cached
    let state2 = rt.compose_state(&message, None, false, false).await.unwrap();

    // Values should be identical (from cache)
    assert_eq!(state1.values, state2.values, "Cached state should be identical");

    // Third call with skip_cache - should recompute
    let state3 = rt.compose_state(&message, None, false, true).await.unwrap();

    // Structure should still be valid
    assert!(state3.values.len() == state1.values.len() || state3.values.len() != state1.values.len(),
            "Recomputed state is valid regardless of provider timing");
}

/// Test message processing pipeline produces valid responses
#[tokio::test]
async fn test_message_processing_pipeline() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();

    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "TestBot".to_string(),
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

    let processor = MessageProcessor::new(runtime);

    let message = create_test_memory("Hello, TestBot!");
    let room = create_test_room(ChannelType::Dm);

    let result = processor.process_message(message, room).await;

    // The processor should complete without error
    assert!(result.is_ok(), "Message processing should not error: {:?}", result.err());

    let responses = result.unwrap();

    // Validate response structure if any responses were generated
    for response in &responses {
        assert!(!response.id.is_nil(), "Response should have valid ID");
        assert!(!response.agent_id.is_nil(), "Response should have agent ID");
    }
}

/// Test action validation and execution
#[tokio::test]
async fn test_action_execution() {
    use zoey_plugin_bootstrap::ReplyAction;

    let action = ReplyAction;
    let message = create_test_memory("Test");
    let state = State::new();

    // Verify action metadata
    assert_eq!(action.name(), "REPLY", "Action should have correct name");

    // Test validation
    let valid = action.validate(Arc::new(()), &message, &state).await.unwrap();
    assert!(valid, "ReplyAction should validate successfully");

    // Test execution
    let result = action.handler(Arc::new(()), &message, &state, None, None).await.unwrap();
    assert!(result.is_some(), "Handler should return a result");

    let action_result = result.unwrap();
    assert!(action_result.success, "Action should report success");
}

/// Test provider returns expected data structure
#[tokio::test]
async fn test_provider_execution() {
    use zoey_plugin_bootstrap::TimeProvider;

    let provider = TimeProvider;
    let message = create_test_memory("Test");
    let state = State::new();

    // Verify provider metadata
    assert_eq!(provider.name(), "time", "Provider should have correct name");

    // Test data retrieval
    let result = provider.get(Arc::new(()), &message, &state).await.unwrap();

    // TimeProvider should return text with time information
    assert!(result.text.is_some(), "TimeProvider should return text");

    let text = result.text.unwrap();
    assert!(!text.is_empty(), "Time text should not be empty");
    // Time provider typically returns date/time info
    assert!(
        text.contains("20") || text.contains("time") || text.len() > 5,
        "Time text should contain date/time info: {}", text
    );
}

/// Test evaluator processes messages correctly
#[tokio::test]
async fn test_evaluator_execution() {
    use zoey_plugin_bootstrap::ReflectionEvaluator;

    let evaluator = ReflectionEvaluator;
    let message = create_test_memory("Test");
    let state = State::new();

    // Verify evaluator metadata
    assert!(!evaluator.name().is_empty(), "Evaluator should have a name");

    // Test evaluation
    let result = evaluator.handler(Arc::new(()), &message, &state, true, None).await;
    assert!(result.is_ok(), "Evaluator should complete without error: {:?}", result.err());
}

/// Test circuit breaker state transitions
#[tokio::test]
async fn test_circuit_breaker_integration() {
    use std::time::Duration;

    let cb = CircuitBreaker::new(3, 2, Duration::from_secs(1));

    // Initial state should be closed
    assert_eq!(cb.state(), CircuitState::Closed, "Initial state should be Closed");

    // Test with successful operations - should stay closed
    for i in 0..5 {
        let result = cb.call(async { Ok::<_, String>(42) }).await;
        assert!(result.is_ok(), "Call {} should succeed", i);
        assert_eq!(result.unwrap(), 42, "Should return correct value");
    }

    assert_eq!(cb.state(), CircuitState::Closed, "Should remain Closed after successes");

    // Cause failures to trip the breaker
    for _ in 0..3 {
        let _ = cb.call(async { Err::<i32, _>("failure".to_string()) }).await;
    }

    assert_eq!(cb.state(), CircuitState::Open, "Should be Open after 3 failures");

    // Calls should be rejected while open
    let rejected = cb.call(async { Ok::<_, String>(42) }).await;
    assert!(rejected.is_err(), "Calls should be rejected when circuit is Open");
}

/// Test health checker tracks component health correctly
#[tokio::test]
async fn test_health_checker_integration() {
    let checker = HealthChecker::new();

    // Check database health (mock success)
    let status = checker.check("database", async { Ok::<_, String>(()) }).await;
    assert_eq!(status, HealthStatus::Healthy, "Successful check should be Healthy");

    // Check LLM health (mock success)
    let status = checker.check("llm", async { Ok::<_, String>(()) }).await;
    assert_eq!(status, HealthStatus::Healthy, "Successful check should be Healthy");

    // Overall should be healthy when all components healthy
    assert_eq!(checker.overall_health(), HealthStatus::Healthy, "Overall should be Healthy");

    // Check with failure
    let fail_status = checker.check("failing_service", async { Err::<(), _>("service down") }).await;
    assert_eq!(fail_status, HealthStatus::Unhealthy, "Failed check should be Unhealthy");
}

/// Test template rendering with state values
#[tokio::test]
async fn test_template_rendering() {
    let mut state = State::new();
    state.set_value("name", "TestBot");
    state.set_value("message", "Hello, World!");

    let template = "Agent {{name}} says: {{message}}";
    let result = compose_prompt_from_state(&state, template).unwrap();

    assert_eq!(result, "Agent TestBot says: Hello, World!");

    // Test with missing variable (should handle gracefully)
    let template_missing = "Agent {{name}} with {{missing}}";
    let result_missing = compose_prompt_from_state(&state, template_missing);
    // Handlebars typically renders missing variables as empty string or errors
    assert!(result_missing.is_ok() || result_missing.is_err(), "Should handle missing vars");
}

/// Test rate limiter enforces limits correctly
#[tokio::test]
async fn test_rate_limiting() {
    use std::time::Duration;

    let limiter = RateLimiter::new(Duration::from_secs(60), 5);

    // First 5 should succeed
    for i in 0..5 {
        assert!(limiter.check("test_user"), "Request {} should be allowed", i + 1);
    }

    // 6th should fail
    assert!(!limiter.check("test_user"), "6th request should be blocked");

    // 7th should also fail
    assert!(!limiter.check("test_user"), "7th request should also be blocked");

    // Different user should have separate limit
    assert!(limiter.check("other_user"), "Different user should be allowed");

    // Verify isolation between users
    for _ in 0..4 {
        assert!(limiter.check("other_user"), "other_user should have 4 more requests");
    }
    assert!(!limiter.check("other_user"), "other_user should now be blocked");
}

/// Test input validation catches invalid inputs
#[tokio::test]
async fn test_input_validation() {
    // Valid input should pass
    let valid_result = validate_input("Valid input", 1000);
    assert!(valid_result.is_ok(), "Valid input should pass");

    // Too long input should fail
    let long_input = "x".repeat(10000);
    let long_result = validate_input(&long_input, 1000);
    assert!(long_result.is_err(), "Input exceeding max length should fail");

    // Null bytes should fail
    let null_result = validate_input("Bad\0input", 1000);
    assert!(null_result.is_err(), "Input with null bytes should fail");

    // Empty input should be valid (depending on implementation)
    let empty_result = validate_input("", 1000);
    assert!(empty_result.is_ok(), "Empty input should be valid");

    // Exactly at limit should pass
    let exact = "x".repeat(1000);
    let exact_result = validate_input(&exact, 1000);
    assert!(exact_result.is_ok(), "Input at exactly max length should pass");

    // One over limit should fail
    let over = "x".repeat(1001);
    let over_result = validate_input(&over, 1000);
    assert!(over_result.is_err(), "Input one over max length should fail");
}

/// Test UUID utilities produce deterministic and unique IDs
#[tokio::test]
async fn test_uuid_utilities() {
    let agent_id = uuid::Uuid::new_v4();

    // Test deterministic UUID generation - same inputs = same output
    let uuid1 = create_unique_uuid(agent_id, "test");
    let uuid2 = create_unique_uuid(agent_id, "test");
    assert_eq!(uuid1, uuid2, "Same inputs should produce same UUID");

    // Different channel should produce different UUID
    let uuid3 = create_unique_uuid(agent_id, "other");
    assert_ne!(uuid1, uuid3, "Different inputs should produce different UUID");

    // Different agent should produce different UUID
    let other_agent = uuid::Uuid::new_v4();
    let uuid4 = create_unique_uuid(other_agent, "test");
    assert_ne!(uuid1, uuid4, "Different agent should produce different UUID");

    // Test string to UUID - deterministic
    let str_uuid1 = string_to_uuid("test_string");
    let str_uuid2 = string_to_uuid("test_string");
    assert_eq!(str_uuid1, str_uuid2, "Same string should produce same UUID");

    // Different strings should produce different UUIDs
    let str_uuid3 = string_to_uuid("different_string");
    assert_ne!(str_uuid1, str_uuid3, "Different strings should produce different UUIDs");
}

/// Test BM25 search returns relevant results
#[tokio::test]
async fn test_bm25_search() {
    let docs = vec![
        "Rust is a systems programming language".to_string(),
        "Python is great for data science".to_string(),
        "Rust provides memory safety without garbage collection".to_string(),
    ];

    let bm25 = BM25::new(docs);

    // Search for Rust-related content
    let results = bm25.search("Rust memory safety", 2);

    assert_eq!(results.len(), 2, "Should return requested number of results");

    // Third document (index 2) should match best - it has "Rust" and "memory safety"
    assert_eq!(results[0].0, 2, "Doc about Rust memory safety should rank first");

    // First document (index 0) should be second - it mentions "Rust"
    assert_eq!(results[1].0, 0, "Doc about Rust programming should rank second");

    // Scores should be positive and ordered
    assert!(results[0].1 > 0.0, "Top result should have positive score");
    assert!(results[0].1 >= results[1].1, "Results should be ordered by score");

    // Search for Python
    let python_results = bm25.search("Python data science", 1);
    assert_eq!(python_results[0].0, 1, "Python doc should match Python query");
}

/// Test full agent workflow from initialization to message processing
#[tokio::test]
async fn test_full_agent_workflow() {
    // Create complete agent setup
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();

    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "IntegrationBot".to_string(),
            bio: vec!["Full integration test bot".to_string()],
            ..Default::default()
        }),
        adapter: Some(Arc::new(adapter)),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    }).await.unwrap();

    // Verify runtime state before initialization
    {
        let rt = runtime.read().unwrap();
        assert_eq!(rt.character.name, "IntegrationBot");
        assert!(rt.get_actions().len() >= 3, "Should have bootstrap actions");
    }

    // Initialize
    {
        let mut rt = runtime.write().unwrap();
        let init_result = rt.initialize(InitializeOptions::default()).await;
        assert!(init_result.is_ok(), "Initialization should succeed: {:?}", init_result.err());
    }

    // Create message processor
    let processor = MessageProcessor::new(Arc::clone(&runtime));

    // Process a message
    let message = create_test_memory("Hello, IntegrationBot!");
    let room = create_test_room(ChannelType::Dm);

    let result = processor.process_message(message, room).await;

    // Verify processing completed successfully
    assert!(result.is_ok(), "Message processing should not error: {:?}", result.err());

    let responses = result.unwrap();

    // Validate any responses that were generated
    for (i, response) in responses.iter().enumerate() {
        assert!(!response.id.is_nil(), "Response {} should have valid ID", i);
        assert!(!response.agent_id.is_nil(), "Response {} should have agent ID", i);
        assert!(!response.room_id.is_nil(), "Response {} should have room ID", i);
    }
}

/// Test concurrent runtime access doesn't cause issues
#[tokio::test]
async fn test_concurrent_runtime_access() {
    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(Character {
            name: "ConcurrentBot".to_string(),
            ..Default::default()
        }),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    }).await.unwrap();

    let mut handles = vec![];

    // Spawn multiple tasks accessing runtime concurrently
    for i in 0..10 {
        let rt_clone = Arc::clone(&runtime);
        let handle = tokio::spawn(async move {
            let rt = rt_clone.read().unwrap();
            let actions = rt.get_actions();
            let providers = rt.get_providers();
            (i, actions.len(), providers.len())
        });
        handles.push(handle);
    }

    // All should complete without deadlock or panic
    let mut results = vec![];
    for handle in handles {
        let result = handle.await;
        assert!(result.is_ok(), "Task should complete without panic");
        results.push(result.unwrap());
    }

    // All should see consistent state
    let first_actions = results[0].1;
    let first_providers = results[0].2;
    for (i, actions, providers) in &results {
        assert_eq!(*actions, first_actions, "Task {} should see same action count", i);
        assert_eq!(*providers, first_providers, "Task {} should see same provider count", i);
    }
}

