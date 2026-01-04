//! Task types for deferred/scheduled work

use super::UUID;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Task for deferred or scheduled execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Task ID
    pub id: UUID,

    /// Agent ID
    pub agent_id: UUID,

    /// Task type/name
    pub task_type: String,

    /// Task data/payload
    pub data: serde_json::Value,

    /// Status
    pub status: TaskStatus,

    /// Priority (higher = more important)
    pub priority: i32,

    /// Scheduled execution time (unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<i64>,

    /// Actual execution time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_at: Option<i64>,

    /// Retry count
    #[serde(default)]
    pub retry_count: i32,

    /// Max retries
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Creation timestamp
    pub created_at: i64,

    /// Update timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

fn default_max_retries() -> i32 {
    3
}

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    /// Pending execution
    Pending,
    /// Currently running
    Running,
    /// Completed successfully
    Completed,
    /// Failed (exceeded retries)
    Failed,
    /// Cancelled
    Cancelled,
}

/// Task worker trait for executing tasks
#[async_trait]
pub trait TaskWorker: Send + Sync {
    /// Task type this worker handles
    fn task_type(&self) -> &str;

    /// Execute a task
    async fn execute(
        &self,
        runtime: std::sync::Arc<dyn std::any::Any + Send + Sync>,
        task: &Task,
    ) -> Result<()>;

    /// Whether this worker can handle the task
    fn can_handle(&self, task: &Task) -> bool {
        task.task_type == self.task_type()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_task_creation() {
        let task = Task {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            task_type: "test_task".to_string(),
            data: serde_json::json!({"key": "value"}),
            status: TaskStatus::Pending,
            priority: 5,
            scheduled_at: None,
            executed_at: None,
            retry_count: 0,
            max_retries: 3,
            error: None,
            created_at: 12345,
            updated_at: None,
        };

        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.retry_count, 0);
    }

    #[test]
    fn test_task_status_serialization() {
        let status = TaskStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"RUNNING\"");
    }
}
