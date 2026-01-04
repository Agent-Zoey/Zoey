//! Integration tests for SQLite adapter with real database operations

use zoey_core::*;
use zoey_storage_sql::SqliteAdapter;
use std::sync::Arc;

#[tokio::test]
async fn test_sqlite_agent_crud() {
    // Use in-memory database for testing
    let mut adapter = SqliteAdapter::new(":memory:").await.unwrap();
    adapter.initialize(None).await.unwrap();

    // Create a test agent
    let agent_id = uuid::Uuid::new_v4();
    let character = Character {
        name: "TestAgent".to_string(),
        bio: vec!["A test agent".to_string()],
        ..Default::default()
    };

    let agent = Agent {
        id: agent_id,
        name: "Test Agent".to_string(),
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
    assert_eq!(retrieved_agent.name, "Test Agent");

    // Test get_agents
    let agents = adapter.get_agents().await.unwrap();
    assert_eq!(agents.len(), 1);

    // Test update
    let mut updated_agent = agent.clone();
    updated_agent.name = "Updated Agent".to_string();
    let updated = adapter
        .update_agent(agent_id, &updated_agent)
        .await
        .unwrap();
    assert!(updated, "Agent should be updated");

    let retrieved_updated = adapter.get_agent(agent_id).await.unwrap().unwrap();
    assert_eq!(retrieved_updated.name, "Updated Agent");

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
async fn test_sqlite_entity_crud() {
    let mut adapter = SqliteAdapter::new(":memory:").await.unwrap();
    adapter.initialize(None).await.unwrap();

    let entity_id = uuid::Uuid::new_v4();
    let agent_id = uuid::Uuid::new_v4();

    let entity = Entity {
        id: entity_id,
        agent_id,
        name: Some("Test Entity".to_string()),
        username: Some("testuser".to_string()),
        email: Some("test@example.com".to_string()),
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
    assert_eq!(retrieved_entity.name, Some("Test Entity".to_string()));
    assert_eq!(retrieved_entity.username, Some("testuser".to_string()));

    // Test get by IDs
    let entities = adapter.get_entities_by_ids(vec![entity_id]).await.unwrap();
    assert_eq!(entities.len(), 1);

    // Test update
    let mut updated_entity = entity.clone();
    updated_entity.name = Some("Updated Entity".to_string());
    adapter.update_entity(&updated_entity).await.unwrap();

    let retrieved_updated = adapter.get_entity_by_id(entity_id).await.unwrap().unwrap();
    assert_eq!(retrieved_updated.name, Some("Updated Entity".to_string()));
}

#[tokio::test]
async fn test_sqlite_memory_crud() {
    let mut adapter = SqliteAdapter::new(":memory:").await.unwrap();
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
            text: "Test message".to_string(),
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
    assert_eq!(memories[0].content.text, "Test message");

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
    updated_memory.content.text = "Updated message".to_string();
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
    assert_eq!(retrieved_memories[0].content.text, "Updated message");

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
async fn test_sqlite_memory_filtering() {
    let mut adapter = SqliteAdapter::new(":memory:").await.unwrap();
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
                text: format!("Message {}", i),
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
async fn test_sqlite_with_runtime() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();

    let character = Character {
        name: "TestBot".to_string(),
        bio: vec!["A test bot for integration testing".to_string()],
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
    assert_eq!(rt.character.name, "TestBot");
    // Runtime initialized successfully with adapter
}
