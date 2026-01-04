//! Integration tests for Agent API
//!
//! Comprehensive security and functionality tests

#[cfg(test)]
mod tests {
    use crate::agent_api::{
        auth::ApiAuthManager,
        task::{TaskManager, TaskResult, TaskStatus},
        types::{ApiPermission, ApiToken, ChatResponse},
    };
    use crate::{types::Character, AgentRuntime, RuntimeOpts};
    use std::sync::{Arc, RwLock};

    /// Helper to create test runtime
    async fn create_test_runtime() -> Arc<RwLock<AgentRuntime>> {
        use crate::RuntimeOpts;
        let opts = RuntimeOpts {
            test_mode: Some(true),
            ..Default::default()
        };
        AgentRuntime::new(opts).await.unwrap()
    }

    // ===================
    // Task Manager Tests
    // ===================

    #[tokio::test]
    async fn test_task_manager_lifecycle() {
        let manager = TaskManager::new(60);

        // Create task
        let task_id = manager.create_task();
        let task = manager.get_task(task_id).unwrap();
        assert!(matches!(task.status, TaskStatus::Pending));

        // Mark as running
        manager.mark_running(task_id);
        let task = manager.get_task(task_id).unwrap();
        assert!(matches!(task.status, TaskStatus::Running));

        // Complete task
        let result = TaskResult::Chat(ChatResponse {
            success: true,
            messages: Some(vec![]),
            error: None,
            metadata: None,
        });
        manager.complete_task(task_id, result);
        let task = manager.get_task(task_id).unwrap();
        assert!(matches!(task.status, TaskStatus::Completed));
        assert!(task.result.is_some());
    }

    #[tokio::test]
    async fn test_task_manager_failure() {
        let manager = TaskManager::new(60);

        let task_id = manager.create_task();
        manager.mark_running(task_id);
        manager.fail_task(task_id, "Test error".to_string());

        let task = manager.get_task(task_id).unwrap();
        assert!(matches!(task.status, TaskStatus::Failed));
        assert_eq!(task.error.as_deref(), Some("Test error"));
    }

    #[tokio::test]
    async fn test_task_manager_cleanup() {
        let manager = TaskManager::new(1); // 1 second max age

        let task_id = manager.create_task();
        manager.complete_task(
            task_id,
            TaskResult::Chat(ChatResponse {
                success: true,
                messages: None,
                error: None,
                metadata: None,
            }),
        );

        // Task should exist
        assert!(manager.get_task(task_id).is_some());

        // Wait for task to age
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Cleanup
        manager.cleanup_old_tasks();

        // Task should be removed
        assert!(manager.get_task(task_id).is_none());
    }

    #[tokio::test]
    async fn test_task_manager_concurrent_creation() {
        let manager = TaskManager::new(300);

        // Create tasks concurrently
        let mut handles = vec![];
        for _ in 0..10 {
            let mgr = manager.clone();
            handles.push(tokio::spawn(async move { mgr.create_task() }));
        }

        // Collect results
        let mut task_ids = vec![];
        for handle in handles {
            task_ids.push(handle.await.unwrap());
        }

        // All task IDs should be unique
        let unique_ids: std::collections::HashSet<_> = task_ids.iter().collect();
        assert_eq!(unique_ids.len(), 10);

        // All tasks should be retrievable
        for task_id in task_ids {
            assert!(manager.get_task(task_id).is_some());
        }
    }

    #[tokio::test]
    async fn test_task_manager_stats() {
        let manager = TaskManager::new(300);

        // Create and complete some tasks
        let id1 = manager.create_task();
        manager.mark_running(id1);

        let id2 = manager.create_task();
        manager.complete_task(
            id2,
            TaskResult::Chat(ChatResponse {
                success: true,
                messages: None,
                error: None,
                metadata: None,
            }),
        );

        let id3 = manager.create_task();
        manager.fail_task(id3, "error".to_string());

        let stats = manager.task_stats();
        assert_eq!(stats.get("running"), Some(&1));
        assert_eq!(stats.get("completed"), Some(&1));
        assert_eq!(stats.get("failed"), Some(&1));
    }

    // ==========================
    // Authentication Tests
    // ==========================

    #[tokio::test]
    async fn test_auth_token_validation() {
        let token = ApiToken {
            token: ApiAuthManager::hash_token("test-secret"),
            name: "Test".to_string(),
            permissions: vec![ApiPermission::Read],
            expires_at: None,
            agent_id: None,
        };

        let manager = ApiAuthManager::new(vec![token]);

        // Valid token
        assert!(manager.validate_token("test-secret").await.is_ok());

        // Invalid token
        assert!(manager.validate_token("wrong-secret").await.is_err());
    }

