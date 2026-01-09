//! Supabase database performance benchmarks
//!
//! Benchmarks Supabase CRUD operations to validate performance.
//! Requires SUPABASE_URL and SUPABASE_ANON_KEY environment variables.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zoey_core::*;
use zoey_storage_supabase::{SupabaseAdapter, supabase::SupabaseConfig};
use tokio::runtime::Runtime;

/// Benchmark Supabase agent CRUD operations
fn bench_supabase_agent_crud(c: &mut Criterion) {
    let supabase_url = match std::env::var("SUPABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("SUPABASE_URL not set, skipping Supabase benchmarks");
            return;
        }
    };

    let supabase_key = match std::env::var("SUPABASE_ANON_KEY") {
        Ok(key) => key,
        Err(_) => {
            eprintln!("SUPABASE_ANON_KEY not set, skipping Supabase benchmarks");
            return;
        }
    };

    let rt = Runtime::new().unwrap();

    c.bench_function("supabase_create_agent", |b| {
        b.to_async(&rt).iter(|| async {
            let config = SupabaseConfig::new(&supabase_url, &supabase_key);
            let mut adapter = SupabaseAdapter::new(config).await.unwrap();
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
            
            // Cleanup
            adapter.delete_agent(agent_id).await.ok();
        });
    });

    c.bench_function("supabase_get_agent", |b| {
        let config = SupabaseConfig::new(&supabase_url, &supabase_key);
        let mut adapter = rt.block_on(SupabaseAdapter::new(config)).unwrap();
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

        // Cleanup
        rt.block_on(adapter.delete_agent(agent_id)).ok();
    });
}

/// Benchmark Supabase memory operations
fn bench_supabase_memory_ops(c: &mut Criterion) {
    let supabase_url = match std::env::var("SUPABASE_URL") {
        Ok(url) => url,
        Err(_) => return,
    };
    let supabase_key = match std::env::var("SUPABASE_ANON_KEY") {
        Ok(key) => key,
        Err(_) => return,
    };

    let rt = Runtime::new().unwrap();

    c.bench_function("supabase_query_memories", |b| {
        let config = SupabaseConfig::new(&supabase_url, &supabase_key);
        let mut adapter = rt.block_on(SupabaseAdapter::new(config)).unwrap();
        rt.block_on(adapter.initialize(None)).unwrap();

        let agent_id = uuid::Uuid::new_v4();

        // Create test agent
        let character = Character {
            name: "BenchAgent".to_string(),
            bio: vec!["Benchmark test agent".to_string()],
            ..Default::default()
        };

        let agent = Agent {
            id: agent_id,
            name: "Memory Benchmark Agent".to_string(),
            character: serde_json::to_value(&character).unwrap(),
            created_at: Some(chrono::Utc::now().timestamp()),
            updated_at: None,
        };

        rt.block_on(adapter.create_agent(&agent)).unwrap();

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

        // Cleanup
        rt.block_on(adapter.delete_agent(agent_id)).ok();
    });
}

/// Benchmark REST API latency
fn bench_supabase_api_latency(c: &mut Criterion) {
    let supabase_url = match std::env::var("SUPABASE_URL") {
        Ok(url) => url,
        Err(_) => return,
    };
    let supabase_key = match std::env::var("SUPABASE_ANON_KEY") {
        Ok(key) => key,
        Err(_) => return,
    };

    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("supabase_api");

    group.bench_function("health_check", |b| {
        let config = SupabaseConfig::new(&supabase_url, &supabase_key);
        let adapter = rt.block_on(SupabaseAdapter::new(config)).unwrap();

        b.to_async(&rt).iter(|| async {
            black_box(adapter.is_ready().await.unwrap());
        });
    });

    group.bench_function("get_agents", |b| {
        let config = SupabaseConfig::new(&supabase_url, &supabase_key);
        let adapter = rt.block_on(SupabaseAdapter::new(config)).unwrap();

        b.to_async(&rt).iter(|| async {
            black_box(adapter.get_agents().await.unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    supabase_benches,
    bench_supabase_agent_crud,
    bench_supabase_memory_ops,
    bench_supabase_api_latency,
);
criterion_main!(supabase_benches);
