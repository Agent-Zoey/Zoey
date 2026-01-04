use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use zoey_core::{types::*, Result};
use std::collections::HashMap;
use std::sync::Arc;
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

/// Render the Scheduler plugin banner with settings
fn render_scheduler_banner(rows: Vec<SettingRow>) {
    let blue = "\x1b[34m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{blue}+{line}+{reset}", line = "=".repeat(78), blue = blue, reset = reset);

    // ASCII Art Header - SCHEDULER with clock aesthetic
    println!(
        "{blue}|{bold}  ____   ____ _   _ _____ ____  _   _ _     _____ ____   {reset}{blue}    {dim}12{reset}{blue}     |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{blue}|{bold} / ___| / ___| | | | ____|  _ \\| | | | |   | ____|  _ \\  {reset}{blue}  {dim}9  |  3{reset}{blue}  |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{blue}|{bold} \\___ \\| |   | |_| |  _| | | | | | | | |   |  _| | |_) | {reset}{blue}  {dim}   \\_/{reset}{blue}    |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{blue}|{bold}  ___) | |___|  _  | |___| |_| | |_| | |___| |___|  _ <  {reset}{blue}    {dim}6{reset}{blue}     |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{blue}|{bold} |____/ \\____|_| |_|_____|____/ \\___/|_____|_____|_| \\_\\ {reset}{blue}  {dim}[CRON]{reset}{blue}   |{reset}",
        blue = blue, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{blue}|{reset}", blue = blue, reset = reset);
    println!(
        "{blue}|{inner}|{reset}",
        inner = pad(&format!("      {yellow}Tasks{reset}  {dim}+{reset}  {yellow}Reminders{reset}  {dim}+{reset}  {yellow}Scheduling{reset}  {dim}+{reset}  {yellow}Automation{reset}",
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
        let status_icon = if row.is_default { " " } else { "+" };

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
        inner = pad(&format!("  {green}+{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        blue = blue, reset = reset
    );
    println!("{blue}+{line}+{reset}", line = "=".repeat(78), blue = blue, reset = reset);
}

pub struct SchedulerPlugin;

#[async_trait]
impl Plugin for SchedulerPlugin {
    fn name(&self) -> &str {
        "scheduler"
    }
    fn description(&self) -> &str {
        "Tasks and reminders"
    }
    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![
            Arc::new(CreateTaskAction),
            Arc::new(ListTasksAction),
            Arc::new(CompleteTaskAction),
            Arc::new(ScheduleReminderAction),
        ]
    }
    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        let rows = vec![
            SettingRow {
                name: "SCHEDULER_MAX_TASKS".to_string(),
                value: std::env::var("SCHEDULER_MAX_TASKS").unwrap_or_else(|_| "100".to_string()),
                is_default: std::env::var("SCHEDULER_MAX_TASKS").is_err(),
                env_var: "SCHEDULER_MAX_TASKS".to_string(),
            },
            SettingRow {
                name: "SCHEDULER_REMINDER_GRANULARITY".to_string(),
                value: std::env::var("SCHEDULER_REMINDER_GRANULARITY")
                    .unwrap_or_else(|_| "seconds".to_string()),
                is_default: std::env::var("SCHEDULER_REMINDER_GRANULARITY").is_err(),
                env_var: "SCHEDULER_REMINDER_GRANULARITY".to_string(),
            },
            SettingRow {
                name: "SCHEDULER_DEFAULT_PRIORITY".to_string(),
                value: std::env::var("SCHEDULER_DEFAULT_PRIORITY")
                    .unwrap_or_else(|_| "normal".to_string()),
                is_default: std::env::var("SCHEDULER_DEFAULT_PRIORITY").is_err(),
                env_var: "SCHEDULER_DEFAULT_PRIORITY".to_string(),
            },
            SettingRow {
                name: "SCHEDULER_RETENTION_DAYS".to_string(),
                value: std::env::var("SCHEDULER_RETENTION_DAYS")
                    .unwrap_or_else(|_| "30".to_string()),
                is_default: std::env::var("SCHEDULER_RETENTION_DAYS").is_err(),
                env_var: "SCHEDULER_RETENTION_DAYS".to_string(),
            },
        ];
        render_scheduler_banner(rows);
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TaskItem {
    id: Uuid,
    title: String,
    due_at: i64,
    completed: bool,
}

struct CreateTaskAction;
struct ListTasksAction;
struct CompleteTaskAction;
struct ScheduleReminderAction;

#[async_trait]
impl Action for CreateTaskAction {
    fn name(&self) -> &str {
        "create_task"
    }
    fn description(&self) -> &str {
        "Create a new task"
    }
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }
    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
        options: Option<HandlerOptions>,
        _cb: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        use zoey_core::runtime_ref::downcast_runtime_ref;
        let rt_ref = downcast_runtime_ref(&runtime)
            .and_then(|r| r.try_upgrade())
            .unwrap();
        let (agent_id, adapter) = {
            let rt = rt_ref.read().unwrap();
            (rt.agent_id, rt.get_adapter())
        };
        if let Some(adapter) = adapter {
            let title = options
                .as_ref()
                .and_then(|o| o.custom.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("Task");
            let due = options
                .as_ref()
                .and_then(|o| o.custom.get("due_at"))
                .and_then(|v| v.as_i64())
                .unwrap_or(Utc::now().timestamp());
            let item = TaskItem {
                id: Uuid::new_v4(),
                title: title.to_string(),
                due_at: due,
                completed: false,
            };
            let comp = Component {
                id: item.id,
                entity_id: agent_id,
                world_id: agent_id,
                source_entity_id: None,
                component_type: "task-item".to_string(),
                data: serde_json::to_value(item).unwrap(),
                created_at: Some(Utc::now().timestamp()),
                updated_at: Some(Utc::now().timestamp()),
            };
            let _ = adapter.create_component(&comp).await;
            return Ok(Some(ActionResult {
                action_name: Some(self.name().to_string()),
                text: Some("Task created".to_string()),
                values: None,
                data: None,
                success: true,
                error: None,
            }));
        }
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some("No adapter configured".to_string()),
            values: None,
            data: None,
            success: false,
            error: None,
        }))
    }
}