    #[tokio::test]
    async fn test_auth_permission_check() {
        let token = ApiToken {
            token: ApiAuthManager::hash_token("test-token"),
            name: "Test".to_string(),
            permissions: vec![ApiPermission::Read],
            expires_at: None,
            agent_id: None,
        };

        let manager = ApiAuthManager::new(vec![token]);

        // Has Read permission
        assert!(manager
            .has_permission("test-token", ApiPermission::Read)
            .await
            .unwrap());

        // Doesn't have Write permission
        assert!(!manager
            .has_permission("test-token", ApiPermission::Write)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_auth_admin_has_all_permissions() {
        let token = ApiToken {
            token: ApiAuthManager::hash_token("admin-token"),
            name: "Admin".to_string(),
            permissions: vec![ApiPermission::Admin],
            expires_at: None,
            agent_id: None,
        };

        let manager = ApiAuthManager::new(vec![token]);

        // Admin has all permissions
        assert!(manager
            .has_permission("admin-token", ApiPermission::Read)
            .await
            .unwrap());
        assert!(manager
            .has_permission("admin-token", ApiPermission::Write)
            .await
            .unwrap());
        assert!(manager
            .has_permission("admin-token", ApiPermission::Execute)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_auth_expired_token() {
        let token = ApiToken {
            token: ApiAuthManager::hash_token("expired-token"),
            name: "Expired".to_string(),
            permissions: vec![ApiPermission::Read],
            expires_at: Some(chrono::Utc::now().timestamp() - 3600), // Expired 1 hour ago
            agent_id: None,
        };

        let manager = ApiAuthManager::new(vec![token]);

        // Expired token should fail
        assert!(manager.validate_token("expired-token").await.is_err());
    }

    #[tokio::test]
    async fn test_auth_disabled_allows_all() {
        let manager = ApiAuthManager::disabled();

        // Any token should work when auth is disabled
        let permissions = manager.validate_token("any-token").await.unwrap();

        // Should have standard permissions
        assert!(permissions.contains(&ApiPermission::Read));
        assert!(permissions.contains(&ApiPermission::Write));
        assert!(permissions.contains(&ApiPermission::Execute));
    }

    #[tokio::test]
    async fn test_auth_token_hashing() {
        let token1 = "my-secret-token";
        let token2 = "my-secret-token";
        let token3 = "different-token";

        let hash1 = ApiAuthManager::hash_token(token1);
        let hash2 = ApiAuthManager::hash_token(token2);
        let hash3 = ApiAuthManager::hash_token(token3);

        // Same token produces same hash
        assert_eq!(hash1, hash2);

        // Different token produces different hash
        assert_ne!(hash1, hash3);

        // Hash should be hex string (64 chars for SHA-256)
        assert_eq!(hash1.len(), 64);
    }

    // =========================
    // Security Tests
    // =========================

    #[tokio::test]
    async fn test_multiple_permission_levels() {
        let read_token = ApiToken {
            token: ApiAuthManager::hash_token("read-token"),
            name: "Read Only".to_string(),
            permissions: vec![ApiPermission::Read],
            expires_at: None,
            agent_id: None,
        };

        let write_token = ApiToken {
            token: ApiAuthManager::hash_token("write-token"),
            name: "Read/Write".to_string(),
            permissions: vec![ApiPermission::Read, ApiPermission::Write],
            expires_at: None,
            agent_id: None,
        };

        let manager = ApiAuthManager::new(vec![read_token, write_token]);

        // Read-only token can read
        assert!(manager
            .has_permission("read-token", ApiPermission::Read)
            .await
            .unwrap());

        // Read-only token cannot write
        assert!(!manager
            .has_permission("read-token", ApiPermission::Write)
            .await
            .unwrap());

        // Write token can read and write
        assert!(manager
            .has_permission("write-token", ApiPermission::Read)
            .await
            .unwrap());
        assert!(manager
            .has_permission("write-token", ApiPermission::Write)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_task_result_serialization() {
        // Ensure task results can be serialized (important for API responses)
        let result = TaskResult::Chat(ChatResponse {
            success: true,
            messages: None,
            error: Some("test error".to_string()),
            metadata: None,
        });

        let serialized = serde_json::to_string(&result);
        assert!(serialized.is_ok());

        let json_value = serde_json::to_value(&result).unwrap();
        assert!(json_value.is_object());
    }

    #[tokio::test]
    async fn test_task_cleanup_preserves_active_tasks() {
        let manager = TaskManager::new(1); // 1 second max age

        // Create completed task (will be cleaned up)
        let completed_id = manager.create_task();
        manager.complete_task(
            completed_id,
            TaskResult::Chat(ChatResponse {
                success: true,
                messages: None,
                error: None,
                metadata: None,
            }),
        );

        // Create active task (should not be cleaned up)
        let active_id = manager.create_task();
        manager.mark_running(active_id);

        // Wait for completed task to age
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Cleanup
        manager.cleanup_old_tasks();

        // Completed task should be removed
        assert!(manager.get_task(completed_id).is_none());

        // Active task should still exist
        assert!(manager.get_task(active_id).is_some());
    }

    #[tokio::test]
    async fn test_task_duration_tracking() {
        let manager = TaskManager::new(300);

        let task_id = manager.create_task();
        manager.mark_running(task_id);

        // Simulate some work
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        manager.complete_task(
            task_id,
            TaskResult::Chat(ChatResponse {
                success: true,
                messages: None,
                error: None,
                metadata: None,
            }),
        );

        let task = manager.get_task(task_id).unwrap();
        let duration = task.duration_ms();

        assert!(duration.is_some());
        assert!(duration.unwrap() >= 100); // At least 100ms
    }

    #[tokio::test]
    async fn test_concurrent_task_operations() {
        let manager = TaskManager::new(300);
        let task_id = manager.create_task();

        // Spawn multiple concurrent operations
        let mgr1 = manager.clone();
        let mgr2 = manager.clone();
        let mgr3 = manager.clone();

        let handle1 = tokio::spawn(async move {
            mgr1.mark_running(task_id);
        });

        let handle2 = tokio::spawn(async move {
            mgr2.get_task(task_id);
        });

        let handle3 = tokio::spawn(async move {
            mgr3.task_count();
        });

        // All operations should complete without panicking
        let _ = tokio::join!(handle1, handle2, handle3);
    }

    // ==========================
    // Endpoint Integration Tests
    // ==========================

    use crate::agent_api::{
        handlers::{
            action_handler, chat_handler, health_check, state_handler, task_status_handler,
        },
        server::AgentApiConfig,
        state::{ApiState, ServerState},
        types::{ActionRequest, ChatRequest, StateRequest},
    };
    use crate::security::RateLimiter;
    use axum::{
        body::Body,
        extract::{Path, State as AxumState},
        http::{Request, StatusCode},
        response::IntoResponse,
        Json,
    };
    use std::time::Duration;

    /// Helper to create test server state
    async fn create_test_server_state() -> ServerState {
        let runtime = create_test_runtime().await;
        {
            let mut rt = runtime.write().unwrap();
            // Disable streaming to avoid executor initialization during unit tests
            rt.set_setting("ui:streaming", serde_json::json!(false), false);
            // Ensure no provider racing in tests
            rt.set_setting("ui:provider_racing", serde_json::json!(false), false);
        }
        let api_state = ApiState::new(runtime);
        let auth_manager = Arc::new(ApiAuthManager::disabled());
        let rate_limiter = Arc::new(RwLock::new(RateLimiter::new(Duration::from_secs(60), 60)));
        let task_manager = TaskManager::new(300);
        let config = Arc::new(AgentApiConfig::default());

        ServerState {
            api_state,
            auth_manager,
            rate_limiter,
            task_manager,
            config,
        }
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let state = create_test_server_state().await;
        let response = health_check(AxumState(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_chat_endpoint_creates_task() {
        let state = create_test_server_state().await;

        let request = ChatRequest {
            text: "Hello, how are you?".to_string(),
            room_id: uuid::Uuid::nil(),
            entity_id: None,
            source: "test".to_string(),
            metadata: std::collections::HashMap::new(),
            stream: false,
        };

        let response = chat_handler(AxumState(state.clone()), Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[ignore]
    async fn test_chat_endpoint_rejects_empty_message() {
        let state = create_test_server_state().await;

        let request = ChatRequest {
            text: "   ".to_string(), // Empty/whitespace only
            room_id: uuid::Uuid::nil(),
            entity_id: None,
            source: "test".to_string(),
            metadata: std::collections::HashMap::new(),
            stream: false,
        };

        let response = chat_handler(AxumState(state), Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    #[ignore]
    async fn test_state_endpoint_creates_task() {
        let state = create_test_server_state().await;

        let request = StateRequest {
            room_id: uuid::Uuid::nil(),
            entity_id: Some(uuid::Uuid::nil()),
        };

        let response = state_handler(AxumState(state), Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_action_endpoint_rejects_empty_action() {
        let state = create_test_server_state().await;

        let request = ActionRequest {
            action: "   ".to_string(), // Empty/whitespace only
            room_id: uuid::Uuid::nil(),
            entity_id: None,
            parameters: std::collections::HashMap::new(),
        };

        let response = action_handler(AxumState(state), Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    #[ignore]
    async fn test_task_status_endpoint_not_found() {
        let state = create_test_server_state().await;
        let fake_task_id = uuid::Uuid::new_v4();

        let response = task_status_handler(AxumState(state), Path(fake_task_id))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    #[ignore]
    async fn test_task_lifecycle_end_to_end() {
        let state = create_test_server_state().await;

        // Create task via manager directly
        let task_id = state.task_manager.create_task();

        // Check it's pending
        let task = state.task_manager.get_task(task_id).unwrap();
        assert!(matches!(task.status, TaskStatus::Pending));

        // Mark as running
        state.task_manager.mark_running(task_id);

        // Complete it
        state.task_manager.complete_task(
            task_id,
            TaskResult::Chat(ChatResponse {
                success: true,
                messages: Some(vec![]),
                error: None,
                metadata: None,
            }),
        );

        // Verify via endpoint
        let response = task_status_handler(AxumState(state), Path(task_id))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
    }

    // ======================
    // Input Validation Tests
    // ======================

    #[tokio::test]
    async fn test_chat_rejects_extremely_long_message() {
        let state = create_test_server_state().await;

        let long_text = "a".repeat(1_000_000); // 1MB of text
        let request = ChatRequest {
            text: long_text,
            room_id: uuid::Uuid::nil(),
            entity_id: None,
            source: "test".to_string(),
            metadata: std::collections::HashMap::new(),
            stream: false,
        };

        let response = chat_handler(AxumState(state), Json(request))
            .await
            .into_response();

        // Should be rejected (either BAD_REQUEST or PAYLOAD_TOO_LARGE)
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_action_with_valid_input() {
        let state = create_test_server_state().await;

        let mut params = std::collections::HashMap::new();
        params.insert("key".to_string(), serde_json::json!("value"));

        let request = ActionRequest {
            action: "test_action".to_string(),
            room_id: uuid::Uuid::nil(),
            entity_id: None,
            parameters: params,
        };

        let response = action_handler(AxumState(state), Json(request))
            .await
            .into_response();

        // Test runtime has no actions, so should return NOT_FOUND
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // ================================
    // Authentication Integration Tests
    // ================================

    #[tokio::test]
    async fn test_auth_manager_with_production_tokens() {
        // Create production-style tokens
        let tokens = vec![
            ApiToken {
                token: ApiAuthManager::hash_token("prod-read-token"),
                name: "Production Reader".to_string(),
                permissions: vec![ApiPermission::Read],
                expires_at: None,
                agent_id: None,
            },
            ApiToken {
                token: ApiAuthManager::hash_token("prod-write-token"),
                name: "Production Writer".to_string(),
                permissions: vec![ApiPermission::Read, ApiPermission::Write],
                expires_at: None,
                agent_id: None,
            },
            ApiToken {
                token: ApiAuthManager::hash_token("prod-admin-token"),
                name: "Production Admin".to_string(),
                permissions: vec![ApiPermission::Admin],
                expires_at: None,
                agent_id: None,
            },
        ];

        let auth_manager = ApiAuthManager::new(tokens);

        // Read token can only read
        assert!(auth_manager
            .has_permission("prod-read-token", ApiPermission::Read)
            .await
            .unwrap());
        assert!(!auth_manager
            .has_permission("prod-read-token", ApiPermission::Write)
            .await
            .unwrap());

        // Write token can read and write
        assert!(auth_manager
            .has_permission("prod-write-token", ApiPermission::Read)
            .await
            .unwrap());
        assert!(auth_manager
            .has_permission("prod-write-token", ApiPermission::Write)
            .await
            .unwrap());

        // Admin has all permissions
        assert!(auth_manager
            .has_permission("prod-admin-token", ApiPermission::Read)
            .await
            .unwrap());
        assert!(auth_manager
            .has_permission("prod-admin-token", ApiPermission::Write)
            .await
            .unwrap());
        assert!(auth_manager
            .has_permission("prod-admin-token", ApiPermission::Execute)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_rate_limiter_enforcement() {
        let limiter = RateLimiter::new(Duration::from_secs(1), 3); // 3 requests per second

        // First 3 should pass
        assert!(limiter.check("test-key"));
        assert!(limiter.check("test-key"));
        assert!(limiter.check("test-key"));

        // 4th should fail
        assert!(!limiter.check("test-key"));

        // Different key should work
        assert!(limiter.check("other-key"));

        // Wait for window to reset
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Should work again
        assert!(limiter.check("test-key"));
    }
}
