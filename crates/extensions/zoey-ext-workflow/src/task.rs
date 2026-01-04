/*!
# Task Management

Provides task definition and execution.
*/

use crate::context::TaskContext;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is pending
    Pending,
    /// Task is queued
    Queued,
    /// Task is running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
    /// Task is being retried
    Retrying,
    /// Task timed out
    TimedOut,
    /// Task is skipped
    Skipped,
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Task result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task ID
    pub task_id: Uuid,

    /// Task name
    pub task_name: String,

    /// Status
    pub status: TaskStatus,

    /// Output data
    pub output: Option<serde_json::Value>,

    /// Error message (if failed)
    pub error: Option<String>,

    /// Start time
    pub started_at: Option<DateTime<Utc>>,

    /// End time
    pub ended_at: Option<DateTime<Utc>>,

    /// Duration in milliseconds
    pub duration_ms: Option<u64>,

    /// Retry count
    pub retry_count: usize,
}

impl TaskResult {
    pub fn success(task_id: Uuid, task_name: String, output: serde_json::Value) -> Self {
        Self {
            task_id,
            task_name,
            status: TaskStatus::Completed,
            output: Some(output),
            error: None,
            started_at: None,
            ended_at: Some(Utc::now()),
            duration_ms: None,
            retry_count: 0,
        }
    }

    pub fn failure(task_id: Uuid, task_name: String, error: String) -> Self {
        Self {
            task_id,
            task_name,
            status: TaskStatus::Failed,
            output: None,
            error: Some(error),
            started_at: None,
            ended_at: Some(Utc::now()),
            duration_ms: None,
            retry_count: 0,
        }
    }
}

/// Task configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// Task name
    pub name: String,

    /// Task description
    pub description: String,

    /// Timeout in seconds
    pub timeout_secs: u64,

    /// Enable retry
    pub retry_enabled: bool,

    /// Maximum retries
    pub max_retries: usize,

    /// Retry delay in seconds
    pub retry_delay_secs: u64,

    /// Dependencies (task names)
    pub dependencies: Vec<String>,

    /// Condition for execution
    pub condition: Option<String>,

    /// Tags for categorization
    pub tags: Vec<String>,

    /// Priority (higher = run first)
    pub priority: i32,

    /// Custom metadata
    pub metadata: serde_json::Value,
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            timeout_secs: 300,
            retry_enabled: true,
            max_retries: 3,
            retry_delay_secs: 5,
            dependencies: Vec::new(),
            condition: None,
            tags: Vec::new(),
            priority: 0,
            metadata: serde_json::Value::Null,
        }
    }
}

