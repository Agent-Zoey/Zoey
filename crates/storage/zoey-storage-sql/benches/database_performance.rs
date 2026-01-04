//! Database performance benchmarks
//!
//! Benchmarks SQLite and PostgreSQL CRUD operations to validate performance claims.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zoey_core::*;
use zoey_storage_sql::{PostgresAdapter, SqliteAdapter};
use tokio::runtime::Runtime;

/// Benchmark SQLite agent CRUD operations
fn bench_sqlite_agent_crud(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("sqlite_create_agent", |b| {
        b.to_async(&rt).iter(|| async {
            let mut adapter = SqliteAdapter::new(":memory:").await.unwrap();
            adapter.initialize(None).await.unwrap();

            let agent_id = uuid::Uuid::new_v4();
            let character = Character {
                name: "BenchAgent".to_string(),
                bio: vec!["Benchmark test agent".to_string()],
                ..Default::default()
            };

            let agent = Agent {
                id: agent_id,
                name: "Benchmark Agent".to_string(),
                character: serde_json::to_value(&character).unwrap(),
                created_at: Some(chrono::Utc::now().timestamp()),
                updated_at: None,
            };

            black_box(adapter.create_agent(&agent).await.unwrap());
        });
    });

    c.bench_function("sqlite_get_agent", |b| {
        let mut adapter = rt.block_on(SqliteAdapter::new(":memory:")).unwrap();
        rt.block_on(adapter.initialize(None)).unwrap();

        let agent_id = uuid::Uuid::new_v4();
        let character = Character {
            name: "BenchAgent".to_string(),
            bio: vec!["Benchmark test agent".to_string()],
            ..Default::default()
        };

        let agent = Agent {
            id: agent_id,
            name: "Benchmark Agent".to_string(),
            character: serde_json::to_value(&character).unwrap(),
            created_at: Some(chrono::Utc::now().timestamp()),
            updated_at: None,
        };

        rt.block_on(adapter.create_agent(&agent)).unwrap();

        b.to_async(&rt).iter(|| async {
            black_box(adapter.get_agent(agent_id).await.unwrap());
        });
    });

    c.bench_function("sqlite_update_agent", |b| {
        let mut adapter = rt.block_on(SqliteAdapter::new(":memory:")).unwrap();
        rt.block_on(adapter.initialize(None)).unwrap();

        let agent_id = uuid::Uuid::new_v4();
        let character = Character {
            name: "BenchAgent".to_string(),
            bio: vec!["Benchmark test agent".to_string()],
            ..Default::default()
        };

        let agent = Agent {
            id: agent_id,
            name: "Benchmark Agent".to_string(),
            character: serde_json::to_value(&character).unwrap(),
            created_at: Some(chrono::Utc::now().timestamp()),
            updated_at: None,
        };

        rt.block_on(adapter.create_agent(&agent)).unwrap();

        b.to_async(&rt).iter(|| async {
            let updated = Agent {
                id: agent_id,
                name: "Updated Agent".to_string(),
                character: serde_json::to_value(&character).unwrap(),
                created_at: Some(chrono::Utc::now().timestamp()),
                updated_at: Some(chrono::Utc::now().timestamp()),
            };
            black_box(adapter.update_agent(agent_id, &updated).await.unwrap());
        });
    });
}

/// Benchmark SQLite memory operations
fn bench_sqlite_memory_ops(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("sqlite_create_memory", |b| {
        b.to_async(&rt).iter(|| async {
            let mut adapter = SqliteAdapter::new(":memory:").await.unwrap();
            adapter.initialize(None).await.unwrap();

            let memory = Memory {
                id: uuid::Uuid::new_v4(),
                entity_id: uuid::Uuid::new_v4(),
                agent_id: uuid::Uuid::new_v4(),
                room_id: uuid::Uuid::new_v4(),
                content: Content {
                    text: "Benchmark memory".to_string(),
                    ..Default::default()
                },
                embedding: None,
                metadata: None,
                created_at: chrono::Utc::now().timestamp(),
                unique: Some(false),
                similarity: None,
            };

            black_box(adapter.create_memory(&memory, "memories").await.unwrap());
        });
    });

    c.bench_function("sqlite_query_memories", |b| {
        let mut adapter = rt.block_on(SqliteAdapter::new(":memory:")).unwrap();
        rt.block_on(adapter.initialize(None)).unwrap();

        let agent_id = uuid::Uuid::new_v4();

        // Create 100 memories
        for i in 0..100 {
            let memory = Memory {
                id: uuid::Uuid::new_v4(),
                entity_id: uuid::Uuid::new_v4(),
                agent_id,
                room_id: uuid::Uuid::new_v4(),
                content: Content {
                    text: format!("Memory {}", i),
                    ..Default::default()
                },
                embedding: None,
                metadata: None,
                created_at: chrono::Utc::now().timestamp(),
                unique: Some(false),
                similarity: None,
            };
            rt.block_on(adapter.create_memory(&memory, "memories"))
                .unwrap();
        }

        b.to_async(&rt).iter(|| async {
            let params = MemoryQuery {
                agent_id: Some(agent_id),
                room_id: None,
                entity_id: None,
                world_id: None,
                unique: None,
                count: Some(10),
                offset: None,
                table_name: "memories".to_string(),
                start: None,
                end: None,
            };
            black_box(adapter.get_memories(params).await.unwrap());
        });
    });
}

