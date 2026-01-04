/*!
# Workflow Definition

Provides workflow creation and management.
*/

use crate::task::{Task, TaskResult, TaskStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Workflow status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    /// Workflow is defined but not started
    Created,
    /// Workflow is queued for execution
    Queued,
    /// Workflow is running
    Running,
    /// Workflow is paused
    Paused,
    /// Workflow completed successfully
    Completed,
    /// Workflow failed
    Failed,
    /// Workflow was cancelled
    Cancelled,
}

impl Default for WorkflowStatus {
    fn default() -> Self {
        Self::Created
    }
}

/// Workflow configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    /// Workflow name
    pub name: String,

    /// Workflow description
    pub description: String,

    /// Version
    pub version: String,

    /// Maximum execution time in seconds
    pub timeout_secs: u64,

    /// Enable parallel task execution
    pub parallel_execution: bool,

    /// Maximum concurrent tasks
    pub max_concurrent_tasks: usize,

    /// Enable checkpointing
    pub enable_checkpoints: bool,

    /// Continue on task failure
    pub continue_on_failure: bool,

    /// Tags
    pub tags: Vec<String>,

    /// Custom metadata
    pub metadata: serde_json::Value,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            version: "1.0.0".to_string(),
            timeout_secs: 3600,
            parallel_execution: true,
            max_concurrent_tasks: 5,
            enable_checkpoints: true,
            continue_on_failure: false,
            tags: Vec::new(),
            metadata: serde_json::Value::Null,
        }
    }
}

/// A workflow definition
#[derive(Clone)]
pub struct Workflow {
    /// Workflow ID
    pub id: Uuid,

    /// Configuration
    pub config: WorkflowConfig,

    /// Tasks in the workflow
    pub tasks: Vec<Task>,

    /// Task order (topologically sorted)
    task_order: Vec<String>,

    /// Current status
    status: WorkflowStatus,

    /// Task results
    results: HashMap<String, TaskResult>,

    /// Created timestamp
    pub created_at: DateTime<Utc>,

    /// Started timestamp
    pub started_at: Option<DateTime<Utc>>,

    /// Completed timestamp
    pub completed_at: Option<DateTime<Utc>>,
}

impl Workflow {
    /// Create a new workflow
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: Uuid::new_v4(),
            config: WorkflowConfig {
                name: name.clone(),
                description: format!("Workflow: {}", name),
                ..Default::default()
            },
            tasks: Vec::new(),
            task_order: Vec::new(),
            status: WorkflowStatus::Created,
            results: HashMap::new(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }

    /// Get workflow name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get workflow status
    pub fn status(&self) -> WorkflowStatus {
        self.status
    }

    /// Set workflow status
    pub fn set_status(&mut self, status: WorkflowStatus) {
        self.status = status;
        if status == WorkflowStatus::Running && self.started_at.is_none() {
            self.started_at = Some(Utc::now());
        }
        if matches!(
            status,
            WorkflowStatus::Completed | WorkflowStatus::Failed | WorkflowStatus::Cancelled
        ) {
            self.completed_at = Some(Utc::now());
        }
    }

    /// Add a task to the workflow
    pub fn add_task(&mut self, task: Task) {
        self.tasks.push(task);
        self.compute_task_order();
    }

    /// Get task by name
    pub fn get_task(&self, name: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.name() == name)
    }

    /// Get mutable task by name
    pub fn get_task_mut(&mut self, name: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.name() == name)
    }

    /// Get tasks in execution order
    pub fn tasks_in_order(&self) -> Vec<&Task> {
        self.task_order
            .iter()
            .filter_map(|name| self.get_task(name))
            .collect()
    }

    /// Get runnable tasks (dependencies met)
    pub fn get_runnable_tasks(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| {
                task.status() == TaskStatus::Pending && self.dependencies_met(task.name())
            })
            .collect()
    }

    /// Get runnable task names (owned, avoiding borrow issues)
    pub fn get_runnable_task_names(&self) -> Vec<String> {
        self.tasks
            .iter()
            .filter(|task| {
                task.status() == TaskStatus::Pending && self.dependencies_met(task.name())
            })
            .map(|task| task.name().to_string())
            .collect()
    }

    /// Check if dependencies are met for a task
    pub fn dependencies_met(&self, task_name: &str) -> bool {
        let task = match self.get_task(task_name) {
            Some(t) => t,
            None => return false,
        };

        task.config.dependencies.iter().all(|dep| {
            self.results
                .get(dep)
                .map(|r| r.status == TaskStatus::Completed)
                .unwrap_or(false)
        })
    }

    /// Store task result
    pub fn store_result(&mut self, result: TaskResult) {
        let task_name = result.task_name.clone();
        let status = result.status;
        self.results.insert(task_name.clone(), result);

        // Update task status
        if let Some(task) = self.get_task_mut(&task_name) {
            task.set_status(status);
        }
    }

    /// Get task result
    pub fn get_result(&self, task_name: &str) -> Option<&TaskResult> {
        self.results.get(task_name)
    }

    /// Get all results
    pub fn results(&self) -> &HashMap<String, TaskResult> {
        &self.results
    }

    /// Check if workflow is complete
    pub fn is_complete(&self) -> bool {
        self.tasks.iter().all(|task| {
            matches!(
                task.status(),
                TaskStatus::Completed
                    | TaskStatus::Failed
                    | TaskStatus::Skipped
                    | TaskStatus::Cancelled
            )
        })
    }

    /// Check if workflow failed
    pub fn has_failed(&self) -> bool {
        self.tasks
            .iter()
            .any(|task| task.status() == TaskStatus::Failed)
            && !self.config.continue_on_failure
    }

    /// Get progress (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.tasks.is_empty() {
            return 1.0;
        }

        let completed = self
            .tasks
            .iter()
            .filter(|t| matches!(t.status(), TaskStatus::Completed | TaskStatus::Skipped))
            .count();

        completed as f64 / self.tasks.len() as f64
    }

    /// Compute topological order of tasks
    fn compute_task_order(&mut self) {
        let mut order = Vec::new();
        let mut visited = HashMap::new();
        let mut in_progress = HashMap::new();

        for task in &self.tasks {
            visited.insert(task.name().to_string(), false);
            in_progress.insert(task.name().to_string(), false);
        }

        for task in &self.tasks {
            if !*visited.get(task.name()).unwrap_or(&true) {
                self.topological_sort(task.name(), &mut visited, &mut in_progress, &mut order);
            }
        }

        self.task_order = order;
    }

    fn topological_sort(
        &self,
        task_name: &str,
        visited: &mut HashMap<String, bool>,
        in_progress: &mut HashMap<String, bool>,
        order: &mut Vec<String>,
    ) {
        if *in_progress.get(task_name).unwrap_or(&false) {
            tracing::warn!("Circular dependency detected involving task: {}", task_name);
            return;
        }

        if *visited.get(task_name).unwrap_or(&true) {
            return;
        }

        in_progress.insert(task_name.to_string(), true);

        if let Some(task) = self.get_task(task_name) {
            for dep in &task.config.dependencies {
                self.topological_sort(dep, visited, in_progress, order);
            }
        }

        in_progress.insert(task_name.to_string(), false);
        visited.insert(task_name.to_string(), true);
        order.push(task_name.to_string());
    }
}