/// Task handler function type
pub type TaskHandler = Arc<
    dyn Fn(
            TaskContext,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, TaskError>> + Send>>
        + Send
        + Sync,
>;

/// A task in a workflow
pub struct Task {
    /// Task ID
    pub id: Uuid,

    /// Configuration
    pub config: TaskConfig,

    /// Handler function
    handler: Option<TaskHandler>,

    /// Current status
    status: TaskStatus,

    /// Retry count
    retry_count: usize,
}

impl Task {
    /// Create a new task
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: Uuid::new_v4(),
            config: TaskConfig {
                name: name.clone(),
                description: format!("Task: {}", name),
                ..Default::default()
            },
            handler: None,
            status: TaskStatus::Pending,
            retry_count: 0,
        }
    }

    /// Create with handler
    pub fn with_handler<F, Fut>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(TaskContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<serde_json::Value, TaskError>> + Send + 'static,
    {
        let name = name.into();
        Self {
            id: Uuid::new_v4(),
            config: TaskConfig {
                name: name.clone(),
                description: format!("Task: {}", name),
                ..Default::default()
            },
            handler: Some(Arc::new(move |ctx| Box::pin(handler(ctx)))),
            status: TaskStatus::Pending,
            retry_count: 0,
        }
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.config.description = desc.into();
        self
    }

    /// Add dependency
    pub fn depends_on(mut self, task_name: impl Into<String>) -> Self {
        self.config.dependencies.push(task_name.into());
        self
    }

    /// Add multiple dependencies
    pub fn depends_on_all(mut self, task_names: Vec<impl Into<String>>) -> Self {
        for name in task_names {
            self.config.dependencies.push(name.into());
        }
        self
    }

    /// Set timeout
    pub fn timeout(mut self, secs: u64) -> Self {
        self.config.timeout_secs = secs;
        self
    }

    /// Set retry configuration
    pub fn retry(mut self, max_retries: usize, delay_secs: u64) -> Self {
        self.config.retry_enabled = true;
        self.config.max_retries = max_retries;
        self.config.retry_delay_secs = delay_secs;
        self
    }

    /// Disable retry
    pub fn no_retry(mut self) -> Self {
        self.config.retry_enabled = false;
        self
    }

    /// Set condition
    pub fn when(mut self, condition: impl Into<String>) -> Self {
        self.config.condition = Some(condition.into());
        self
    }

    /// Set priority
    pub fn priority(mut self, priority: i32) -> Self {
        self.config.priority = priority;
        self
    }

    /// Add tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.config.tags.push(tag.into());
        self
    }

    /// Get name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get status
    pub fn status(&self) -> TaskStatus {
        self.status
    }

    /// Set status
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
    }

    /// Get retry count
    pub fn retry_count(&self) -> usize {
        self.retry_count
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    /// Check if can retry
    pub fn can_retry(&self) -> bool {
        self.config.retry_enabled && self.retry_count < self.config.max_retries
    }

    /// Execute the task
    pub async fn execute(&mut self, context: TaskContext) -> TaskResult {
        let start = std::time::Instant::now();
        self.status = TaskStatus::Running;

        let result = if let Some(ref handler) = self.handler {
            match tokio::time::timeout(
                std::time::Duration::from_secs(self.config.timeout_secs),
                handler(context),
            )
            .await
            {
                Ok(Ok(output)) => {
                    self.status = TaskStatus::Completed;
                    TaskResult {
                        task_id: self.id,
                        task_name: self.config.name.clone(),
                        status: TaskStatus::Completed,
                        output: Some(output),
                        error: None,
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        retry_count: self.retry_count,
                    }
                }
                Ok(Err(e)) => {
                    self.status = TaskStatus::Failed;
                    TaskResult::failure(self.id, self.config.name.clone(), e.to_string())
                }
                Err(_) => {
                    self.status = TaskStatus::TimedOut;
                    TaskResult::failure(
                        self.id,
                        self.config.name.clone(),
                        format!("Task timed out after {} seconds", self.config.timeout_secs),
                    )
                }
            }
        } else {
            // No handler - return empty success
            self.status = TaskStatus::Completed;
            TaskResult::success(self.id, self.config.name.clone(), serde_json::json!({}))
        };

        result
    }
}

impl Clone for Task {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            config: self.config.clone(),
            handler: self.handler.clone(),
            status: self.status,
            retry_count: self.retry_count,
        }
    }
}

impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("id", &self.id)
            .field("name", &self.config.name)
            .field("status", &self.status)
            .field("dependencies", &self.config.dependencies)
            .finish()
    }
}

/// Task errors
#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("Task execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Task timed out")]
    Timeout,

    #[error("Task cancelled")]
    Cancelled,

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Dependency failed: {0}")]
    DependencyFailed(String),

    #[error("Condition not met: {0}")]
    ConditionNotMet(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new("test_task")
            .description("A test task")
            .depends_on("other_task")
            .timeout(60)
            .priority(10);

        assert_eq!(task.name(), "test_task");
        assert_eq!(task.config.timeout_secs, 60);
        assert_eq!(task.config.priority, 10);
        assert!(task.config.dependencies.contains(&"other_task".to_string()));
    }

    #[test]
    fn test_task_retry() {
        let mut task = Task::new("test").retry(3, 5);

        assert!(task.can_retry());

        task.increment_retry();
        task.increment_retry();
        task.increment_retry();

        assert!(!task.can_retry());
    }

    #[tokio::test]
    async fn test_task_execution() {
        let mut task = Task::with_handler("test", |_ctx| async {
            Ok(serde_json::json!({"result": "success"}))
        });

        let context = TaskContext::new("test");
        let result = task.execute(context).await;

        assert_eq!(result.status, TaskStatus::Completed);
        assert!(result.output.is_some());
    }
}
