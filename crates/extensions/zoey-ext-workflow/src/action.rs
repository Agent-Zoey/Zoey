//! WorkflowAction - Action that executes workflows

use crate::{WorkflowRepository, WorkflowValidator, WorkflowExecutor, WorkflowExecution};
use zoey_core::{types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info};

/// Action that executes workflows
pub struct WorkflowAction {
    repository: Arc<WorkflowRepository>,
    validator: Arc<WorkflowValidator>,
}

impl WorkflowAction {
    /// Create new workflow action
    pub fn new(
        repository: Arc<WorkflowRepository>,
        validator: Arc<WorkflowValidator>,
    ) -> Self {
        Self { 
            repository, 
            validator,
        }
    }
}

#[async_trait]
impl Action for WorkflowAction {
    fn name(&self) -> &str {
        "EXECUTE_WORKFLOW"
    }
    
    fn description(&self) -> &str {
        "Execute a YAML-defined workflow by composing multiple actions"
    }
    
    fn examples(&self) -> Vec<Vec<ActionExample>> {
        vec![
            vec![
                ActionExample {
                    name: "User".to_string(),
                    text: "Run patient intake workflow".to_string(),
                },
                ActionExample {
                    name: "Agent".to_string(),
                    text: "I'll execute the patient intake workflow for you.".to_string(),
                },
            ],
        ]
    }
    
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
    ) -> Result<bool> {
        // Check if workflow name is in state or message
        let workflow_name = state.get_value("WORKFLOW_NAME")
            .or_else(|| state.get_value("workflow_name"))
            .or_else(|| {
                // Try to extract from message text
                let text = message.content.text.to_lowercase();
                if text.contains("workflow") {
                    // Simple extraction - in production, use LLM or better parsing
                    None // Would need better extraction logic
                } else {
                    None
                }
            });
        
        // For now, always validate (workflow name should be in state)
        Ok(true)
    }
    
    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
        options: Option<HandlerOptions>,
        _callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        info!("Executing workflow action");
        
        // Get workflow name from state or options
        let workflow_name = options.as_ref()
            .and_then(|o| o.custom.get("workflow_name"))
            .and_then(|v| v.as_str())
            .or_else(|| state.get_value("WORKFLOW_NAME").map(|s| s.as_str()))
            .or_else(|| state.get_value("workflow_name").map(|s| s.as_str()))
            .ok_or_else(|| zoey_core::ZoeyError::Validation(
                "Workflow name not specified".to_string()
            ))?;
        
        debug!("Loading workflow: {}", workflow_name);
        
        // Load workflow
        let workflow = self.repository.load(workflow_name)
            .map_err(|e| zoey_core::ZoeyError::NotFound(
                format!("Failed to load workflow '{}': {}", workflow_name, e)
            ))?;
        
        // Validate workflow
        self.validator.validate(&workflow)?;
        
        // Extract runtime from handler parameter (RuntimeRef)
        use zoey_core::runtime_ref::downcast_runtime_ref;
        let runtime_arc = downcast_runtime_ref(&runtime)
            .and_then(|runtime_ref| runtime_ref.try_upgrade())
            .ok_or_else(|| zoey_core::ZoeyError::Runtime(
                "Failed to extract runtime from handler parameter - ensure runtime is properly initialized".to_string()
            ))?;
        
        // Create executor
        let executor = WorkflowExecutor::new(runtime_arc);
        
        // Create execution
        let execution = WorkflowExecution::new(workflow);
        
        // Execute workflow
        let result = executor.execute(execution, message.clone(), state.clone()).await?;
        
        // Build result
        let mut result_data = std::collections::HashMap::new();
        result_data.insert("workflow_name".to_string(), serde_json::Value::String(workflow_name.to_string()));
        result_data.insert("execution_id".to_string(), serde_json::Value::String(result.id.to_string()));
        result_data.insert("status".to_string(), serde_json::Value::String(
            format!("{:?}", result.state.status)
        ));
        
        if let Some(ref error) = result.state.error {
            result_data.insert("error".to_string(), serde_json::Value::String(error.clone()));
        }
        
        // Add step results
        let mut steps_obj = serde_json::Map::new();
        for (step_name, step_result) in &result.state.step_results {
            let mut step_obj = serde_json::Map::new();
            if let Some(ref text) = step_result.text {
                step_obj.insert("text".to_string(), serde_json::Value::String(text.clone()));
            }
            step_obj.insert("success".to_string(), serde_json::Value::Bool(step_result.success));
            steps_obj.insert(step_name.clone(), serde_json::Value::Object(step_obj));
        }
        result_data.insert("steps".to_string(), serde_json::Value::Object(steps_obj));
        
        Ok(Some(ActionResult {
            action_name: Some("EXECUTE_WORKFLOW".to_string()),
            text: Some(format!("Workflow '{}' executed with status: {:?}", workflow_name, result.state.status)),
            values: None,
            data: Some(result_data),
            success: result.state.status == crate::engine::WorkflowExecutionStatus::Completed,
            error: result.state.error,
        }))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_action_creation() {
        let repo = Arc::new(WorkflowRepository::new());
        let validator = Arc::new(WorkflowValidator::default());
        let action = WorkflowAction::new(repo, validator);
        
        assert_eq!(action.name(), "EXECUTE_WORKFLOW");
    }
}