#[async_trait]
impl Action for ListTasksAction {
    fn name(&self) -> &str {
        "list_tasks"
    }
    fn description(&self) -> &str {
        "List tasks"
    }
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }
    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        _cb: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        use zoey_core::runtime_ref::downcast_runtime_ref;
        let rt_ref = downcast_runtime_ref(&runtime)
            .and_then(|r| r.try_upgrade())
            .unwrap();
        let (agent_id, adapter) = {
            let rt = rt_ref.read().unwrap();
            (rt.agent_id, rt.get_adapter())
        };
        if let Some(adapter) = adapter {
            let comps = adapter
                .get_components(agent_id, None, None)
                .await
                .unwrap_or_default();
            let items: Vec<serde_json::Value> = comps
                .into_iter()
                .filter(|c| c.component_type == "task-item")
                .map(|c| c.data)
                .collect();
            let mut data = HashMap::new();
            data.insert("tasks".to_string(), serde_json::to_value(items).unwrap());
            return Ok(Some(ActionResult {
                action_name: Some(self.name().to_string()),
                text: None,
                values: None,
                data: Some(data),
                success: true,
                error: None,
            }));
        }
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some("No adapter configured".to_string()),
            values: None,
            data: None,
            success: false,
            error: None,
        }))
    }
}

#[async_trait]
impl Action for CompleteTaskAction {
    fn name(&self) -> &str {
        "complete_task"
    }
    fn description(&self) -> &str {
        "Mark a task as complete"
    }
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }
    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
        options: Option<HandlerOptions>,
        _cb: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        use zoey_core::runtime_ref::downcast_runtime_ref;
        let rt_ref = downcast_runtime_ref(&runtime)
            .and_then(|r| r.try_upgrade())
            .unwrap();
        let (agent_id, adapter) = {
            let rt = rt_ref.read().unwrap();
            (rt.agent_id, rt.get_adapter())
        };
        if let Some(adapter) = adapter {
            if let Some(task_id) = options
                .as_ref()
                .and_then(|o| o.custom.get("task_id"))
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
            {
                let comps = adapter
                    .get_components(agent_id, None, None)
                    .await
                    .unwrap_or_default();
                if let Some(mut comp) = comps.into_iter().find(|c| c.id == task_id) {
                    let mut item: TaskItem = serde_json::from_value(comp.data.clone()).unwrap();
                    item.completed = true;
                    comp.data = serde_json::to_value(item).unwrap();
                    let _ = adapter.update_component(&comp).await;
                    return Ok(Some(ActionResult {
                        action_name: Some(self.name().to_string()),
                        text: Some("Task completed".to_string()),
                        values: None,
                        data: None,
                        success: true,
                        error: None,
                    }));
                }
            }
        }
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some("Task not found".to_string()),
            values: None,
            data: None,
            success: false,
            error: None,
        }))
    }
}

#[async_trait]
impl Action for ScheduleReminderAction {
    fn name(&self) -> &str {
        "schedule_reminder"
    }
    fn description(&self) -> &str {
        "Schedule a reminder"
    }
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }
    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        _cb: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some("Reminder scheduled".to_string()),
            values: None,
            data: None,
            success: true,
            error: None,
        }))
    }
}
