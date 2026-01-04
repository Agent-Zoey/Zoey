//! Workflow execution engine

use crate::workflow::WorkflowDefinition;
use zoey_core::{types::*, Result};
use std::collections::HashMap;
use std::sync::Arc;

/// Workflow execution state
#[derive(Debug, Clone)]
pub struct WorkflowState {
    /// Workflow definition
    pub workflow: WorkflowDefinition,
    
    /// Step results (step_name -> ActionResult)
    pub step_results: HashMap<String, ActionResult>,
    
    /// Global variables
    pub variables: HashMap<String, serde_json::Value>,
    
    /// Current step being executed
    pub current_step: Option<String>,
    
    /// Execution status
    pub status: WorkflowExecutionStatus,
    
    /// Error message if failed
    pub error: Option<String>,
}

/// Workflow execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowExecutionStatus {
    /// Not started
    Pending,
    
    /// Currently executing
    Running,
    
    /// Completed successfully
    Completed,
    
    /// Failed
    Failed,
    
    /// Aborted
    Aborted,
}

/// Workflow execution context
#[derive(Debug, Clone)]
pub struct WorkflowExecution {
    /// Execution ID
    pub id: uuid::Uuid,
    
    /// Workflow name
    pub workflow_name: String,
    
    /// State
    pub state: WorkflowState,
    
    /// Start time
    pub started_at: chrono::DateTime<chrono::Utc>,
    
    /// End time
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl WorkflowState {
    /// Create new workflow state
    pub fn new(workflow: WorkflowDefinition) -> Self {
        Self {
            variables: workflow.variables.clone(),
            workflow,
            step_results: HashMap::new(),
            current_step: None,
            status: WorkflowExecutionStatus::Pending,
            error: None,
        }
    }
    
    /// Get step result
    pub fn get_step_result(&self, step_name: &str) -> Option<&ActionResult> {
        self.step_results.get(step_name)
    }
    
    /// Set step result
    pub fn set_step_result(&mut self, step_name: String, result: ActionResult) {
        self.step_results.insert(step_name, result);
    }
    
    /// Resolve template string (e.g., "{{steps.step1.result.text}}")
    pub fn resolve_template(&self, template: &str) -> Result<String> {
        use handlebars::Handlebars;
        
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(true);
        
        // Build context for template rendering
        let mut context = serde_json::Map::new();
        
        // Add step results
        let mut steps = serde_json::Map::new();
        for (step_name, result) in &self.step_results {
            let mut step_obj = serde_json::Map::new();
            
            if let Some(ref text) = result.text {
                step_obj.insert("text".to_string(), serde_json::Value::String(text.clone()));
            }
            
            if let Some(ref values) = result.values {
                let mut values_obj = serde_json::Map::new();
                for (k, v) in values {
                    values_obj.insert(k.clone(), serde_json::Value::String(v.clone()));
                }
                step_obj.insert("result".to_string(), serde_json::Value::Object(values_obj));
            }
            
            if let Some(ref data) = result.data {
                step_obj.insert("data".to_string(), serde_json::Value::Object(
                    data.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                ));
            }
            
            steps.insert(step_name.clone(), serde_json::Value::Object(step_obj));
        }
        context.insert("steps".to_string(), serde_json::Value::Object(steps));
        
        // Add variables
        context.insert("variables".to_string(), serde_json::Value::Object(
            self.variables.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        ));
        
        // Render template
        handlebars.render_template(template, &context)
            .map_err(|e| zoey_core::ZoeyError::Validation(format!("Template rendering error: {}", e)))
    }
    
    /// Evaluate condition expression
    pub fn evaluate_condition(&self, condition: &str) -> Result<bool> {
        // Support template-based conditions with comparisons
        // Examples:
        // - "{{steps.step1.result.priority}} == 'high'"
        // - "{{steps.step1.result.success}} == true"
        // - "{{variables.count}} > 5"
        
        // First, resolve any template variables
        let resolved = self.resolve_template(condition)?;
        
        // Parse comparison operators
        let operators = ["==", "!=", ">=", "<=", ">", "<"];
        for op in &operators {
            if resolved.contains(op) {
                let parts: Vec<&str> = resolved.split(op).map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    let left = parts[0].trim_matches('"').trim_matches('\'');
                    let right = parts[1].trim_matches('"').trim_matches('\'');
                    
                    match *op {
                        "==" => return Ok(left == right),
                        "!=" => return Ok(left != right),
                        ">" => {
                            if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
                                return Ok(l > r);
                            }
                        }
                        "<" => {
                            if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
                                return Ok(l < r);
                            }
                        }
                        ">=" => {
                            if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
                                return Ok(l >= r);
                            }
                        }
                        "<=" => {
                            if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
                                return Ok(l <= r);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        
        // Boolean evaluation
        let lower = resolved.to_lowercase();
        if lower == "true" || lower == "1" || lower == "yes" {
            Ok(true)
        } else if lower == "false" || lower == "0" || lower == "no" || lower.is_empty() {
            Ok(false)
        } else {
            // Default: non-empty string is true
            Ok(!resolved.trim().is_empty())
        }
    }
}

impl WorkflowExecution {
    /// Create new workflow execution
    pub fn new(workflow: WorkflowDefinition) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            workflow_name: workflow.name.clone(),
            state: WorkflowState::new(workflow),
            started_at: chrono::Utc::now(),
            ended_at: None,
        }
    }
    
    /// Mark as running
    pub fn start(&mut self) {
        self.state.status = WorkflowExecutionStatus::Running;
    }
    
    /// Mark as completed
    pub fn complete(&mut self) {
        self.state.status = WorkflowExecutionStatus::Completed;
        self.ended_at = Some(chrono::Utc::now());
    }
    
    /// Mark as failed
    pub fn fail(&mut self, error: String) {
        self.state.status = WorkflowExecutionStatus::Failed;
        self.state.error = Some(error);
        self.ended_at = Some(chrono::Utc::now());
    }
    
    /// Mark as aborted
    pub fn abort(&mut self) {
        self.state.status = WorkflowExecutionStatus::Aborted;
        self.ended_at = Some(chrono::Utc::now());
    }
}

/// Workflow engine for managing workflow execution
pub struct WorkflowEngine {
    /// Runtime reference
    runtime: Arc<std::sync::RwLock<zoey_core::AgentRuntime>>,
}

impl WorkflowEngine {
    /// Create new workflow engine
    pub fn new(runtime: Arc<std::sync::RwLock<zoey_core::AgentRuntime>>) -> Self {
        Self { runtime }
    }
    
    /// Get runtime
    pub fn runtime(&self) -> Arc<std::sync::RwLock<zoey_core::AgentRuntime>> {
        self.runtime.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_state_new() {
        let workflow = WorkflowDefinition {
            name: "test".to_string(),
            version: "1.0".to_string(),
            description: String::new(),
            steps: vec![],
            variables: HashMap::new(),
            on_error: crate::workflow::StepAction::Abort,
        };
        
        let state = WorkflowState::new(workflow);
        assert_eq!(state.status, WorkflowExecutionStatus::Pending);
    }

    #[test]
    fn test_workflow_execution_new() {
        let workflow = WorkflowDefinition {
            name: "test".to_string(),
            version: "1.0".to_string(),
            description: String::new(),
            steps: vec![],
            variables: HashMap::new(),
            on_error: crate::workflow::StepAction::Abort,
        };
        
        let execution = WorkflowExecution::new(workflow);
        assert_eq!(execution.state.status, WorkflowExecutionStatus::Pending);
        assert!(execution.ended_at.is_none());
    }
}

