//! WorkflowProvider - Provides workflow state to LLM

use crate::WorkflowRepository;
use zoey_core::{types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::debug;

/// Provider that supplies workflow information to the LLM
pub struct WorkflowProvider {
    repository: Arc<WorkflowRepository>,
}

impl WorkflowProvider {
    /// Create new workflow provider
    pub fn new(repository: Arc<WorkflowRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl Provider for WorkflowProvider {
    fn name(&self) -> &str {
        "workflow"
    }
    
    fn description(&self) -> Option<String> {
        Some("Provides information about available workflows and their status".to_string())
    }
    
    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        state: &State,
    ) -> Result<ProviderResult> {
        debug!("WorkflowProvider: Getting workflow information");
        
        // List available workflows
        let workflows = self.repository.list();
        
        if workflows.is_empty() {
            return Ok(ProviderResult {
                text: Some("No workflows are currently available.".to_string()),
                values: None,
                data: None,
            });
        }
        
        // Build workflow list text
        let mut workflow_text = format!("Available workflows ({}):\n", workflows.len());
        for workflow_name in &workflows {
            workflow_text.push_str(&format!("- {}\n", workflow_name));
            
            // Try to load workflow for description
            if let Ok(workflow) = self.repository.load(workflow_name) {
                if !workflow.description.is_empty() {
                    workflow_text.push_str(&format!("  Description: {}\n", workflow.description));
                }
                workflow_text.push_str(&format!("  Steps: {}\n", workflow.steps.len()));
            }
        }
        
        // Build structured data
        let mut workflows_data = serde_json::Map::new();
        for workflow_name in &workflows {
            if let Ok(workflow) = self.repository.load(workflow_name) {
                let mut workflow_obj = serde_json::Map::new();
                workflow_obj.insert("name".to_string(), serde_json::Value::String(workflow.name.clone()));
                workflow_obj.insert("version".to_string(), serde_json::Value::String(workflow.version.clone()));
                workflow_obj.insert("description".to_string(), serde_json::Value::String(workflow.description.clone()));
                workflow_obj.insert("step_count".to_string(), serde_json::Value::Number(
                    serde_json::Number::from(workflow.steps.len())
                ));
                
                // Add step names
                let step_names: Vec<serde_json::Value> = workflow.steps
                    .iter()
                    .map(|s| serde_json::Value::String(s.name.clone()))
                    .collect();
                workflow_obj.insert("steps".to_string(), serde_json::Value::Array(step_names));
                
                workflows_data.insert(workflow_name.clone(), serde_json::Value::Object(workflow_obj));
            }
        }
        
        let mut data = std::collections::HashMap::new();
        data.insert("workflows".to_string(), serde_json::Value::Object(workflows_data));
        
        Ok(ProviderResult {
            text: Some(workflow_text),
            values: Some({
                let mut values = std::collections::HashMap::new();
                values.insert("AVAILABLE_WORKFLOWS".to_string(), workflows.join(", "));
                values
            }),
            data: Some(data),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_provider_creation() {
        let repo = Arc::new(WorkflowRepository::new());
        let provider = WorkflowProvider::new(repo);
        
        assert_eq!(provider.name(), "workflow");
    }
}

