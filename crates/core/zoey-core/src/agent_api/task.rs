//! Task management system for async Agent API operations
//!
//! This module implements an async task pattern to work around Axum 0.7.9
//! Handler trait limitations with async functions. Instead of awaiting results
//! in the handler, we spawn tasks and return task IDs for polling.

use super::types::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Task status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task is queued but not yet started
    Pending,
    /// Task is currently running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed with an error
    Failed,
}

/// Generic task result that can hold any task output
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum TaskResult {
    Chat(ChatResponse),
    State(StateResponse),
    Action(ActionResponse),
}

/// Task information
#[derive(Debug, Clone)]
pub struct Task {
    pub id: Uuid,
    pub status: TaskStatus,
    pub result: Option<TaskResult>,
    pub error: Option<String>,
    pub created_at: Instant,
    pub completed_at: Option<Instant>,
}

impl Task {
    /// Create a new pending task
    pub fn new(id: Uuid) -> Self {
        Self {
            id,
            status: TaskStatus::Pending,
            result: None,
            error: None,
            created_at: Instant::now(),
            completed_at: None,
        }
    }

    /// Mark task as running
    pub fn mark_running(&mut self) {
        self.status = TaskStatus::Running;
    }

    /// Mark task as completed with result
    pub fn complete(&mut self, result: TaskResult) {
        self.status = TaskStatus::Completed;
        self.result = Some(result);
        self.completed_at = Some(Instant::now());
    }

    /// Mark task as failed with error
    pub fn fail(&mut self, error: String) {
        self.status = TaskStatus::Failed;
        self.error = Some(error);
        self.completed_at = Some(Instant::now());
    }

    /// Get task duration in milliseconds
    pub fn duration_ms(&self) -> Option<u128> {
        self.completed_at
            .map(|completed| completed.duration_since(self.created_at).as_millis())
    }
}

/// Task manager for handling async operations
#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<RwLock<HashMap<Uuid, Task>>>,
    max_age: Duration,
}

impl TaskManager {
    /// Create a new task manager
    pub fn new(max_age_secs: u64) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            max_age: Duration::from_secs(max_age_secs),
        }
    }

    /// Create a new task and return its ID
    pub fn create_task(&self) -> Uuid {
        let task_id = Uuid::new_v4();
        let task = Task::new(task_id);

        let mut tasks = self.tasks.write().unwrap();
        tasks.insert(task_id, task);

        task_id
    }

    /// Get task by ID
    pub fn get_task(&self, task_id: Uuid) -> Option<Task> {
        let tasks = self.tasks.read().unwrap();
        tasks.get(&task_id).cloned()
    }

    /// Update task status to running
    pub fn mark_running(&self, task_id: Uuid) {
        let mut tasks = self.tasks.write().unwrap();
        if let Some(task) = tasks.get_mut(&task_id) {
            task.mark_running();
        }
    }

    /// Complete task with result
    pub fn complete_task(&self, task_id: Uuid, result: TaskResult) {
        let mut tasks = self.tasks.write().unwrap();
        if let Some(task) = tasks.get_mut(&task_id) {
            task.complete(result);
        }
    }

    /// Fail task with error
    pub fn fail_task(&self, task_id: Uuid, error: String) {
        let mut tasks = self.tasks.write().unwrap();
        if let Some(task) = tasks.get_mut(&task_id) {
            task.fail(error);
        }
    }

    /// Remove old completed/failed tasks
    pub fn cleanup_old_tasks(&self) {
        let mut tasks = self.tasks.write().unwrap();
        let now = Instant::now();

        tasks.retain(|_, task| match task.status {
            TaskStatus::Pending | TaskStatus::Running => true,
            TaskStatus::Completed | TaskStatus::Failed => {
                if let Some(completed_at) = task.completed_at {
                    now.duration_since(completed_at) < self.max_age
                } else {
                    true
                }
            }
        });
    }

    /// Get total number of tasks
    pub fn task_count(&self) -> usize {
        let tasks = self.tasks.read().unwrap();
        tasks.len()
    }

    /// Get task counts by status
    pub fn task_stats(&self) -> HashMap<String, usize> {
        let tasks = self.tasks.read().unwrap();
        let mut stats = HashMap::new();

        for task in tasks.values() {
            let status = format!("{:?}", task.status).to_lowercase();
            *stats.entry(status).or_insert(0) += 1;
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_lifecycle() {
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

    #[test]
    fn test_task_failure() {
        let manager = TaskManager::new(60);
        let task_id = manager.create_task();

        manager.fail_task(task_id, "Test error".to_string());
        let task = manager.get_task(task_id).unwrap();
        assert!(matches!(task.status, TaskStatus::Failed));
        assert_eq!(task.error.as_deref(), Some("Test error"));
    }

    #[test]
    fn test_task_stats() {
        let manager = TaskManager::new(60);

        let id1 = manager.create_task();
        manager.mark_running(id1);

        let id2 = manager.create_task();
        let result = TaskResult::Chat(ChatResponse {
            success: true,
            messages: Some(vec![]),
            error: None,
            metadata: None,
        });
        manager.complete_task(id2, result);

        let stats = manager.task_stats();
        assert_eq!(stats.get("running"), Some(&1));
        assert_eq!(stats.get("completed"), Some(&1));
    }
}
