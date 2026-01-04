/*!
# Workflow Plugin

Plugin interface for integrating workflow orchestration with LauraAI.
*/

use crate::{
    executor::WorkflowEngine, scheduler::Scheduler, workflow::Workflow, WorkflowPluginConfig,
    WorkflowStatistics,
};
use async_trait::async_trait;
use zoey_core::{ZoeyError, Plugin};
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// ============================================================================
// ANSI Art Banner Rendering
// ============================================================================

/// Represents a configuration setting row for display
struct SettingRow {
    name: String,
    value: String,
    is_default: bool,
    env_var: String,
}

/// Pad string to width, truncating if necessary
fn pad(s: &str, w: usize) -> String {
    let mut out = s.to_string();
    if out.len() > w {
        out.truncate(w);
    }
    let pad_len = if w > out.len() { w - out.len() } else { 0 };
    out + &" ".repeat(pad_len)
}

/// Render the Workflow plugin banner with settings
fn render_workflow_banner(rows: Vec<SettingRow>) {
    let blue = "\x1b[34m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{blue}+{line}+{reset}", line = "=".repeat(78), blue = blue, reset = reset);

    // ASCII Art Header - WORKFLOW with flow/pipeline aesthetic
    println!(
        "{blue}|{bold} __        _____  ____  _  _______ _     _____        __     {reset}{blue} {dim}o--o{reset}{blue}  |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{blue}|{bold} \\ \\      / / _ \\|  _ \\| |/ /  ___| |   / _ \\ \\      / /     {reset}{blue} {dim}|  |{reset}{blue}  |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{blue}|{bold}  \\ \\ /\\ / / | | | |_) | ' /| |_  | |  | | | \\ \\ /\\ / /      {reset}{blue} {dim}o--o{reset}{blue}  |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{blue}|{bold}   \\ V  V /| |_| |  _ <| . \\|  _| | |__| |_| |\\ V  V /       {reset}{blue} {dim}|  |{reset}{blue}  |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{blue}|{bold}    \\_/\\_/  \\___/|_| \\_\\_|\\_\\_|   |_____\\___/  \\_/\\_/        {reset}{blue} {dim}o--o{reset}{blue}  |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{blue}|{reset}", blue = blue, reset = reset);
    println!(
        "{blue}|{inner}|{reset}",
        inner = pad(&format!("   {yellow}Tasks{reset}  {dim}-->{reset}  {yellow}Pipelines{reset}  {dim}-->{reset}  {yellow}Scheduling{reset}  {dim}-->{reset}  {yellow}Automation{reset}",
            yellow = yellow, dim = dim, reset = reset), 78),
        blue = blue, reset = reset
    );

    // Separator
    println!("{blue}+{line}+{reset}", line = "-".repeat(78), blue = blue, reset = reset);

    // Settings header
    println!(
        "{blue}|{a}|{b}|{c}|{d}|{reset}",
        a = pad(&format!(" {bold}ENV VARIABLE{reset}", bold = bold, reset = reset), 34),
        b = pad(&format!(" {bold}VALUE{reset}", bold = bold, reset = reset), 20),
        c = pad(&format!(" {bold}STATUS{reset}", bold = bold, reset = reset), 12),
        d = pad(" ", 12),
        blue = blue, reset = reset
    );
    println!("{blue}+{line}+{reset}", line = "-".repeat(78), blue = blue, reset = reset);

    // Settings rows
    for row in &rows {
        let status_color = if row.is_default { dim } else { green };
        let status_text = if row.is_default { "default" } else { "custom" };
        let status_icon = if row.is_default { " " } else { ">" };

        println!(
            "{blue}|{icon} {name}|{value}|{status}|{pad}|{reset}",
            icon = status_icon,
            name = pad(&row.env_var, 32),
            value = pad(&row.value, 20),
            status = pad(&format!("{status_color}{status_text}{reset}", status_color = status_color, status_text = status_text, reset = reset), 22),
            pad = pad("", 12),
            blue = blue, reset = reset
        );
    }

    // Legend
    println!("{blue}+{line}+{reset}", line = "-".repeat(78), blue = blue, reset = reset);
    println!(
        "{blue}|{inner}|{reset}",
        inner = pad(&format!("  {green}>{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        blue = blue, reset = reset
    );
    println!("{blue}+{line}+{reset}", line = "=".repeat(78), blue = blue, reset = reset);
}

/// The workflow plugin
pub struct WorkflowPlugin {
    config: WorkflowPluginConfig,
    engine: Arc<RwLock<WorkflowEngine>>,
    scheduler: Arc<RwLock<Scheduler>>,
    workflows: Arc<RwLock<HashMap<Uuid, Workflow>>>,
    statistics: Arc<RwLock<WorkflowStatistics>>,
}

impl WorkflowPlugin {
    /// Create a new workflow plugin
    pub fn new() -> Self {
        Self {
            config: WorkflowPluginConfig::default(),
            engine: Arc::new(RwLock::new(WorkflowEngine::new())),
            scheduler: Arc::new(RwLock::new(Scheduler::new())),
            workflows: Arc::new(RwLock::new(HashMap::new())),
            statistics: Arc::new(RwLock::new(WorkflowStatistics::default())),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: WorkflowPluginConfig) -> Self {
        use crate::executor::ExecutionConfig;

        let engine_config = ExecutionConfig {
            max_concurrent_tasks: config.max_concurrent_tasks,
            task_timeout_secs: config.default_timeout_secs,
            retry_on_failure: config.enable_retry,
            max_retries: config.max_retries,
            retry_delay_secs: config.retry_delay_secs,
            ..Default::default()
        };

        Self {
            config,
            engine: Arc::new(RwLock::new(WorkflowEngine::with_config(engine_config))),
            scheduler: Arc::new(RwLock::new(Scheduler::new())),
            workflows: Arc::new(RwLock::new(HashMap::new())),
            statistics: Arc::new(RwLock::new(WorkflowStatistics::default())),
        }
    }

    /// Register a workflow
    pub async fn register_workflow(&self, workflow: Workflow) -> Uuid {
        let id = workflow.id;
        self.workflows.write().await.insert(id, workflow);
        println!("Registered workflow: {}", id);
        id
    }

    /// Get workflow by ID
    pub async fn get_workflow(&self, id: Uuid) -> Option<Workflow> {
        self.workflows.read().await.get(&id).cloned()
    }

    /// List all workflows
    pub async fn list_workflows(&self) -> Vec<Workflow> {
        self.workflows.read().await.values().cloned().collect()
    }

    /// Execute a workflow
    pub async fn execute(
        &self,
        workflow: Workflow,
    ) -> Result<crate::executor::ExecutionResult, crate::workflow::WorkflowError> {
        let engine = self.engine.read().await;
        let result = engine.execute(workflow).await?;

        // Update statistics
        let mut stats = self.statistics.write().await;
        stats.total_workflows += 1;
        if result.status == crate::workflow::WorkflowStatus::Completed {
            stats.successful_workflows += 1;
        } else {
            stats.failed_workflows += 1;
        }
        stats.total_tasks += result.task_results.len();
        stats.last_activity = Some(chrono::Utc::now());

        Ok(result)
    }

    /// Execute workflow by ID
    pub async fn execute_by_id(
        &self,
        id: Uuid,
    ) -> Result<crate::executor::ExecutionResult, crate::workflow::WorkflowError> {
        let workflow = self
            .get_workflow(id)
            .await
            .ok_or_else(|| crate::workflow::WorkflowError::TaskNotFound(id.to_string()))?;
        self.execute(workflow).await
    }

    /// Get the scheduler
    pub fn scheduler(&self) -> Arc<RwLock<Scheduler>> {
        self.scheduler.clone()
    }

    /// Get statistics
    pub async fn statistics(&self) -> WorkflowStatistics {
        let stats = self.statistics.read().await.clone();
        let mut updated = stats;

        // Update active workflows count
        let engine = self.engine.read().await;
        updated.active_workflows = engine.running_workflows().await.len();

        // Update scheduled jobs count
        let scheduler = self.scheduler.read().await;
        updated.scheduled_jobs = scheduler.list_jobs().await.len();

        updated
    }
}

impl Default for WorkflowPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for WorkflowPlugin {
    fn name(&self) -> &str {
        "workflow"
    }

    fn description(&self) -> &str {
        "Workflow orchestration - task automation, pipelines, and process management"
    }

    fn dependencies(&self) -> Vec<String> {
        vec![]
    }

    fn priority(&self) -> i32 {
        50 // Medium priority
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn Any + Send + Sync>,
    ) -> Result<(), ZoeyError> {
        let rows = vec![
            SettingRow {
                name: "WORKFLOW_MAX_CONCURRENT".to_string(),
                value: std::env::var("WORKFLOW_MAX_CONCURRENT").unwrap_or_else(|_| "10".to_string()),
                is_default: std::env::var("WORKFLOW_MAX_CONCURRENT").is_err(),
                env_var: "WORKFLOW_MAX_CONCURRENT".to_string(),
            },
            SettingRow {
                name: "WORKFLOW_MAX_TASKS".to_string(),
                value: std::env::var("WORKFLOW_MAX_TASKS").unwrap_or_else(|_| "5".to_string()),
                is_default: std::env::var("WORKFLOW_MAX_TASKS").is_err(),
                env_var: "WORKFLOW_MAX_TASKS".to_string(),
            },
            SettingRow {
                name: "WORKFLOW_DEFAULT_TIMEOUT_SECS".to_string(),
                value: std::env::var("WORKFLOW_DEFAULT_TIMEOUT_SECS").unwrap_or_else(|_| "300".to_string()),
                is_default: std::env::var("WORKFLOW_DEFAULT_TIMEOUT_SECS").is_err(),
                env_var: "WORKFLOW_DEFAULT_TIMEOUT_SECS".to_string(),
            },
            SettingRow {
                name: "WORKFLOW_ENABLE_RETRY".to_string(),
                value: std::env::var("WORKFLOW_ENABLE_RETRY").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("WORKFLOW_ENABLE_RETRY").is_err(),
                env_var: "WORKFLOW_ENABLE_RETRY".to_string(),
            },
            SettingRow {
                name: "WORKFLOW_MAX_RETRIES".to_string(),
                value: std::env::var("WORKFLOW_MAX_RETRIES").unwrap_or_else(|_| "3".to_string()),
                is_default: std::env::var("WORKFLOW_MAX_RETRIES").is_err(),
                env_var: "WORKFLOW_MAX_RETRIES".to_string(),
            },
            SettingRow {
                name: "WORKFLOW_PERSISTENCE".to_string(),
                value: std::env::var("WORKFLOW_PERSISTENCE").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("WORKFLOW_PERSISTENCE").is_err(),
                env_var: "WORKFLOW_PERSISTENCE".to_string(),
            },
            SettingRow {
                name: "WORKFLOW_EVENTS".to_string(),
                value: std::env::var("WORKFLOW_EVENTS").unwrap_or_else(|_| "true".to_string()),
                is_default: std::env::var("WORKFLOW_EVENTS").is_err(),
                env_var: "WORKFLOW_EVENTS".to_string(),
            },
        ];
        render_workflow_banner(rows);

        // Start scheduler
        self.scheduler.read().await.start().await;

        println!("Workflow plugin initialized successfully");
        Ok(())
    }

    fn actions(&self) -> Vec<Arc<dyn zoey_core::Action>> {
        vec![]
    }

    fn providers(&self) -> Vec<Arc<dyn zoey_core::Provider>> {
        vec![]
    }

    fn evaluators(&self) -> Vec<Arc<dyn zoey_core::Evaluator>> {
        vec![]
    }

    fn services(&self) -> Vec<Arc<dyn zoey_core::Service>> {
        vec![]
    }

    fn models(&self) -> HashMap<String, zoey_core::ModelHandler> {
        HashMap::new()
    }

    fn events(&self) -> HashMap<String, Vec<zoey_core::EventHandler>> {
        HashMap::new()
    }

    fn routes(&self) -> Vec<zoey_core::Route> {
        vec![]
    }

    fn schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "workflows": {
                "type": "table",
                "columns": {
                    "id": "UUID PRIMARY KEY",
                    "name": "VARCHAR(255) NOT NULL",
                    "config": "JSONB",
                    "status": "VARCHAR(50) DEFAULT 'created'",
                    "created_at": "TIMESTAMP DEFAULT NOW()",
                    "updated_at": "TIMESTAMP DEFAULT NOW()"
                }
            },
            "workflow_tasks": {
                "type": "table",
                "columns": {
                    "id": "UUID PRIMARY KEY",
                    "workflow_id": "UUID REFERENCES workflows(id)",
                    "name": "VARCHAR(255) NOT NULL",
                    "config": "JSONB",
                    "status": "VARCHAR(50) DEFAULT 'pending'",
                    "result": "JSONB",
                    "created_at": "TIMESTAMP DEFAULT NOW()"
                }
            },
            "workflow_executions": {
                "type": "table",
                "columns": {
                    "id": "UUID PRIMARY KEY",
                    "workflow_id": "UUID REFERENCES workflows(id)",
                    "status": "VARCHAR(50)",
                    "started_at": "TIMESTAMP",
                    "ended_at": "TIMESTAMP",
                    "duration_ms": "BIGINT",
                    "error": "TEXT"
                }
            },
            "scheduled_jobs": {
                "type": "table",
                "columns": {
                    "id": "UUID PRIMARY KEY",
                    "name": "VARCHAR(255) NOT NULL",
                    "workflow_id": "UUID REFERENCES workflows(id)",
                    "cron": "VARCHAR(100)",
                    "enabled": "BOOLEAN DEFAULT true",
                    "last_run": "TIMESTAMP",
                    "next_run": "TIMESTAMP",
                    "run_count": "INTEGER DEFAULT 0",
                    "created_at": "TIMESTAMP DEFAULT NOW()"
                }
            }
        }))
    }

    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "max_concurrent_workflows": {
                    "type": "integer",
                    "default": 10,
                    "description": "Maximum concurrent workflow executions"
                },
                "max_concurrent_tasks": {
                    "type": "integer",
                    "default": 5,
                    "description": "Maximum concurrent tasks per workflow"
                },
                "default_timeout_secs": {
                    "type": "integer",
                    "default": 300,
                    "description": "Default task timeout in seconds"
                },
                "enable_retry": {
                    "type": "boolean",
                    "default": true,
                    "description": "Enable automatic task retry on failure"
                },
                "max_retries": {
                    "type": "integer",
                    "default": 3,
                    "description": "Maximum retry attempts per task"
                },
                "enable_persistence": {
                    "type": "boolean",
                    "default": true,
                    "description": "Enable workflow state persistence"
                },
                "enable_events": {
                    "type": "boolean",
                    "default": true,
                    "description": "Enable event-driven workflow triggering"
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Task;
    use crate::workflow::WorkflowBuilder;

    #[test]
    fn test_plugin_creation() {
        let plugin = WorkflowPlugin::new();
        assert_eq!(plugin.name(), "workflow");
        assert_eq!(plugin.priority(), 50);
    }

    #[tokio::test]
    async fn test_workflow_registration() {
        let plugin = WorkflowPlugin::new();

        let workflow = WorkflowBuilder::new("test")
            .add_task(Task::new("task1"))
            .build()
            .unwrap();

        let id = plugin.register_workflow(workflow).await;

        let workflows = plugin.list_workflows().await;
        assert_eq!(workflows.len(), 1);

        let retrieved = plugin.get_workflow(id).await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_workflow_execution() {
        let plugin = WorkflowPlugin::new();

        let workflow = WorkflowBuilder::new("test")
            .add_task(Task::with_handler("task1", |_| async {
                Ok(serde_json::json!({"result": "success"}))
            }))
            .build()
            .unwrap();

        let result = plugin.execute(workflow).await.unwrap();
        assert_eq!(result.status, crate::workflow::WorkflowStatus::Completed);

        let stats = plugin.statistics().await;
        assert_eq!(stats.total_workflows, 1);
        assert_eq!(stats.successful_workflows, 1);
    }
}
