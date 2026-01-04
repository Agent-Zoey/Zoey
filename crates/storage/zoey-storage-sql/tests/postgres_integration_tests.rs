//! Integration tests for PostgreSQL adapter with real database operations
//!
//! These tests require a running PostgreSQL instance.
//!
//! Setup:
//! 1. Start PostgreSQL: `docker run -d -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:15`
//! 2. Run tests: `cargo test --test postgres_integration_tests -- --ignored --nocapture`
//!
//! Or use environment variable:
//! ```
//! export DATABASE_URL="postgresql://postgres:postgres@localhost:5432/zoey_test"
//! cargo test --test postgres_integration_tests -- --ignored
//! ```

use zoey_core::*;
use zoey_storage_sql::PostgresAdapter;
use std::sync::Arc;

/// Get database URL from environment or use default
fn get_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/zoey_test".to_string())
}

#[tokio::test]
#[ignore = "Integration test - requires PostgreSQL running"]
async fn test_postgres_agent_crud() {
    let mut adapter = PostgresAdapter::new(&get_database_url()).await.unwrap();
    adapter.initialize(None).await.unwrap();

    // Create a test agent
    let agent_id = uuid::Uuid::new_v4();
    let character = Character {
        name: "TestAgent".to_string(),
        bio: vec!["A test agent for PostgreSQL".to_string()],
        ..Default::default()
    };

    let agent = Agent {
        id: agent_id,
        name: "Test Agent PG".to_string(),
        character: serde_json::to_value(&character).unwrap(),
        created_at: Some(chrono::Utc::now().timestamp()),
        updated_at: None,
    };

    // Test create
    let created = adapter.create_agent(&agent).await.unwrap();
    assert!(created, "Agent should be created");

    // Test get
    let retrieved = adapter.get_agent(agent_id).await.unwrap();
    assert!(retrieved.is_some(), "Agent should be retrievable");
    let retrieved_agent = retrieved.unwrap();
    assert_eq!(retrieved_agent.id, agent_id);
    assert_eq!(retrieved_agent.name, "Test Agent PG");

    // Test get_agents
    let agents = adapter.get_agents().await.unwrap();
    assert!(!agents.is_empty(), "Should have at least one agent");

    // Test update
    let mut updated_agent = agent.clone();
    updated_agent.name = "Updated Agent PG".to_string();
    let updated = adapter
        .update_agent(agent_id, &updated_agent)
        .await
        .unwrap();
    assert!(updated, "Agent should be updated");

    let retrieved_updated = adapter.get_agent(agent_id).await.unwrap().unwrap();
    assert_eq!(retrieved_updated.name, "Updated Agent PG");

    // Test delete
    let deleted = adapter.delete_agent(agent_id).await.unwrap();
    assert!(deleted, "Agent should be deleted");

    let retrieved_deleted = adapter.get_agent(agent_id).await.unwrap();
    assert!(
        retrieved_deleted.is_none(),
        "Deleted agent should not be found"
    );
}

#[tokio::test]
#[ignore = "Integration test - requires PostgreSQL running"]
async fn test_postgres_entity_crud() {
    let mut adapter = PostgresAdapter::new(&get_database_url()).await.unwrap();
    adapter.initialize(None).await.unwrap();

    let entity_id = uuid::Uuid::new_v4();
    let agent_id = uuid::Uuid::new_v4();

    let entity = Entity {
        id: entity_id,
        agent_id,
        name: Some("Test Entity PG".to_string()),
        username: Some("testuser_pg".to_string()),
        email: Some("test.pg@example.com".to_string()),
        avatar_url: None,
        metadata: std::collections::HashMap::new(),
        created_at: Some(chrono::Utc::now().timestamp()),
    };

    // Test create
    let created = adapter.create_entities(vec![entity.clone()]).await.unwrap();
    assert!(created, "Entity should be created");

    // Test get by ID
    let retrieved = adapter.get_entity_by_id(entity_id).await.unwrap();
    assert!(retrieved.is_some(), "Entity should be retrievable");
    let retrieved_entity = retrieved.unwrap();
    assert_eq!(retrieved_entity.id, entity_id);
    assert_eq!(retrieved_entity.name, Some("Test Entity PG".to_string()));
    assert_eq!(retrieved_entity.username, Some("testuser_pg".to_string()));
    assert_eq!(
        retrieved_entity.email,
        Some("test.pg@example.com".to_string())
    );

    // Test get by IDs
    let entities = adapter.get_entities_by_ids(vec![entity_id]).await.unwrap();
    assert_eq!(entities.len(), 1);

    // Clean up - PostgreSQL doesn't have entity delete in adapter yet, so we'll leave it
}

