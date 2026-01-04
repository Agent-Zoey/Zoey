//! Workflow storage and versioning

use crate::workflow::WorkflowDefinition;
use crate::engine::WorkflowExecution;
use zoey_core::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock;

/// Workflow repository for loading and saving workflows
#[derive(Debug)]
pub struct WorkflowRepository {
    /// In-memory workflow cache
    workflows: Arc<RwLock<HashMap<String, WorkflowDefinition>>>,
    
    /// Base directory for workflow files
    base_dir: Option<PathBuf>,
}

impl WorkflowRepository {
    /// Create new workflow repository
    pub fn new() -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            base_dir: None,
        }
    }
    
    /// Create repository with base directory
    pub fn with_base_dir<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            base_dir: Some(base_dir.as_ref().to_path_buf()),
        }
    }
    
    /// Load workflow from YAML file
    pub fn load_from_file<P: AsRef<Path>>(&self, path: P) -> Result<WorkflowDefinition> {
        let workflow = WorkflowDefinition::from_file(path.as_ref())
            .map_err(|e| zoey_core::ZoeyError::Validation(format!("Failed to load workflow: {}", e)))?;
        
        // Cache workflow
        if let Ok(mut workflows) = self.workflows.write() {
            workflows.insert(workflow.name.clone(), workflow.clone());
        } else {
            return Err(zoey_core::ZoeyError::Runtime("Workflow cache lock poisoned".to_string()));
        }
        
        Ok(workflow)
    }
    
    /// Load workflow by name (from cache or file)
    pub fn load(&self, name: &str) -> Result<WorkflowDefinition> {
        // Check cache first
        {
            if let Ok(workflows) = self.workflows.read() {
                if let Some(workflow) = workflows.get(name) {
                    return Ok(workflow.clone());
                }
            }
        }
        
        // Load from file if base_dir is set
        if let Some(ref base_dir) = self.base_dir {
            let path = base_dir.join(format!("{}.yaml", name));
            if path.exists() {
                return self.load_from_file(&path);
            }
        }
        
        Err(zoey_core::ZoeyError::NotFound(
            format!("Workflow '{}' not found", name)
        ))
    }
    
    /// Register workflow in memory
    pub fn register(&self, workflow: WorkflowDefinition) {
        if let Ok(mut workflows) = self.workflows.write() {
            workflows.insert(workflow.name.clone(), workflow);
        }
    }
    
    /// Save workflow to file
    pub fn save_to_file<P: AsRef<Path>>(&self, workflow: &WorkflowDefinition, path: P) -> Result<()> {
        let yaml = workflow.to_yaml()
            .map_err(|e| zoey_core::ZoeyError::Validation(format!("Failed to serialize workflow: {}", e)))?;
        
        std::fs::write(path.as_ref(), yaml)
            .map_err(|e| zoey_core::ZoeyError::Runtime(format!("Failed to write workflow file: {}", e)))?;
        
        // Update cache
        if let Ok(mut workflows) = self.workflows.write() {
            workflows.insert(workflow.name.clone(), workflow.clone());
        }
        
        Ok(())
    }
    
    /// List all registered workflows
    pub fn list(&self) -> Vec<String> {
        if let Ok(workflows) = self.workflows.read() {
            workflows.keys().cloned().collect()
        } else {
            vec![]
        }
    }
    
    /// Check if workflow exists
    pub fn exists(&self, name: &str) -> bool {
        if let Ok(workflows) = self.workflows.read() {
            workflows.contains_key(name)
        } else {
            false
        }
    }
}

impl Default for WorkflowRepository {
    fn default() -> Self {
        Self::new()
    }
}

/// Workflow execution store for tracking workflow runs
#[derive(Debug)]
pub struct WorkflowExecutionStore {
    /// In-memory execution store
    executions: Arc<RwLock<HashMap<uuid::Uuid, WorkflowExecution>>>,
}

impl WorkflowExecutionStore {
    /// Create new execution store
    pub fn new() -> Self {
        Self {
            executions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Store execution
    pub fn store(&self, execution: WorkflowExecution) {
        let mut executions = self.executions.write().unwrap();
        executions.insert(execution.id, execution);
    }
    
    /// Get execution by ID
    pub fn get(&self, id: uuid::Uuid) -> Option<WorkflowExecution> {
        let executions = self.executions.read().unwrap();
        executions.get(&id).cloned()
    }
    
    /// List all executions
    pub fn list(&self) -> Vec<uuid::Uuid> {
        let executions = self.executions.read().unwrap();
        executions.keys().cloned().collect()
    }
    
    /// Get executions for a workflow
    pub fn get_by_workflow(&self, workflow_name: &str) -> Vec<WorkflowExecution> {
        let executions = self.executions.read().unwrap();
        executions.values()
            .filter(|e| e.workflow_name == workflow_name)
            .cloned()
            .collect()
    }
}

impl Default for WorkflowExecutionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_repository_new() {
        let repo = WorkflowRepository::new();
        assert_eq!(repo.list().len(), 0);
    }

    #[test]
    fn test_workflow_repository_register() {
        let repo = WorkflowRepository::new();
        let workflow = WorkflowDefinition {
            name: "test".to_string(),
            version: "1.0".to_string(),
            description: String::new(),
            steps: vec![],
            variables: HashMap::new(),
            on_error: crate::workflow::StepAction::Abort,
        };
        
        repo.register(workflow);
        assert!(repo.exists("test"));
        assert_eq!(repo.list().len(), 1);
    }

    #[test]
    fn test_execution_store() {
        let store = WorkflowExecutionStore::new();
        let workflow = WorkflowDefinition {
            name: "test".to_string(),
            version: "1.0".to_string(),
            description: String::new(),
            steps: vec![],
            variables: HashMap::new(),
            on_error: crate::workflow::StepAction::Abort,
        };
        
        let execution = WorkflowExecution::new(workflow);
        let id = execution.id;
        store.store(execution);
        
        assert!(store.get(id).is_some());
    }
}