impl std::fmt::Debug for Workflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workflow")
            .field("id", &self.id)
            .field("name", &self.config.name)
            .field("status", &self.status)
            .field("tasks", &self.tasks.len())
            .field("progress", &format!("{:.0}%", self.progress() * 100.0))
            .finish()
    }
}

/// Builder for creating workflows
pub struct WorkflowBuilder {
    workflow: Workflow,
}

impl WorkflowBuilder {
    /// Create a new builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            workflow: Workflow::new(name),
        }
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.workflow.config.description = desc.into();
        self
    }

    /// Set version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.workflow.config.version = version.into();
        self
    }

    /// Set timeout
    pub fn timeout(mut self, secs: u64) -> Self {
        self.workflow.config.timeout_secs = secs;
        self
    }

    /// Enable/disable parallel execution
    pub fn parallel(mut self, enabled: bool) -> Self {
        self.workflow.config.parallel_execution = enabled;
        self
    }

    /// Set max concurrent tasks
    pub fn max_concurrent(mut self, max: usize) -> Self {
        self.workflow.config.max_concurrent_tasks = max;
        self
    }

    /// Enable/disable checkpoints
    pub fn checkpoints(mut self, enabled: bool) -> Self {
        self.workflow.config.enable_checkpoints = enabled;
        self
    }

    /// Continue on failure
    pub fn continue_on_failure(mut self, enabled: bool) -> Self {
        self.workflow.config.continue_on_failure = enabled;
        self
    }

    /// Add tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.workflow.config.tags.push(tag.into());
        self
    }

    /// Add a task
    pub fn add_task(mut self, task: Task) -> Self {
        self.workflow.add_task(task);
        self
    }

    /// Build the workflow
    pub fn build(self) -> Result<Workflow, WorkflowError> {
        if self.workflow.tasks.is_empty() {
            return Err(WorkflowError::EmptyWorkflow);
        }
        Ok(self.workflow)
    }
}

/// Workflow errors
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("Workflow is empty")]
    EmptyWorkflow,

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Workflow execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Workflow timed out")]
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_creation() {
        let workflow = Workflow::new("test_workflow");
        assert_eq!(workflow.name(), "test_workflow");
        assert_eq!(workflow.status(), WorkflowStatus::Created);
    }

    #[test]
    fn test_workflow_builder() {
        let workflow = WorkflowBuilder::new("test")
            .description("A test workflow")
            .timeout(600)
            .parallel(true)
            .add_task(Task::new("task1"))
            .add_task(Task::new("task2").depends_on("task1"))
            .build()
            .unwrap();

        assert_eq!(workflow.tasks.len(), 2);
        assert_eq!(workflow.config.timeout_secs, 600);
    }

    #[test]
    fn test_task_order() {
        let workflow = WorkflowBuilder::new("test")
            .add_task(Task::new("task3").depends_on("task2"))
            .add_task(Task::new("task1"))
            .add_task(Task::new("task2").depends_on("task1"))
            .build()
            .unwrap();

        let order = workflow.tasks_in_order();
        let names: Vec<&str> = order.iter().map(|t| t.name()).collect();

        // task1 should come before task2, task2 before task3
        let pos1 = names.iter().position(|&n| n == "task1").unwrap();
        let pos2 = names.iter().position(|&n| n == "task2").unwrap();
        let pos3 = names.iter().position(|&n| n == "task3").unwrap();

        assert!(pos1 < pos2);
        assert!(pos2 < pos3);
    }

    #[test]
    fn test_dependencies_met() {
        let mut workflow = WorkflowBuilder::new("test")
            .add_task(Task::new("task1"))
            .add_task(Task::new("task2").depends_on("task1"))
            .build()
            .unwrap();

        // Initially, task2 deps not met
        assert!(!workflow.dependencies_met("task2"));

        // After task1 completes, deps are met
        workflow.store_result(TaskResult::success(
            Uuid::new_v4(),
            "task1".to_string(),
            serde_json::json!({}),
        ));

        assert!(workflow.dependencies_met("task2"));
    }
}
