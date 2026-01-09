//! MongoDB integration tests
//!
//! These tests require a running MongoDB instance.
//! Set MONGODB_URL environment variable to run these tests.
//!
//! Run with: cargo test -p zoey-storage-mongo --test mongo_integration_tests -- --ignored

use zoey_core::types::*;
use zoey_storage_mongo::MongoAdapter;

async fn setup_adapter() -> Option<MongoAdapter> {
    let mongodb_url = std::env::var("MONGODB_URL").ok()?;
    let db_name = format!("zoey_test_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string());
    
    match MongoAdapter::new(&mongodb_url, &db_name).await {
        Ok(mut adapter) => {
            adapter.initialize(None).await.ok()?;
            Some(adapter)
        }
        Err(_) => None,
    }
}

#[tokio::test]
#[ignore = "Requires MongoDB instance"]
async fn test_agent_crud() {
    let Some(adapter) = setup_adapter().await else {
        eprintln!("Skipping test - MongoDB not available");
        return;
    };

    let agent_id = uuid::Uuid::new_v4();
    let character = serde_json::json!({
        "name": "TestAgent",
        "bio": ["A test agent for integration testing"]
    });

    let agent = Agent {
        id: agent_id,
        name: "Test Agent".to_string(),
        character,
        created_at: Some(chrono::Utc::now().timestamp()),
        updated_at: None,
    };

    // Create
    let created = adapter.create_agent(&agent).await.unwrap();
    assert!(created);

    // Read
    let retrieved = adapter.get_agent(agent_id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.name, "Test Agent");

    // Update
    let updated_agent = Agent {
        id: agent_id,
        name: "Updated Agent".to_string(),
        character: serde_json::json!({"name": "UpdatedAgent"}),
        created_at: agent.created_at,
        updated_at: Some(chrono::Utc::now().timestamp()),
    };
    let updated = adapter.update_agent(agent_id, &updated_agent).await.unwrap();
    assert!(updated);

    // Verify update
    let retrieved = adapter.get_agent(agent_id).await.unwrap().unwrap();
    assert_eq!(retrieved.name, "Updated Agent");

    // Delete
    let deleted = adapter.delete_agent(agent_id).await.unwrap();
    assert!(deleted);

    // Verify deletion
    let retrieved = adapter.get_agent(agent_id).await.unwrap();
    assert!(retrieved.is_none());
}

#[tokio::test]
#[ignore = "Requires MongoDB instance"]
async fn test_memory_crud() {
    let Some(adapter) = setup_adapter().await else {
        eprintln!("Skipping test - MongoDB not available");
        return;
    };

    let agent_id = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();
    let room_id = uuid::Uuid::new_v4();
    let memory_id = uuid::Uuid::new_v4();

    let memory = Memory {
        id: memory_id,
        entity_id,
        agent_id,
        room_id,
        content: Content {
            text: "Test memory content".to_string(),
            ..Default::default()
        },
        embedding: None,
        metadata: None,
        created_at: chrono::Utc::now().timestamp(),
        unique: Some(false),
        similarity: None,
    };

    // Create
    let created_id = adapter.create_memory(&memory, "memories").await.unwrap();
    assert_eq!(created_id, memory_id);

    // Read
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
    let memories = adapter.get_memories(params).await.unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].content.text, "Test memory content");

    // Count
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

    // Delete
    let deleted = adapter.remove_memory(memory_id, "memories").await.unwrap();
    assert!(deleted);
}

#[tokio::test]
#[ignore = "Requires MongoDB instance"]
async fn test_entity_operations() {
    let Some(adapter) = setup_adapter().await else {
        eprintln!("Skipping test - MongoDB not available");
        return;
    };

    let agent_id = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();

    let entity = Entity {
        id: entity_id,
        agent_id,
        name: Some("Test Entity".to_string()),
        username: Some("testuser".to_string()),
        email: Some("test@example.com".to_string()),
        avatar_url: None,
        metadata: std::collections::HashMap::new(),
        created_at: chrono::Utc::now().timestamp(),
    };

    // Create
    let created = adapter.create_entities(vec![entity.clone()]).await.unwrap();
    assert!(created);

    // Read
    let retrieved = adapter.get_entity_by_id(entity_id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.name, Some("Test Entity".to_string()));

    // Update
    let updated_entity = Entity {
        id: entity_id,
        agent_id,
        name: Some("Updated Entity".to_string()),
        username: Some("updateduser".to_string()),
        email: entity.email.clone(),
        avatar_url: None,
        metadata: std::collections::HashMap::new(),
        created_at: entity.created_at,
    };
    adapter.update_entity(&updated_entity).await.unwrap();

    // Verify update
    let retrieved = adapter.get_entity_by_id(entity_id).await.unwrap().unwrap();
    assert_eq!(retrieved.name, Some("Updated Entity".to_string()));
}

