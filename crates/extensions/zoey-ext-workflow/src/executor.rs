/*!
# Workflow Executor

Provides workflow execution engine.
*/

use crate::context::{TaskContext, WorkflowContext};
use crate::task::{TaskResult, TaskStatus};
use crate::workflow::{Workflow, WorkflowError, WorkflowStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use uuid::Uuid;

/// Execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Maximum concurrent tasks
    pub max_concurrent_tasks: usize,

    /// Task execution timeout (seconds)
    pub task_timeout_secs: u64,

    /// Workflow timeout (seconds)
    pub workflow_timeout_secs: u64,

    /// Enable retry on failure
    pub retry_on_failure: bool,

    /// Maximum retries per task
    pub max_retries: usize,

    /// Retry delay (seconds)
    pub retry_delay_secs: u64,

    /// Enable checkpointing
    pub enable_checkpoints: bool,

    /// Continue workflow on task failure
    pub continue_on_failure: bool,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 5,
            task_timeout_secs: 300,
            workflow_timeout_secs: 3600,
            retry_on_failure: true,
            max_retries: 3,
            retry_delay_secs: 5,
            enable_checkpoints: true,
            continue_on_failure: false,
        }
    }
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Workflow ID
    pub workflow_id: Uuid,

    /// Workflow name
    pub workflow_name: String,

    /// Final status
    pub status: WorkflowStatus,

    /// Task results
    pub task_results: HashMap<String, TaskResult>,

    /// Start time
    pub started_at: DateTime<Utc>,

    /// End time
    pub ended_at: DateTime<Utc>,

    /// Total duration in milliseconds
    pub duration_ms: u64,

    /// Error message (if failed)
    pub error: Option<String>,
}

/// Workflow execution engine
pub struct WorkflowEngine {
    config: ExecutionConfig,
    running_workflows: Arc<RwLock<HashMap<Uuid, WorkflowStatus>>>,
    semaphore: Arc<Semaphore>,
}

impl WorkflowEngine {
    /// Create a new workflow engine
    pub fn new() -> Self {
        let config = ExecutionConfig::default();
        Self {
            semaphore: Arc::new(Semaphore::new(config.max_concurrent_tasks)),
            config,
            running_workflows: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ExecutionConfig) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(config.max_concurrent_tasks)),
            config,
            running_workflows: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Execute a workflow
    pub async fn execute(&self, mut workflow: Workflow) -> Result<ExecutionResult, WorkflowError> {
        let start = std::time::Instant::now();
        let started_at = Utc::now();

        workflow.set_status(WorkflowStatus::Running);

        // Register workflow
        self.running_workflows
            .write()
            .await
            .insert(workflow.id, WorkflowStatus::Running);

        // Create workflow context
        let wf_context = WorkflowContext::new(workflow.id, workflow.name());

        // Execute tasks
        let result = self.execute_tasks(&mut workflow, &wf_context).await;

        // Determine final status
        let (status, error) = match result {
            Ok(()) => {
                if workflow.has_failed() {
                    (
                        WorkflowStatus::Failed,
                        Some("One or more tasks failed".to_string()),
                    )
                } else {
                    (WorkflowStatus::Completed, None)
                }
            }
            Err(e) => (WorkflowStatus::Failed, Some(e.to_string())),
        };

        workflow.set_status(status);

        // Unregister workflow
        self.running_workflows.write().await.remove(&workflow.id);

        let duration = start.elapsed();

        Ok(ExecutionResult {
            workflow_id: workflow.id,
            workflow_name: workflow.name().to_string(),
            status,
            task_results: workflow.results().clone(),
            started_at,
            ended_at: Utc::now(),
            duration_ms: duration.as_millis() as u64,
            error,
        })
    }

    async fn execute_tasks(
        &self,
        workflow: &mut Workflow,
        context: &WorkflowContext,
    ) -> Result<(), WorkflowError> {
        loop {
            // Check if workflow is complete
            if workflow.is_complete() {
                break;
            }

            // Check if workflow has failed and should stop
            if workflow.has_failed() && !self.config.continue_on_failure {
                break;
            }

            // Get runnable task names (owned to avoid borrow issues)
            let runnable_names = workflow.get_runnable_task_names();

            if runnable_names.is_empty() {
                // No runnable tasks but not complete - might be blocked
                if !workflow.is_complete() {
                    // Wait a bit and check again
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }
                break;
            }

            // Execute runnable tasks (potentially in parallel)
            if self.config.max_concurrent_tasks > 1 && workflow.config.parallel_execution {
                self.execute_tasks_parallel(workflow, &runnable_names, context)
                    .await?;
            } else {
                self.execute_tasks_sequential(workflow, &runnable_names, context)
                    .await?;
            }
        }

        Ok(())
    }

