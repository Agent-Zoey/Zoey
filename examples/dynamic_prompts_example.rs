//! Dynamic Prompt Execution Example
//!
//! Demonstrates the schema-driven prompt execution system from PR #6113
//! https://github.com/zoeyOS/zoey/pull/6113
//!
//! Features:
//! - Schema-based validation
//! - Automatic retries
//! - Metrics tracking
//! - Token estimation
//! - Multiple format support (XML/JSON)

use zoey_core::*;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🎯 Dynamic Prompt Execution Example");
    println!("Based on PR #6113\n");

    // ========================================================================
    // 1. Create Dynamic Prompt Executor
    // ========================================================================

    println!("📋 Setting up dynamic prompt executor...\n");

    let executor = DynamicPromptExecutor::new(Some(1000));
    println!("✓ Executor created with max 1000 cache entries\n");

    // ========================================================================
    // 2. Define Schema for "Should Respond" Decision
    // ========================================================================

    println!("🔍 Example 1: Should Respond Decision\n");

    let should_respond_schema = vec![
        SchemaRow {
            field: "name".to_string(),
            description: "Agent name".to_string(),
            field_type: SchemaType::String,
            required: true,
            example: Some("ZoeyBot".to_string()),
            validation: None,
        },
        SchemaRow {
            field: "reasoning".to_string(),
            description: "Reason for decision".to_string(),
            field_type: SchemaType::String,
            required: true,
            example: Some("User asked a direct question".to_string()),
            validation: None,
        },
        SchemaRow {
            field: "action".to_string(),
            description: "RESPOND, IGNORE, or WAIT".to_string(),
            field_type: SchemaType::Enum,
            required: true,
            example: Some("RESPOND".to_string()),
            validation: Some("^(RESPOND|IGNORE|WAIT)$".to_string()),
        },
    ];

    println!("Schema fields:");
    for field in &should_respond_schema {
        println!(
            "  - {}: {:?} (required: {})",
            field.field, field.field_type, field.required
        );
    }
    println!();

    // ========================================================================
    // 3. Create State with Context
    // ========================================================================

    let mut state = State::new();
    state.set_value("userMessage", "Hey ZoeyBot, how are you today?");
    state.set_value("roomType", "DM");
    state.set_value("agentName", "ZoeyBot");

    println!("State values:");
    for (key, value) in &state.values {
        println!("  {}: {}", key, value);
    }
    println!();

    // ========================================================================
    // 4. Execute with Mock Model (Simulates LLM Response)
    // ========================================================================

    println!("🤖 Executing prompt with schema validation...\n");

    // Mock model function that returns XML response
    let model_fn = |prompt: String, opts: DynamicPromptOptions| async move {
        println!("  📝 Model called with prompt ({} chars)", prompt.len());
        println!(
            "  ⚙️  Options: model_size={:?}, retries={}",
            opts.model_size, opts.max_retries
        );

        // Simulate LLM thinking time
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Return XML formatted response
        Ok(r#"<response>
            <name>ZoeyBot</name>
            <reasoning>User greeted me in a direct message, should respond politely</reasoning>
            <action>RESPOND</action>
        </response>"#
            .to_string())
    };

    let options = DynamicPromptOptions {
        model_size: Some("small".to_string()),
        validation_level: ValidationLevel::Strict,
        max_retries: 3,
        ..Default::default()
    };

    let template = r#"
# Task: Decide whether to respond

User Message: {{userMessage}}
Room Type: {{roomType}}
Agent: {{agentName}}

Analyze the message and decide whether to respond.

Return XML with:
<response>
  <name>agent name</name>
  <reasoning>your reasoning</reasoning>
  <action>RESPOND, IGNORE, or WAIT</action>
