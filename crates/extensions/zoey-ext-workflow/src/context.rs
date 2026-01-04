/*!
# Workflow Context

Provides context management for workflow execution.
*/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Context value types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContextValue {
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<ContextValue>),
    Object(HashMap<String, ContextValue>),
    Json(serde_json::Value),
    Null,
}

impl From<String> for ContextValue {
    fn from(s: String) -> Self {
        ContextValue::String(s)
    }
}

impl From<&str> for ContextValue {
    fn from(s: &str) -> Self {
        ContextValue::String(s.to_string())
    }
}

impl From<f64> for ContextValue {
    fn from(n: f64) -> Self {
        ContextValue::Number(n)
    }
}

impl From<i64> for ContextValue {
    fn from(n: i64) -> Self {
        ContextValue::Number(n as f64)
    }
}

impl From<bool> for ContextValue {
    fn from(b: bool) -> Self {
        ContextValue::Bool(b)
    }
}

impl From<serde_json::Value> for ContextValue {
    fn from(v: serde_json::Value) -> Self {
        ContextValue::Json(v)
    }
}

/// Task execution context
#[derive(Debug, Clone)]
pub struct TaskContext {
    /// Task name
    pub task_name: String,

    /// Workflow ID
    pub workflow_id: Option<Uuid>,

    /// Input values from dependencies
    inputs: HashMap<String, serde_json::Value>,

    /// Local variables
    variables: HashMap<String, ContextValue>,

    /// Shared context
    shared: Arc<RwLock<SharedContext>>,
}

impl TaskContext {
    /// Create a new task context
    pub fn new(task_name: impl Into<String>) -> Self {
        Self {
            task_name: task_name.into(),
            workflow_id: None,
            inputs: HashMap::new(),
            variables: HashMap::new(),
            shared: Arc::new(RwLock::new(SharedContext::default())),
        }
    }

    /// Create with workflow ID
    pub fn with_workflow(mut self, workflow_id: Uuid) -> Self {
        self.workflow_id = Some(workflow_id);
        self
    }

    /// Create with shared context
    pub fn with_shared(mut self, shared: Arc<RwLock<SharedContext>>) -> Self {
        self.shared = shared;
        self
    }

    /// Set input from dependency
    pub fn set_input(&mut self, task_name: impl Into<String>, value: serde_json::Value) {
        self.inputs.insert(task_name.into(), value);
    }

    /// Get input from dependency
    pub fn get_input(&self, task_name: &str) -> Option<&serde_json::Value> {
        self.inputs.get(task_name)
    }

    /// Get all inputs
    pub fn inputs(&self) -> &HashMap<String, serde_json::Value> {
        &self.inputs
    }

    /// Set local variable
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<ContextValue>) {
        self.variables.insert(key.into(), value.into());
    }

    /// Get local variable
    pub fn get(&self, key: &str) -> Option<&ContextValue> {
        self.variables.get(key)
    }

    /// Get shared context
    pub fn shared(&self) -> Arc<RwLock<SharedContext>> {
        self.shared.clone()
    }

    /// Set shared variable
    pub async fn set_shared(&self, key: impl Into<String>, value: impl Into<ContextValue>) {
        self.shared.write().await.set(key, value);
    }

    /// Get shared variable
    pub async fn get_shared(&self, key: &str) -> Option<ContextValue> {
        self.shared.read().await.get(key).cloned()
    }
}

/// Shared context across tasks
#[derive(Debug, Clone, Default)]
pub struct SharedContext {
    values: HashMap<String, ContextValue>,
    artifacts: HashMap<String, serde_json::Value>,
}