    async fn execute_tasks_parallel(
        &self,
        workflow: &mut Workflow,
        task_names: &[String],
        context: &WorkflowContext,
    ) -> Result<(), WorkflowError> {
        // Clone tasks and prepare contexts before spawning
        let mut task_data: Vec<(String, crate::task::Task, TaskContext)> = Vec::new();

        for task_name in task_names {
            let task = match workflow.get_task(task_name) {
                Some(t) => t.clone(),
                None => continue,
            };

            // Create task context with inputs from dependencies
            let mut task_context = context.create_task_context(task_name);
            for dep in &task.config.dependencies {
                if let Some(result) = workflow.get_result(dep) {
                    if let Some(output) = &result.output {
                        task_context.set_input(dep, output.clone());
                    }
                }
            }
            task_data.push((task_name.clone(), task, task_context));
        }

        // Now spawn tasks
        let mut handles = Vec::new();
        for (task_name, mut task, task_context) in task_data {
            let semaphore = self.semaphore.clone();

            handles.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                let result = task.execute(task_context).await;
                (task_name, result)
            }));
        }

        // Collect results
        for handle in handles {
            let (task_name, result) = handle
                .await
                .map_err(|e| WorkflowError::ExecutionFailed(e.to_string()))?;

            let output = result.output.clone();
            workflow.store_result(result);

            // Store output in context
            if let Some(output) = output {
                context.store_task_output(&task_name, output).await;
            }
        }

        Ok(())
    }

    async fn execute_tasks_sequential(
        &self,
        workflow: &mut Workflow,
        task_names: &[String],
        context: &WorkflowContext,
    ) -> Result<(), WorkflowError> {
        for task_name in task_names {
            // Get dependencies first (immutable borrow)
            let dependencies: Vec<String> = workflow
                .get_task(task_name)
                .map(|t| t.config.dependencies.clone())
                .unwrap_or_default();

            // Create task context with inputs from dependencies
            let mut task_context = context.create_task_context(task_name);
            for dep in &dependencies {
                if let Some(result) = workflow.get_result(dep) {
                    if let Some(output) = &result.output {
                        task_context.set_input(dep, output.clone());
                    }
                }
            }

            // Now get mutable task and execute
            let task = workflow
                .get_task_mut(task_name)
                .ok_or_else(|| WorkflowError::TaskNotFound(task_name.clone()))?;

            // Execute with retry
            let result = self.execute_with_retry(task, task_context).await;
            let output = result.output.clone();
            let status = result.status;
            workflow.store_result(result);

            // Store output in context
            if let Some(output) = output {
                context.store_task_output(task_name, output).await;
            }

            // Check for failure
            if !self.config.continue_on_failure && status == TaskStatus::Failed {
                break;
            }
        }

        Ok(())
    }

    async fn execute_with_retry(
        &self,
        task: &mut crate::task::Task,
        context: TaskContext,
    ) -> TaskResult {
        let mut result = task.execute(context.clone()).await;

        while result.status == TaskStatus::Failed && task.can_retry() {
            task.increment_retry();
            tracing::info!(
                "Retrying task {} (attempt {}/{})",
                task.name(),
                task.retry_count(),
                task.config.max_retries
            );

            tokio::time::sleep(tokio::time::Duration::from_secs(
                self.config.retry_delay_secs,
            ))
            .await;

            task.set_status(TaskStatus::Retrying);
            result = task.execute(context.clone()).await;
            result.retry_count = task.retry_count();
        }

        result
    }

    /// Get running workflows
    pub async fn running_workflows(&self) -> HashMap<Uuid, WorkflowStatus> {
        self.running_workflows.read().await.clone()
    }

    /// Cancel a workflow
    pub async fn cancel(&self, workflow_id: Uuid) -> bool {
        let mut workflows = self.running_workflows.write().await;
        if let Some(status) = workflows.get_mut(&workflow_id) {
            *status = WorkflowStatus::Cancelled;
            true
        } else {
            false
        }
    }

    /// Pause a workflow
    pub async fn pause(&self, workflow_id: Uuid) -> bool {
        let mut workflows = self.running_workflows.write().await;
        if let Some(status) = workflows.get_mut(&workflow_id) {
            if *status == WorkflowStatus::Running {
                *status = WorkflowStatus::Paused;
                return true;
            }
        }
        false
    }

    /// Resume a workflow
    pub async fn resume(&self, workflow_id: Uuid) -> bool {
        let mut workflows = self.running_workflows.write().await;
        if let Some(status) = workflows.get_mut(&workflow_id) {
            if *status == WorkflowStatus::Paused {
                *status = WorkflowStatus::Running;
                return true;
            }
        }
        false
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Task;
    use crate::workflow::WorkflowBuilder;

    #[tokio::test]
    async fn test_simple_workflow() {
        let workflow = WorkflowBuilder::new("test")
            .add_task(Task::with_handler("task1", |_| async {
                Ok(serde_json::json!({"result": 1}))
            }))
            .add_task(
                Task::with_handler("task2", |_| async { Ok(serde_json::json!({"result": 2})) })
                    .depends_on("task1"),
            )
            .build()
            .unwrap();

        let engine = WorkflowEngine::new();
        let result = engine.execute(workflow).await.unwrap();

        assert_eq!(result.status, WorkflowStatus::Completed);
        assert_eq!(result.task_results.len(), 2);
    }

    #[tokio::test]
    async fn test_parallel_execution() {
        let workflow = WorkflowBuilder::new("test")
            .parallel(true)
            .max_concurrent(3)
            .add_task(Task::with_handler("task1", |_| async {
                Ok(serde_json::json!({}))
            }))
            .add_task(Task::with_handler("task2", |_| async {
                Ok(serde_json::json!({}))
            }))
            .add_task(Task::with_handler("task3", |_| async {
                Ok(serde_json::json!({}))
            }))
            .build()
            .unwrap();

        let engine = WorkflowEngine::new();
        let result = engine.execute(workflow).await.unwrap();

        assert_eq!(result.status, WorkflowStatus::Completed);
    }

    #[test]
    fn test_execution_config() {
        let config = ExecutionConfig::default();
        assert_eq!(config.max_concurrent_tasks, 5);
        assert!(config.retry_on_failure);
    }
}
