//! Performance benchmarks

use async_trait::async_trait;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use zoey_core::*;
use std::sync::Arc;

fn benchmark_uuid_generation(c: &mut Criterion) {
    let agent_id = uuid::Uuid::new_v4();

    c.bench_function("create_unique_uuid", |b| {
        b.iter(|| create_unique_uuid(black_box(agent_id), black_box("test_input")))
    });

    c.bench_function("string_to_uuid", |b| {
        b.iter(|| string_to_uuid(black_box("test_string")))
    });
}

fn benchmark_bm25_search(c: &mut Criterion) {
    let documents = vec![
        "The quick brown fox jumps over the lazy dog".to_string(),
        "A fast brown fox leaps across a sleepy canine".to_string(),
        "The slow turtle walks beside the energetic rabbit".to_string(),
        "Quick movements of the agile cat".to_string(),
        "Lazy afternoons with sleeping dogs".to_string(),
    ];

    let bm25 = BM25::new(documents);

    c.bench_function("bm25_search", |b| {
        b.iter(|| bm25.search(black_box("quick brown fox"), black_box(3)))
    });
}

fn benchmark_state_operations(c: &mut Criterion) {
    c.bench_function("state_creation", |b| b.iter(|| State::new()));

    c.bench_function("state_set_value", |b| {
        let mut state = State::new();
        b.iter(|| {
            state.set_value(black_box("key"), black_box("value"));
        })
    });

    c.bench_function("state_get_value", |b| {
        let mut state = State::new();
        state.set_value("key", "value");
        b.iter(|| state.get_value(black_box("key")))
    });
}

fn benchmark_template_rendering(c: &mut Criterion) {
    let mut group = c.benchmark_group("template_rendering");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let mut state = State::new();
            for i in 0..size {
                state.set_value(format!("key{}", i), format!("value{}", i));
            }

            let template = "{{key0}} {{key1}} {{key2}}";

            b.iter(|| compose_prompt_from_state(black_box(&state), black_box(template)))
        });
    }

    group.finish();
}

fn benchmark_rate_limiter(c: &mut Criterion) {
    use std::time::Duration;

    c.bench_function("rate_limiter_check", |b| {
        let limiter = RateLimiter::new(Duration::from_secs(60), 100);
        let mut counter = 0;

        b.iter(|| {
            counter += 1;
            limiter.check(black_box(&format!("user{}", counter % 10)))
        })
    });
}

fn benchmark_input_validation(c: &mut Criterion) {
    c.bench_function("validate_input", |b| {
        let input = "Hello, World! This is a test message.";
        b.iter(|| validate_input(black_box(input), black_box(1000)))
    });

    c.bench_function("sanitize_input", |b| {
        let input = "Hello\x01World\x02Test\x03Message";
        b.iter(|| sanitize_input(black_box(input)))
    });
}

// Mock plugin for benchmarking
struct BenchPlugin {
    name: String,
    dependencies: Vec<String>,
}

#[async_trait]
impl Plugin for BenchPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Benchmark plugin"
    }

    fn dependencies(&self) -> Vec<String> {
        self.dependencies.clone()
    }
}