#[tokio::test]
#[ignore = "Integration test - requires PostgreSQL running"]
async fn test_postgres_memory_crud() {
    let mut adapter = PostgresAdapter::new(&get_database_url()).await.unwrap();
    adapter.initialize(None).await.unwrap();

    let memory_id = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();
    let agent_id = uuid::Uuid::new_v4();
    let room_id = uuid::Uuid::new_v4();

    let memory = Memory {
        id: memory_id,
        entity_id,
        agent_id,
        room_id,
        content: Content {
            text: "Test message PG".to_string(),
            ..Default::default()
        },
        embedding: None,
        metadata: None,
        created_at: chrono::Utc::now().timestamp(),
        unique: Some(false),
        similarity: None,
    };

    // Test create
    let created_id = adapter.create_memory(&memory, "memories").await.unwrap();
    assert_eq!(created_id, memory_id, "Created memory ID should match");

    // Test get memories with filters
    let params = MemoryQuery {
        agent_id: Some(agent_id),
        room_id: None,
        entity_id: None,
        world_id: None,
        unique: None,
        count: None,
        offset: None,
        table_name: "memories".to_string(),
        start: None,
        end: None,
    };

    let memories = adapter.get_memories(params).await.unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].content.text, "Test message PG");

    // Test count
    let count_params = MemoryQuery {
        agent_id: Some(agent_id),
        room_id: None,
        entity_id: None,
        world_id: None,
        unique: None,
        count: None,
        offset: None,
        table_name: "memories".to_string(),
        start: None,
        end: None,
    };
    let count = adapter.count_memories(count_params).await.unwrap();
    assert_eq!(count, 1);

    // Test update
    let mut updated_memory = memory.clone();
    updated_memory.content.text = "Updated message PG".to_string();
    let updated = adapter.update_memory(&updated_memory).await.unwrap();
    assert!(updated, "Memory should be updated");

    let params = MemoryQuery {
        agent_id: Some(agent_id),
        room_id: None,
        entity_id: None,
        world_id: None,
        unique: None,
        count: None,
        offset: None,
        table_name: "memories".to_string(),
        start: None,
        end: None,
    };
    let retrieved_memories = adapter.get_memories(params).await.unwrap();
    assert_eq!(retrieved_memories[0].content.text, "Updated message PG");

    // Test remove single memory
    let removed = adapter.remove_memory(memory_id, "memories").await.unwrap();
    assert!(removed, "Memory should be removed");

    let count_after = adapter
        .count_memories(MemoryQuery {
            agent_id: Some(agent_id),
            room_id: None,
            entity_id: None,
            world_id: None,
            unique: None,
            count: None,
            offset: None,
            table_name: "memories".to_string(),
            start: None,
            end: None,
        })
        .await
        .unwrap();
    assert_eq!(count_after, 0);
}

