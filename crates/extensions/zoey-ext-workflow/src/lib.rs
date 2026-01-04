/*!
# Workflow Plugin for LauraAI

This plugin provides workflow orchestration including:

- **Workflow Engine**: Define and execute multi-step workflows
- **Task Management**: Create, schedule, and monitor tasks
- **Pipeline Orchestration**: Build data and ML pipelines
- **Conditional Logic**: Branching, loops, and error handling
- **Event-Driven Execution**: Trigger workflows on events

## Example Usage

```rust,ignore
use zoey_plugin_workflow::{
    WorkflowPlugin, Workflow, WorkflowBuilder,
    Task, TaskConfig, TaskStatus,
    Pipeline, PipelineStage,
    Scheduler, ScheduleConfig,
};
use serde_json::json;

// 1. Create a workflow
let workflow = WorkflowBuilder::new("data_processing")
    .add_task(Task::new("fetch_data"))
    .add_task(Task::new("process_data").depends_on("fetch_data"))
    .add_task(Task::new("save_results").depends_on("process_data"))
    .build().unwrap();

// 2. Execute workflow
let engine = WorkflowEngine::new();
let _result = engine.execute(workflow);

// 3. Schedule recurring workflow
let scheduler = Scheduler::new();
let _ = scheduler.total_jobs();
```
*/

pub mod conditionals;
pub mod context;
pub mod distributed;
pub mod executor;
pub mod pipeline;
pub mod resources;
pub mod scheduler;
pub mod task;
pub mod workflow;
// TODO: These new modules from PR #27 need additional integration work
// They depend on WorkflowDefinition and other types not yet in the base workflow module
// pub mod action;
// pub mod engine;
// pub mod provider;
// pub mod storage;
// pub mod validation;
pub mod plugin;

pub use conditionals::{
    ArithmeticOp, CompareOp, Condition, ConditionError, EvalContext, Expression, FailureMode,
    IfBranch, LoopBranch, LoopConfig, LoopIteration, ParallelBranch, SwitchBranch, WaitMode,
};
pub use context::{ContextValue, TaskContext, WorkflowContext};
pub use distributed::{
    Checkpoint, CoordinatorStats, DistributedConfig, DistributedCoordinator, DistributedError,
    DistributedTask, DistributedWorker, LoadBalancer, TaskResult as DistributedTaskResult,
    WorkerInfo, WorkerStatus,
};
pub use executor::{ExecutionConfig, ExecutionResult, WorkflowEngine};
pub use pipeline::{Pipeline, PipelineConfig, PipelineStage, StageResult};
pub use resources::{
    RateLimiter, ResourceAllocation, ResourceError, ResourceLimits, ResourcePool, ResourceQuota,
    ResourceRequirements, ResourceType, ResourceUsage, ResourceUtilization, ThrottleConfig,
};
pub use scheduler::{CronExpression, ScheduleConfig, ScheduledJob, Scheduler};
pub use task::{Task, TaskConfig, TaskHandler, TaskResult, TaskStatus};
pub use workflow::{Workflow, WorkflowBuilder, WorkflowConfig, WorkflowStatus};
// TODO: Re-enable when action/engine/provider/storage/validation are fully integrated
// pub use action::*;
// pub use engine::*;
// pub use provider::*;
// pub use storage::*;
// pub use validation::*;
pub use plugin::WorkflowPlugin;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Configuration for the workflow plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPluginConfig {
    /// Maximum concurrent workflows
    pub max_concurrent_workflows: usize,

    /// Maximum concurrent tasks per workflow
    pub max_concurrent_tasks: usize,

    /// Default task timeout in seconds
    pub default_timeout_secs: u64,

    /// Enable task retry on failure
    pub enable_retry: bool,

    /// Maximum retry attempts
    pub max_retries: usize,

    /// Retry delay in seconds
    pub retry_delay_secs: u64,

    /// Enable workflow persistence
    pub enable_persistence: bool,

    /// Checkpoint interval in seconds
    pub checkpoint_interval_secs: u64,

    /// Enable event-driven execution
    pub enable_events: bool,
}

impl Default for WorkflowPluginConfig {
    fn default() -> Self {
        Self {
            max_concurrent_workflows: 10,
            max_concurrent_tasks: 5,
            default_timeout_secs: 300,
            enable_retry: true,
            max_retries: 3,
            retry_delay_secs: 5,
            enable_persistence: true,
            checkpoint_interval_secs: 30,
            enable_events: true,
        }
    }
}

/// Workflow statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowStatistics {
    /// Total workflows executed
    pub total_workflows: usize,

    /// Successful workflows
    pub successful_workflows: usize,

    /// Failed workflows
    pub failed_workflows: usize,

    /// Total tasks executed
    pub total_tasks: usize,

    /// Average workflow duration (seconds)
    pub avg_workflow_duration_secs: f64,

    /// Active workflows
    pub active_workflows: usize,

    /// Scheduled jobs
    pub scheduled_jobs: usize,

    /// Last activity
    pub last_activity: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WorkflowPluginConfig::default();
        assert_eq!(config.max_concurrent_workflows, 10);
        assert!(config.enable_retry);
    }

    #[test]
    fn test_statistics() {
        let stats = WorkflowStatistics::default();
        assert_eq!(stats.total_workflows, 0);
    }
}