fn benchmark_plugin_loading(c: &mut Criterion) {
    use std::collections::HashMap;
    use tokio::runtime::Runtime;

    let rt = Runtime::new().unwrap();

    // Benchmark resolving 5 plugins with simple dependencies
    c.bench_function("plugin_resolve_5_simple", |b| {
        b.iter(|| {
            let mut plugins = HashMap::new();
            for i in 0..5 {
                let plugin: Arc<dyn Plugin> = Arc::new(BenchPlugin {
                    name: format!("plugin-{}", i),
                    dependencies: vec![],
                });
                plugins.insert(format!("plugin-{}", i), plugin);
            }
            black_box(zoey_core::plugin::resolve_plugin_dependencies(plugins, false).unwrap());
        })
    });

    // Benchmark resolving 20 plugins with dependencies
    c.bench_function("plugin_resolve_20_with_deps", |b| {
        b.iter(|| {
            let mut plugins = HashMap::new();

            // Create a dependency chain: plugin-1 -> plugin-0, plugin-2 -> plugin-1, etc.
            for i in 0..20 {
                let deps = if i == 0 {
                    vec![]
                } else {
                    vec![format!("plugin-{}", i - 1)]
                };

                let plugin: Arc<dyn Plugin> = Arc::new(BenchPlugin {
                    name: format!("plugin-{}", i),
                    dependencies: deps,
                });
                plugins.insert(format!("plugin-{}", i), plugin);
            }

            black_box(zoey_core::plugin::resolve_plugin_dependencies(plugins, false).unwrap());
        })
    });

    // Benchmark loading and validating 5 plugins
    c.bench_function("plugin_load_5", |b| {
        b.iter(|| {
            let mut plugins_vec = Vec::new();
            for i in 0..5 {
                let plugin: Arc<dyn Plugin> = Arc::new(BenchPlugin {
                    name: format!("plugin-{}", i),
                    dependencies: vec![],
                });
                plugins_vec.push(plugin);
            }
            black_box(
                rt.block_on(zoey_core::plugin::load_plugins(plugins_vec, false))
                    .unwrap(),
            );
        })
    });

    // Benchmark loading and validating 20 plugins
    c.bench_function("plugin_load_20", |b| {
        b.iter(|| {
            let mut plugins_vec = Vec::new();

            for i in 0..20 {
                let deps = if i == 0 {
                    vec![]
                } else {
                    vec![format!("plugin-{}", i - 1)]
                };

                let plugin: Arc<dyn Plugin> = Arc::new(BenchPlugin {
                    name: format!("plugin-{}", i),
                    dependencies: deps,
                });
                plugins_vec.push(plugin);
            }

            black_box(
                rt.block_on(zoey_core::plugin::load_plugins(plugins_vec, false))
                    .unwrap(),
            );
        })
    });
}

fn benchmark_entity_resolution(c: &mut Criterion) {
    use entities::format_entities;

    // Benchmark entity UUID creation (deterministic entity ID generation)
    c.bench_function("entity_uuid_creation", |b| {
        let agent_id = uuid::Uuid::new_v4();
        b.iter(|| {
            black_box(entities::create_unique_uuid_for_entity(
                black_box(agent_id),
                black_box("user_123"),
            ));
        })
    });

    // Benchmark entity formatting with 10 entities (used in prompts)
    c.bench_function("entity_format_10", |b| {
        let entities_vec: Vec<Entity> = (0..10)
            .map(|i| Entity {
                id: uuid::Uuid::new_v4(),
                agent_id: uuid::Uuid::new_v4(),
                name: Some(format!("User {}", i)),
                username: Some(format!("user{}", i)),
                email: None,
                avatar_url: None,
                metadata: Metadata::new(),
                created_at: Some(12345),
            })
            .collect();

        b.iter(|| {
            black_box(format_entities(black_box(&entities_vec)));
        })
    });

    // Benchmark entity formatting with 50 entities
    c.bench_function("entity_format_50", |b| {
        let entities_vec: Vec<Entity> = (0..50)
            .map(|i| Entity {
                id: uuid::Uuid::new_v4(),
                agent_id: uuid::Uuid::new_v4(),
                name: Some(format!("User {}", i)),
                username: Some(format!("user{}", i)),
                email: None,
                avatar_url: None,
                metadata: Metadata::new(),
                created_at: Some(12345),
            })
            .collect();

        b.iter(|| {
            black_box(format_entities(black_box(&entities_vec)));
        })
    });

    // Benchmark get_entity_details (processing entities for context)
    c.bench_function("entity_details_10", |b| {
        let room = Room {
            id: uuid::Uuid::new_v4(),
            agent_id: Some(uuid::Uuid::new_v4()),
            name: "Test Room".to_string(),
            source: "test".to_string(),
            channel_type: ChannelType::GuildText,
            channel_id: None,
            server_id: None,
            world_id: uuid::Uuid::new_v4(),
            metadata: Metadata::new(),
            created_at: Some(12345),
        };

        let entities_vec: Vec<Entity> = (0..10)
            .map(|i| Entity {
                id: uuid::Uuid::new_v4(),
                agent_id: room.agent_id.unwrap(),
                name: Some(format!("User {}", i)),
                username: Some(format!("user{}", i)),
                email: None,
                avatar_url: None,
                metadata: Metadata::new(),
                created_at: Some(12345),
            })
            .collect();

        b.iter(|| {
            black_box(entities::get_entity_details(
                black_box(&room),
                black_box(&entities_vec),
            ));
        })
    });
}

fn benchmark_bm25_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("bm25_scaling");

    // Benchmark with different document counts
    for doc_count in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(doc_count), doc_count, |b, &doc_count| {
            // Generate documents
            let documents: Vec<String> = (0..doc_count).map(|i| {
                format!("Document {} contains some text about topic number {} with additional content for searching", i, i % 10)
            }).collect();

            let bm25 = BM25::new(documents);

            b.iter(|| {
                bm25.search(black_box("topic text searching"), black_box(10))
            })
        });
    }

    group.finish();
}