#[tokio::test]
#[ignore = "Integration test - requires PostgreSQL running"]
async fn test_postgres_memory_filtering() {
    let mut adapter = PostgresAdapter::new(&get_database_url()).await.unwrap();
    adapter.initialize(None).await.unwrap();

    let agent_id = uuid::Uuid::new_v4();
    let room1_id = uuid::Uuid::new_v4();
    let room2_id = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();

    // Create memories in different rooms
    for i in 0..5 {
        let room_id = if i < 3 { room1_id } else { room2_id };
        let memory = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id,
            agent_id,
            room_id,
            content: Content {
                text: format!("PG Message {}", i),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp() + i,
            unique: Some(i % 2 == 0),
            similarity: None,
        };
        adapter.create_memory(&memory, "memories").await.unwrap();
    }

    // Test filter by room
    let room1_memories = adapter
        .get_memories(MemoryQuery {
            agent_id: None,
            room_id: Some(room1_id),
            entity_id: None,
            world_id: None,
            unique: None,
            count: None,
            offset: None,
            table_name: "memories".to_string(),
            start: None,
            end: None,
        })
        .await
        .unwrap();
    assert_eq!(room1_memories.len(), 3);

    // Test filter by unique flag
    let unique_memories = adapter
        .get_memories(MemoryQuery {
            agent_id: Some(agent_id),
            room_id: None,
            entity_id: None,
            world_id: None,
            unique: Some(true),
            count: None,
            offset: None,
            table_name: "memories".to_string(),
            start: None,
            end: None,
        })
        .await
        .unwrap();
    assert_eq!(unique_memories.len(), 3); // 0, 2, 4

    // Test limit
    let limited_memories = adapter
        .get_memories(MemoryQuery {
            agent_id: Some(agent_id),
            room_id: None,
            entity_id: None,
            world_id: None,
            unique: None,
            count: Some(2),
            offset: None,
            table_name: "memories".to_string(),
            start: None,
            end: None,
        })
        .await
        .unwrap();
    assert_eq!(limited_memories.len(), 2);

    // Test remove all for agent
    let removed_all = adapter
        .remove_all_memories(agent_id, "memories")
        .await
        .unwrap();
    assert!(removed_all, "All memories should be removed");

    let remaining = adapter
        .count_memories(MemoryQuery {
            agent_id: Some(agent_id),
            room_id: None,
            entity_id: None,
            world_id: None,
            unique: None,
            count: None,
            offset: None,
            table_name: "memories".to_string(),
            start: None,
            end: None,
        })
        .await
        .unwrap();
    assert_eq!(remaining, 0);
}

#[tokio::test]
#[ignore = "Integration test - requires PostgreSQL running"]
async fn test_postgres_with_runtime() {
    let adapter = PostgresAdapter::new(&get_database_url()).await.unwrap();

    let character = Character {
        name: "TestBotPG".to_string(),
        bio: vec!["A test bot for PostgreSQL integration testing".to_string()],
        ..Default::default()
    };

    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(character),
        adapter: Some(Arc::new(adapter)),
        plugins: vec![],
        ..Default::default()
    })
    .await
    .unwrap();

    {
        let mut rt = runtime.write().unwrap();
        rt.initialize(InitializeOptions::default()).await.unwrap();
    }

    let rt = runtime.read().unwrap();
    assert_eq!(rt.character.name, "TestBotPG");
    // Runtime initialized successfully with adapter
}

#[tokio::test]
#[ignore = "Integration test - requires PostgreSQL running"]
async fn test_postgres_concurrent_operations() {
    let adapter = Arc::new(PostgresAdapter::new(&get_database_url()).await.unwrap());

    // Test concurrent agent creations
    let mut handles = vec![];

    for i in 0..5 {
        let adapter_clone = adapter.clone();
        let handle = tokio::spawn(async move {
            let agent_id = uuid::Uuid::new_v4();
            let character = Character {
                name: format!("ConcurrentAgent{}", i),
                bio: vec![format!("Concurrent test agent {}", i)],
                ..Default::default()
            };

            let agent = Agent {
                id: agent_id,
                name: format!("Concurrent Agent {}", i),
                character: serde_json::to_value(&character).unwrap(),
                created_at: Some(chrono::Utc::now().timestamp()),
                updated_at: None,
            };

            adapter_clone.create_agent(&agent).await.unwrap();
            agent_id
        });
        handles.push(handle);
    }

    // Wait for all to complete
    let agent_ids: Vec<uuid::Uuid> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(agent_ids.len(), 5);

    // Verify all agents were created
    for agent_id in &agent_ids {
        let agent = adapter.get_agent(*agent_id).await.unwrap();
        assert!(agent.is_some());
    }

    // Cleanup
    for agent_id in agent_ids {
        adapter.delete_agent(agent_id).await.unwrap();
    }
}