impl SharedContext {
    /// Create new shared context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set value
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<ContextValue>) {
        self.values.insert(key.into(), value.into());
    }

    /// Get value
    pub fn get(&self, key: &str) -> Option<&ContextValue> {
        self.values.get(key)
    }

    /// Remove value
    pub fn remove(&mut self, key: &str) -> Option<ContextValue> {
        self.values.remove(key)
    }

    /// Set artifact
    pub fn set_artifact(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.artifacts.insert(key.into(), value);
    }

    /// Get artifact
    pub fn get_artifact(&self, key: &str) -> Option<&serde_json::Value> {
        self.artifacts.get(key)
    }

    /// List all keys
    pub fn keys(&self) -> Vec<&String> {
        self.values.keys().collect()
    }

    /// List all artifact keys
    pub fn artifact_keys(&self) -> Vec<&String> {
        self.artifacts.keys().collect()
    }

    /// Clear all values
    pub fn clear(&mut self) {
        self.values.clear();
        self.artifacts.clear();
    }
}

/// Workflow context
#[derive(Debug, Clone)]
pub struct WorkflowContext {
    /// Workflow ID
    pub workflow_id: Uuid,

    /// Workflow name
    pub workflow_name: String,

    /// Shared context
    shared: Arc<RwLock<SharedContext>>,

    /// Task outputs
    task_outputs: Arc<RwLock<HashMap<String, serde_json::Value>>>,

    /// Workflow parameters
    parameters: HashMap<String, serde_json::Value>,
}

impl WorkflowContext {
    /// Create new workflow context
    pub fn new(workflow_id: Uuid, workflow_name: impl Into<String>) -> Self {
        Self {
            workflow_id,
            workflow_name: workflow_name.into(),
            shared: Arc::new(RwLock::new(SharedContext::default())),
            task_outputs: Arc::new(RwLock::new(HashMap::new())),
            parameters: HashMap::new(),
        }
    }

    /// Set parameter
    pub fn set_parameter(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.parameters.insert(key.into(), value);
    }

    /// Get parameter
    pub fn get_parameter(&self, key: &str) -> Option<&serde_json::Value> {
        self.parameters.get(key)
    }

    /// Store task output
    pub async fn store_task_output(&self, task_name: impl Into<String>, output: serde_json::Value) {
        self.task_outputs
            .write()
            .await
            .insert(task_name.into(), output);
    }

    /// Get task output
    pub async fn get_task_output(&self, task_name: &str) -> Option<serde_json::Value> {
        self.task_outputs.read().await.get(task_name).cloned()
    }

    /// Create task context
    pub fn create_task_context(&self, task_name: impl Into<String>) -> TaskContext {
        TaskContext::new(task_name)
            .with_workflow(self.workflow_id)
            .with_shared(self.shared.clone())
    }

    /// Get shared context
    pub fn shared(&self) -> Arc<RwLock<SharedContext>> {
        self.shared.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_value_conversion() {
        let s: ContextValue = "hello".into();
        assert!(matches!(s, ContextValue::String(_)));

        let n: ContextValue = 42.0.into();
        assert!(matches!(n, ContextValue::Number(_)));

        let b: ContextValue = true.into();
        assert!(matches!(b, ContextValue::Bool(_)));
    }

    #[test]
    fn test_task_context() {
        let mut ctx = TaskContext::new("test_task");
        ctx.set("key1", "value1");
        ctx.set("key2", 42.0);

        assert!(matches!(ctx.get("key1"), Some(ContextValue::String(_))));
        assert!(matches!(ctx.get("key2"), Some(ContextValue::Number(_))));
    }

    #[tokio::test]
    async fn test_shared_context() {
        let ctx = TaskContext::new("test");
        ctx.set_shared("shared_key", "shared_value").await;

        let value = ctx.get_shared("shared_key").await;
        assert!(value.is_some());
    }

    #[tokio::test]
    async fn test_workflow_context() {
        let mut wf_ctx = WorkflowContext::new(Uuid::new_v4(), "test_workflow");
        wf_ctx.set_parameter("param1", serde_json::json!({"value": 123}));

        let task_ctx = wf_ctx.create_task_context("task1");
        assert_eq!(task_ctx.workflow_id, Some(wf_ctx.workflow_id));
    }
}