fn benchmark_message_processing(c: &mut Criterion) {
    // Simulate end-to-end message processing pipeline
    // This benchmark measures the overhead WITHOUT LLM calls

    // Setup: Create a realistic conversation context
    let agent_id = uuid::Uuid::new_v4();
    let room_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();

    // Pre-create conversation history (100 messages)
    let conversation_history: Vec<String> = (0..100)
        .map(|i| {
            format!(
                "Message {} from user: This is a test message about topic {}",
                i,
                i % 10
            )
        })
        .collect();

    // Pre-create entities
    let entities: Vec<Entity> = (0..10)
        .map(|i| Entity {
            id: uuid::Uuid::new_v4(),
            agent_id,
            name: Some(format!("User {}", i)),
            username: Some(format!("user{}", i)),
            email: None,
            avatar_url: None,
            metadata: Metadata::new(),
            created_at: Some(12345),
        })
        .collect();

    // Create BM25 index from conversation history
    let bm25 = BM25::new(conversation_history.clone());

    // Benchmark: Complete message processing pipeline
    c.bench_function("message_processing_pipeline", |b| {
        b.iter(|| {
            // 1. Validate input (36 ns)
            let input_msg = "Hello, can you help me with topic 5?";
            let _ = validate_input(black_box(input_msg), 1000);

            // 2. Create unique user ID (81 ns)
            let unique_user_id = create_unique_uuid(agent_id, "user_session_123");

            // 3. Create state and populate with context (50 ns)
            let mut state = State::new();
            state.set_value("userName", "TestUser");
            state.set_value("roomId", room_id.to_string());
            state.set_value("agentId", agent_id.to_string());

            // 4. Search conversation history for relevant context (130 µs for 100 docs)
            let relevant_messages = bm25.search(black_box(input_msg), 5);

            // 5. Format entities for prompt (1.7 µs for 10 entities)
            let formatted_entities = entities::format_entities(&entities);

            // 6. Render template with context (15-50 µs)
            let template = "User: {{userName}}\nContext: {{context}}\nQuestion: {{question}}";
            state.set_value("context", format!("{} entities", formatted_entities.len()));
            state.set_value("question", input_msg);
            let prompt = compose_prompt_from_state(&state, template).unwrap();

            // 7. Rate limit check (593 ns)
            let limiter = RateLimiter::new(std::time::Duration::from_secs(60), 100);
            let _can_proceed = limiter.check("user_session_123");

            black_box((unique_user_id, relevant_messages, prompt));
        })
    });

    // Benchmark: Message processing with larger context (1000 messages)
    let large_conversation: Vec<String> = (0..1000)
        .map(|i| {
            format!(
                "Message {} from user: This is a test message about topic {}",
                i,
                i % 20
            )
        })
        .collect();
    let large_bm25 = BM25::new(large_conversation);

    c.bench_function("message_processing_large_context", |b| {
        b.iter(|| {
            let input_msg = "Hello, can you help me with topic 15?";
            let _ = validate_input(black_box(input_msg), 1000);

            let mut state = State::new();
            state.set_value("userName", "TestUser");

            // Search 1K message history (1.3 ms)
            let relevant_messages = large_bm25.search(black_box(input_msg), 10);

            // Format 50 entities (9.9 µs)
            let large_entities: Vec<Entity> = (0..50)
                .map(|i| Entity {
                    id: uuid::Uuid::new_v4(),
                    agent_id,
                    name: Some(format!("User {}", i)),
                    username: Some(format!("user{}", i)),
                    email: None,
                    avatar_url: None,
                    metadata: Metadata::new(),
                    created_at: Some(12345),
                })
                .collect();
            let formatted = entities::format_entities(&large_entities);

            state.set_value("context", formatted);
            state.set_value("question", input_msg);

            let template = "Context: {{context}}\nQuestion: {{question}}";
            let prompt = compose_prompt_from_state(&state, template).unwrap();

            black_box((relevant_messages, prompt));
        })
    });
}

criterion_group!(
    benches,
    benchmark_uuid_generation,
    benchmark_bm25_search,
    benchmark_bm25_scaling,
    benchmark_state_operations,
    benchmark_template_rendering,
    benchmark_rate_limiter,
    benchmark_input_validation,
    benchmark_plugin_loading,
    benchmark_entity_resolution,
    benchmark_message_processing,
);
criterion_main!(benches);