#[tokio::test]
#[ignore = "Integration test - requires PostgreSQL running with pgvector extension"]
async fn test_postgres_vector_search() {
    let mut adapter = PostgresAdapter::new(&get_database_url()).await.unwrap();
    adapter.initialize(None).await.unwrap();

    // Initialize vector extension with 384 dimensions (common for sentence transformers)
    adapter.ensure_embedding_dimension(384).await.unwrap();

    let agent_id = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();
    let room_id = uuid::Uuid::new_v4();

    // Create memories with embeddings
    let memories_data = vec![
        ("Hello, how are you?", vec![0.1f32; 384]),
        ("I'm doing great, thanks!", vec![0.2f32; 384]),
        ("What's the weather like?", vec![0.3f32; 384]),
        ("It's sunny today", vec![0.4f32; 384]),
    ];

    for (text, embedding) in &memories_data {
        let memory = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id,
            agent_id,
            room_id,
            content: Content {
                text: text.to_string(),
                ..Default::default()
            },
            embedding: Some(embedding.clone()),
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: Some(false),
            similarity: None,
        };
        adapter.create_memory(&memory, "memories").await.unwrap();
    }

    // Test vector search with query embedding similar to first memory
    let query_embedding = vec![0.15f32; 384];
    let search_params = SearchMemoriesParams {
        table_name: "memories".to_string(),
        agent_id: Some(agent_id),
        room_id: None,
        world_id: None,
        entity_id: None,
        embedding: query_embedding,
        count: 2,
        unique: None,
        threshold: None,
    };

    let results = adapter
        .search_memories_by_embedding(search_params)
        .await
        .unwrap();
    assert_eq!(results.len(), 2, "Should return top 2 similar memories");
    assert!(
        results[0].similarity.is_some(),
        "Should include similarity score"
    );

    // Verify ordering (most similar first)
    if let (Some(sim1), Some(sim2)) = (
        results[0].similarity,
        results.get(1).and_then(|r| r.similarity),
    ) {
        assert!(
            sim1 <= sim2,
            "Results should be ordered by similarity (ascending distance)"
        );
    }

    // Test dimension mismatch error
    let wrong_dimension_params = SearchMemoriesParams {
        table_name: "memories".to_string(),
        agent_id: Some(agent_id),
        room_id: None,
        world_id: None,
        entity_id: None,
        embedding: vec![0.1f32; 128], // Wrong dimension
        count: 2,
        unique: None,
        threshold: None,
    };

    let result = adapter
        .search_memories_by_embedding(wrong_dimension_params)
        .await;
    assert!(
        result.is_err(),
        "Should fail with wrong embedding dimension"
    );
    if let Err(e) = result {
        assert!(
            e.to_string().contains("dimension"),
            "Error should mention dimension mismatch"
        );
    }

    // Cleanup
    adapter
        .remove_all_memories(agent_id, "memories")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "Integration test - requires PostgreSQL running"]
async fn test_postgres_cached_embeddings() {
    let mut adapter = PostgresAdapter::new(&get_database_url()).await.unwrap();
    adapter.initialize(None).await.unwrap();

    let agent_id = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();
    let room_id = uuid::Uuid::new_v4();

    // Create memories with and without embeddings
    for i in 0..5 {
        let has_embedding = i % 2 == 0;
        let memory = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id,
            agent_id,
            room_id,
            content: Content {
                text: format!("Message {}", i),
                ..Default::default()
            },
            embedding: if has_embedding {
                Some(vec![0.1f32; 384])
            } else {
                None
            },
            metadata: None,
            created_at: chrono::Utc::now().timestamp() + i,
            unique: Some(false),
            similarity: None,
        };
        adapter.create_memory(&memory, "memories").await.unwrap();
    }

    // Get only memories with embeddings
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

    let cached = adapter.get_cached_embeddings(params).await.unwrap();
    assert_eq!(
        cached.len(),
        3,
        "Should return only memories with embeddings (3 out of 5)"
    );

    for memory in &cached {
        assert!(
            memory.embedding.is_some(),
            "All returned memories should have embeddings"
        );
    }

    // Cleanup
    adapter
        .remove_all_memories(agent_id, "memories")
        .await
        .unwrap();
}