</response>
"#;

    match executor
        .execute_from_state(
            &state,
            should_respond_schema.clone(),
            template,
            options,
            model_fn,
        )
        .await
    {
        Ok(result) => {
            println!("  ✅ Execution successful!\n");
            println!("  Parsed response:");
            for (key, value) in &result {
                println!("    {}: {:?}", key, value);
            }
            println!();
        }
        Err(e) => {
            println!("  ❌ Execution failed: {}\n", e);
        }
    }

    // ========================================================================
    // 5. Example 2: Message Handler with Thought + Actions
    // ========================================================================

    println!("💬 Example 2: Message Handler Schema\n");

    let message_handler_schema = vec![
        SchemaRow {
            field: "thought".to_string(),
            description: "Agent's internal thought process".to_string(),
            field_type: SchemaType::String,
            required: true,
            example: Some("I should provide a helpful response".to_string()),
            validation: None,
        },
        SchemaRow {
            field: "actions".to_string(),
            description: "Actions to take".to_string(),
            field_type: SchemaType::Array,
            required: true,
            example: Some(r#"["REPLY", "CONTINUE"]"#.to_string()),
            validation: None,
        },
        SchemaRow {
            field: "text".to_string(),
            description: "Response text".to_string(),
            field_type: SchemaType::String,
            required: true,
            example: Some("I'm doing well, thank you for asking!".to_string()),
            validation: None,
        },
    ];

    let model_fn_2 = |_prompt: String, _opts: DynamicPromptOptions| async move {
        Ok(r#"<response>
            <thought>User greeted me, I should respond warmly and offer help</thought>
            <actions>["REPLY"]</actions>
            <text>I'm doing well, thank you for asking! How can I assist you today?</text>
        </response>"#
            .to_string())
    };

    let mut state_2 = State::new();
    state_2.set_value("userMessage", "Hey ZoeyBot, how are you today?");

    match executor
        .execute_from_state(
            &state_2,
            message_handler_schema,
            "Generate response for: {{userMessage}}",
            DynamicPromptOptions::default(),
            model_fn_2,
        )
        .await
    {
        Ok(result) => {
            println!("  ✅ Message handler executed successfully!\n");
            if let Some(thought) = result.get("thought") {
                println!("  💭 Thought: {:?}", thought);
            }
            if let Some(actions) = result.get("actions") {
                println!("  🎬 Actions: {:?}", actions);
            }
            if let Some(text) = result.get("text") {
                println!("  💬 Response: {:?}", text);
            }
            println!();
        }
        Err(e) => {
            println!("  ❌ Execution failed: {}\n", e);
        }
    }

    // ========================================================================
    // 6. Example 3: Multi-Step Decision (isFinish check)
    // ========================================================================

    println!("🔄 Example 3: Multi-Step Decision\n");

    let multi_step_schema = vec![
        SchemaRow {
            field: "thought".to_string(),
            description: "Agent's reasoning about task completion".to_string(),
            field_type: SchemaType::String,
            required: true,
            example: None,
            validation: None,
        },
        SchemaRow {
            field: "isFinish".to_string(),
            description: "Whether task is complete (true/false)".to_string(),
            field_type: SchemaType::String,
            required: true,
            example: Some("true".to_string()),
            validation: Some("^(true|false)$".to_string()),
        },
        SchemaRow {
            field: "action".to_string(),
            description: "Next action to take".to_string(),
            field_type: SchemaType::String,
            required: false,
            example: Some("".to_string()),
            validation: None,
        },
    ];

    let model_fn_3 = |_prompt: String, _opts: DynamicPromptOptions| async move {
        Ok(r#"<response>
            <thought>Task is complete, all requirements met</thought>
            <isFinish>true</isFinish>
            <action></action>
        </response>"#
            .to_string())
    };

    let mut state_3 = State::new();
    state_3.set_value("taskDescription", "Explain what Rust is");
    state_3.set_value("stepCount", "3");

    let mut opts = DynamicPromptOptions::default();
    opts.validation_level = ValidationLevel::Maximum; // Test regex validation

    match executor
        .execute_from_state(
            &state_3,
            multi_step_schema.clone(),
            "Is task complete? {{taskDescription}}",
            opts,
            model_fn_3,
        )
        .await
    {
        Ok(result) => {
            println!("  ✅ Multi-step decision executed!\n");
            if let Some(is_finish) = result.get("isFinish") {
                println!("  🏁 Is Finish: {:?}", is_finish);
            }
            if let Some(thought) = result.get("thought") {
                println!("  💭 Thought: {:?}", thought);
            }
            println!();
        }
        Err(e) => {
            println!("  ❌ Execution failed: {}\n", e);
        }
    }

    // ========================================================================
    // 7. View Metrics
    // ========================================================================

    println!("📊 Dynamic Prompt Metrics\n");

    let summary = executor.get_metrics_summary();

    println!("  Total executions: {}", summary.total_executions);
    println!("  Successful: {}", summary.total_successes);
    println!("  Failed: {}", summary.total_failures);
    println!("  Total retries: {}", summary.total_retries);
    println!("  Success rate: {:.1}%", summary.success_rate * 100.0);
    println!("  Avg response time: {:.1}ms", summary.avg_response_time_ms);
    println!("  Estimated tokens: {}", summary.total_tokens);
    println!("  Unique schemas: {}", summary.unique_schemas);
    println!("  Unique models: {}", summary.unique_models);
    println!();

    let schema_metrics = executor.get_schema_metrics();
    if !schema_metrics.is_empty() {
        println!("  Schema-specific metrics:");
        for (key, metrics) in schema_metrics.iter().take(3) {
            println!("    {}:", key);
            println!("      Executions: {}", metrics.execution_count);
            println!(
                "      Success: {}, Failures: {}",
                metrics.success_count, metrics.failure_count
            );
            println!("      Avg time: {:.1}ms", metrics.avg_response_time_ms);
        }
        println!();
    }

    // ========================================================================
    // 8. Test Retry Mechanism
    // ========================================================================

    println!("🔄 Example 4: Testing Retry Mechanism\n");

    // Model that fails first 2 times, then succeeds
    // Using AtomicUsize for thread-safe counter that works with Fn trait
    use std::sync::atomic::{AtomicUsize, Ordering};
    let attempt = Arc::new(AtomicUsize::new(0));
    let attempt_clone = Arc::clone(&attempt);
    let model_fn_retry = move |_prompt: String, _opts: DynamicPromptOptions| {
        let attempt = Arc::clone(&attempt_clone);
        async move {
            let current_attempt = attempt.fetch_add(1, Ordering::SeqCst);
            println!("  Attempt {}", current_attempt + 1);

            if current_attempt < 2 {
                // Fail validation (missing required field)
                Ok("<response><partial>data</partial></response>".to_string())
            } else {
                // Success on 3rd attempt
                Ok(r#"<response>
                    <thought>Finally got it right</thought>
                    <isFinish>true</isFinish>
                    <action>COMPLETE</action>
                </response>"#
                    .to_string())
            }
        }
    };

    let retry_opts = DynamicPromptOptions {
        max_retries: 3,
        validation_level: ValidationLevel::Strict,
        ..Default::default()
    };

    match executor
        .execute_from_state(
            &state_3,
            multi_step_schema.clone(),
            "Complete task",
            retry_opts,
            model_fn_retry,
        )
        .await
    {
        Ok(result) => {
            println!("\n  ✅ Succeeded after retries!");
            println!("  Result: {:?}\n", result.get("isFinish"));
        }
        Err(e) => {
            println!("\n  ❌ Failed even with retries: {}\n", e);
        }
    }

    // ========================================================================
    // 9. Final Metrics
    // ========================================================================

    println!("📈 Final Metrics Summary\n");

    let final_summary = executor.get_metrics_summary();
    println!("  Total executions: {}", final_summary.total_executions);
    println!("  Success rate: {:.1}%", final_summary.success_rate * 100.0);
    println!("  Total retries needed: {}", final_summary.total_retries);
    println!(
        "  Average response time: {:.1}ms",
        final_summary.avg_response_time_ms
    );

    println!("\n✨ Key Benefits:");
    println!("  ✅ Schema validation ensures structured responses");
    println!("  ✅ Automatic retries handle transient failures");
    println!("  ✅ Metrics track model performance");
    println!("  ✅ Token estimation helps with cost tracking");
    println!("  ✅ LRU cache prevents memory bloat");

    println!("\n💡 Use Cases:");
    println!("  - shouldRespond() evaluation (small model)");
    println!("  - Message handler (thought + actions + text)");
    println!("  - Multi-step decision loops");
    println!("  - Any structured LLM output");

    println!("\n🎯 Example complete!");

    Ok(())
}
