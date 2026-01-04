//! Testing utilities and framework

use crate::types::*;
use crate::{AgentRuntime, RuntimeOpts};
use std::sync::{Arc, RwLock};

/// Run a test suite
pub async fn run_test_suite(suite: &TestSuite, runtime: Arc<RwLock<AgentRuntime>>) -> TestResults {
    let mut results = TestResults::new();

    for test in &suite.tests {
        tracing::info!("Running test: {}", test.name());

        match test.run(Arc::new(runtime.clone())).await {
            Ok(()) => {
                results.passed.push(test.name().to_string());
                tracing::info!("✓ Test passed: {}", test.name());
            }
            Err(e) => {
                results
                    .failed
                    .push((test.name().to_string(), e.to_string()));
                tracing::error!("✗ Test failed: {} - {}", test.name(), e);
            }
        }
    }

    results
}

/// Mock runtime for testing
pub fn create_mock_runtime() -> Arc<RwLock<AgentRuntime>> {
    let opts = RuntimeOpts {
        character: Some(Character {
            name: "TestBot".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };

    // Use single-thread runtime to avoid nested multi-thread runtime issues in tests
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { AgentRuntime::new(opts).await.unwrap() })
}

/// Create test memory
pub fn create_test_memory(text: &str) -> Memory {
    Memory {
        id: uuid::Uuid::new_v4(),
        entity_id: uuid::Uuid::new_v4(),
        agent_id: uuid::Uuid::new_v4(),
        room_id: uuid::Uuid::new_v4(),
        content: Content {
            text: text.to_string(),
            ..Default::default()
        },
        embedding: None,
        metadata: None,
        created_at: chrono::Utc::now().timestamp(),
        unique: None,
        similarity: None,
    }
}

/// Create test room
pub fn create_test_room(channel_type: ChannelType) -> Room {
    Room {
        id: uuid::Uuid::new_v4(),
        agent_id: None,
        name: "Test Room".to_string(),
        source: "test".to_string(),
        channel_type,
        channel_id: None,
        server_id: None,
        world_id: uuid::Uuid::new_v4(),
        metadata: std::collections::HashMap::new(),
        created_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_memory() {
        let memory = create_test_memory("Test message");
        assert_eq!(memory.content.text, "Test message");
    }

    #[test]
    fn test_create_test_room() {
        let room = create_test_room(ChannelType::Dm);
        assert_eq!(room.channel_type, ChannelType::Dm);
        assert_eq!(room.name, "Test Room");
    }
}