#[tokio::test]
#[ignore = "Requires MongoDB instance"]
async fn test_world_and_room() {
    let Some(adapter) = setup_adapter().await else {
        eprintln!("Skipping test - MongoDB not available");
        return;
    };

    let agent_id = uuid::Uuid::new_v4();
    let world_id = uuid::Uuid::new_v4();
    let room_id = uuid::Uuid::new_v4();

    // Create world
    let world = World {
        id: world_id,
        name: "Test World".to_string(),
        agent_id,
        server_id: Some("test_server".to_string()),
        metadata: std::collections::HashMap::new(),
        created_at: Some(chrono::Utc::now().timestamp()),
    };
    adapter.ensure_world(&world).await.unwrap();

    // Verify world
    let retrieved_world = adapter.get_world(world_id).await.unwrap();
    assert!(retrieved_world.is_some());
    assert_eq!(retrieved_world.unwrap().name, "Test World");

    // Create room
    let room = Room {
        id: room_id,
        agent_id: Some(agent_id),
        name: "Test Room".to_string(),
        source: "test".to_string(),
        channel_type: ChannelType::GuildText,
        channel_id: Some("123".to_string()),
        server_id: Some("test_server".to_string()),
        world_id,
        metadata: std::collections::HashMap::new(),
        created_at: Some(chrono::Utc::now().timestamp()),
    };
    let created_room_id = adapter.create_room(&room).await.unwrap();
    assert_eq!(created_room_id, room_id);

    // Verify room
    let retrieved_room = adapter.get_room(room_id).await.unwrap();
    assert!(retrieved_room.is_some());
    assert_eq!(retrieved_room.unwrap().name, "Test Room");

    // Get rooms for world
    let rooms = adapter.get_rooms(world_id).await.unwrap();
    assert_eq!(rooms.len(), 1);
}

#[tokio::test]
#[ignore = "Requires MongoDB instance"]
async fn test_participants() {
    let Some(adapter) = setup_adapter().await else {
        eprintln!("Skipping test - MongoDB not available");
        return;
    };

    let entity_id = uuid::Uuid::new_v4();
    let room_id = uuid::Uuid::new_v4();

    // Add participant
    let added = adapter.add_participant(entity_id, room_id).await.unwrap();
    assert!(added);

    // Get participants
    let participants = adapter.get_participants(room_id).await.unwrap();
    assert_eq!(participants.len(), 1);
    assert_eq!(participants[0].entity_id, entity_id);

    // Remove participant
    let removed = adapter.remove_participant(entity_id, room_id).await.unwrap();
    assert!(removed);

    // Verify removal
    let participants = adapter.get_participants(room_id).await.unwrap();
    assert!(participants.is_empty());
}

#[tokio::test]
#[ignore = "Requires MongoDB instance"]
async fn test_tasks() {
    let Some(adapter) = setup_adapter().await else {
        eprintln!("Skipping test - MongoDB not available");
        return;
    };

    let agent_id = uuid::Uuid::new_v4();
    let task_id = uuid::Uuid::new_v4();

    let task = Task {
        id: task_id,
        agent_id,
        task_type: "test_task".to_string(),
        data: serde_json::json!({"key": "value"}),
        status: TaskStatus::Pending,
        priority: 5,
        scheduled_at: Some(chrono::Utc::now().timestamp()),
        executed_at: None,
        retry_count: 0,
        max_retries: 3,
        error: None,
        created_at: chrono::Utc::now().timestamp(),
        updated_at: None,
    };

    // Create
    let created_id = adapter.create_task(&task).await.unwrap();
    assert_eq!(created_id, task_id);

    // Read
    let retrieved = adapter.get_task(task_id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.task_type, "test_task");
    assert!(matches!(retrieved.status, TaskStatus::Pending));

    // Get pending tasks
    let pending = adapter.get_pending_tasks(agent_id).await.unwrap();
    assert_eq!(pending.len(), 1);

    // Update
    let updated_task = Task {
        status: TaskStatus::Completed,
        executed_at: Some(chrono::Utc::now().timestamp()),
        ..task
    };
    let updated = adapter.update_task(&updated_task).await.unwrap();
    assert!(updated);

    // Verify update
    let retrieved = adapter.get_task(task_id).await.unwrap().unwrap();
    assert!(matches!(retrieved.status, TaskStatus::Completed));

    // Pending tasks should now be empty
    let pending = adapter.get_pending_tasks(agent_id).await.unwrap();
    assert!(pending.is_empty());
}
