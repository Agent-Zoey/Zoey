//! MongoDB database performance benchmarks
//!
//! Benchmarks MongoDB CRUD operations to validate performance.
//! Requires MONGODB_URL environment variable to be set.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zoey_core::*;
use zoey_storage_mongo::MongoAdapter;
use tokio::runtime::Runtime;

/// Benchmark MongoDB agent CRUD operations
fn bench_mongo_agent_crud(c: &mut Criterion) {
    let Some(mongodb_url) = std::env::var("MONGODB_URL").ok() else {
        eprintln!("MONGODB_URL not set, skipping MongoDB benchmarks");
        return;
    };

    let rt = Runtime::new().unwrap();

    c.bench_function("mongo_create_agent", |b| {
        b.to_async(&rt).iter(|| async {
            let db_name = format!("bench_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string());
            let mut adapter = MongoAdapter::new(&mongodb_url, &db_name).await.unwrap();
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

    c.bench_function("mongo_get_agent", |b| {
        let db_name = format!("bench_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string());
        let mut adapter = rt.block_on(MongoAdapter::new(&mongodb_url, &db_name)).unwrap();
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
}

/// Benchmark MongoDB memory operations
fn bench_mongo_memory_ops(c: &mut Criterion) {
    let Some(mongodb_url) = std::env::var("MONGODB_URL").ok() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    c.bench_function("mongo_create_memory", |b| {
        b.to_async(&rt).iter(|| async {
            let db_name = format!("bench_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string());
            let mut adapter = MongoAdapter::new(&mongodb_url, &db_name).await.unwrap();
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

    c.bench_function("mongo_query_memories", |b| {
        let db_name = format!("bench_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string());
        let mut adapter = rt.block_on(MongoAdapter::new(&mongodb_url, &db_name)).unwrap();
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

/// Benchmark batch operations
fn bench_mongo_batch_ops(c: &mut Criterion) {
    let Some(mongodb_url) = std::env::var("MONGODB_URL").ok() else {
        return;
    };

    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("mongo_batch");

    group.bench_function("mongo_100_memories_insert", |b| {
        b.to_async(&rt).iter(|| async {
            let db_name = format!("bench_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string());
            let mut adapter = MongoAdapter::new(&mongodb_url, &db_name).await.unwrap();
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

    group.finish();
}

criterion_group!(
    mongo_benches,
    bench_mongo_agent_crud,
    bench_mongo_memory_ops,
    bench_mongo_batch_ops,
);
criterion_main!(mongo_benches);