/// Benchmark SQLite vs PostgreSQL (PostgreSQL requires DATABASE_URL env var)
fn bench_database_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("database_comparison");

    // SQLite baseline
    group.bench_function("sqlite_100_memories", |b| {
        b.to_async(&rt).iter(|| async {
            let mut adapter = SqliteAdapter::new(":memory:").await.unwrap();
            adapter.initialize(None).await.unwrap();

            let agent_id = uuid::Uuid::new_v4();

            for i in 0..100 {
                let memory = Memory {
                    id: uuid::Uuid::new_v4(),
                    entity_id: uuid::Uuid::new_v4(),
                    agent_id,
                    room_id: uuid::Uuid::new_v4(),
                    content: Content {
                        text: format!("Memory {}", i),
                        ..Default::default()
                    },
                    embedding: None,
                    metadata: None,
                    created_at: chrono::Utc::now().timestamp(),
                    unique: Some(false),
                    similarity: None,
                };
                adapter.create_memory(&memory, "memories").await.unwrap();
            }
        });
    });

    // PostgreSQL comparison (only if DATABASE_URL is set)
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        group.bench_function("postgres_100_memories", |b| {
            b.to_async(&rt).iter(|| async {
                let mut adapter = PostgresAdapter::new(&database_url).await.unwrap();
                adapter.initialize(None).await.unwrap();

                let agent_id = uuid::Uuid::new_v4();

                for i in 0..100 {
                    let memory = Memory {
                        id: uuid::Uuid::new_v4(),
                        entity_id: uuid::Uuid::new_v4(),
                        agent_id,
                        room_id: uuid::Uuid::new_v4(),
                        content: Content {
                            text: format!("Memory {}", i),
                            ..Default::default()
                        },
                        embedding: None,
                        metadata: None,
                        created_at: chrono::Utc::now().timestamp(),
                        unique: Some(false),
                        similarity: None,
                    };
                    adapter.create_memory(&memory, "memories").await.unwrap();
                }

                // Cleanup
                adapter
                    .remove_all_memories(agent_id, "memories")
                    .await
                    .unwrap();
            });
        });
    }

    group.finish();
}

/// Benchmark vector search performance (requires PostgreSQL with pgvector)
fn bench_vector_search(c: &mut Criterion) {
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        let rt = Runtime::new().unwrap();

        c.bench_function("pgvector_search_384d", |b| {
            let mut adapter = rt.block_on(PostgresAdapter::new(&database_url)).unwrap();
            rt.block_on(adapter.initialize(None)).unwrap();
            rt.block_on(adapter.ensure_embedding_dimension(384))
                .unwrap();

            let agent_id = uuid::Uuid::new_v4();

            // Create 1000 memories with embeddings
            for i in 0..1000 {
                let memory = Memory {
                    id: uuid::Uuid::new_v4(),
                    entity_id: uuid::Uuid::new_v4(),
                    agent_id,
                    room_id: uuid::Uuid::new_v4(),
                    content: Content {
                        text: format!("Vector memory {}", i),
                        ..Default::default()
                    },
                    embedding: Some(vec![((i % 100) as f32) * 0.01; 384]),
                    metadata: None,
                    created_at: chrono::Utc::now().timestamp(),
                    unique: Some(false),
                    similarity: None,
                };
                rt.block_on(adapter.create_memory(&memory, "memories"))
                    .unwrap();
            }

            b.to_async(&rt).iter(|| async {
                let params = SearchMemoriesParams {
                    table_name: "memories".to_string(),
                    agent_id: Some(agent_id),
                    room_id: None,
                    world_id: None,
                    entity_id: None,
                    embedding: vec![0.5f32; 384],
                    count: 10,
                    unique: None,
                    threshold: None,
                };
                black_box(adapter.search_memories_by_embedding(params).await.unwrap());
            });

            // Cleanup
            rt.block_on(adapter.remove_all_memories(agent_id, "memories"))
                .unwrap();
        });
    }
}

criterion_group!(
    database_benches,
    bench_sqlite_agent_crud,
    bench_sqlite_memory_ops,
    bench_database_comparison,
    bench_vector_search,
);
criterion_main!(database_benches);
