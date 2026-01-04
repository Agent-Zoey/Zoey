//! Workflow validation and security checks

use crate::workflow::WorkflowDefinition;
use zoey_core::Result;
use std::collections::HashSet;

/// Workflow validator with security checks
#[derive(Debug, Clone)]
pub struct WorkflowValidator {
    /// Allowed action names (whitelist)
    allowed_actions: HashSet<String>,
    
    /// Maximum number of steps
    max_steps: usize,
    
    /// Maximum dependency depth
    max_depth: usize,
    
    /// Maximum retries per step
    max_retries: usize,
    
    /// Maximum timeout in milliseconds
    max_timeout_ms: u64,
}

impl Default for WorkflowValidator {
    fn default() -> Self {
        Self {
            allowed_actions: HashSet::new(), // Empty = allow all (can be configured)
            max_steps: 100,
            max_depth: 20,
            max_retries: 10,
            max_timeout_ms: 300_000, // 5 minutes
        }
    }
}

impl WorkflowValidator {
    /// Create new validator with default settings
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create validator with action whitelist
    pub fn with_allowed_actions(actions: Vec<String>) -> Self {
        Self {
            allowed_actions: actions.into_iter().collect(),
            ..Default::default()
        }
    }
    
    /// Add allowed action
    pub fn allow_action(&mut self, action: String) {
        self.allowed_actions.insert(action);
    }
    
    /// Set maximum steps
    pub fn set_max_steps(&mut self, max: usize) {
        self.max_steps = max;
    }
    
    /// Validate workflow definition
    pub fn validate(&self, workflow: &WorkflowDefinition) -> Result<()> {
        // Check step count
        if workflow.steps.len() > self.max_steps {
            return Err(zoey_core::ZoeyError::Validation(format!(
                "Workflow has {} steps, maximum is {}",
                workflow.steps.len(),
                self.max_steps
            )));
        }
        
        // Check for cycles
        workflow.check_cycles().map_err(|e| {
            zoey_core::ZoeyError::Validation(e)
        })?;
        
        // Validate dependencies
        workflow.validate_dependencies().map_err(|e| {
            zoey_core::ZoeyError::Validation(e)
        })?;
        
        // Validate each step
        for step in &workflow.steps {
            self.validate_step(step)?;
        }
        
        // Check dependency depth
        let max_depth = self.calculate_max_depth(workflow)?;
        if max_depth > self.max_depth {
            return Err(zoey_core::ZoeyError::Validation(format!(
                "Workflow dependency depth {} exceeds maximum {}",
                max_depth, self.max_depth
            )));
        }
        
        Ok(())
    }
    
    /// Validate individual step
    fn validate_step(&self, step: &crate::workflow::WorkflowStep) -> Result<()> {
        // Check action is allowed
        if !self.allowed_actions.is_empty() && !self.allowed_actions.contains(&step.action) {
            return Err(zoey_core::ZoeyError::Validation(format!(
                "Action '{}' is not allowed in step '{}'",
                step.action, step.name
            )));
        }
        
        // Check retries
        if step.max_retries > self.max_retries {
            return Err(zoey_core::ZoeyError::Validation(format!(
                "Step '{}' has {} retries, maximum is {}",
                step.name, step.max_retries, self.max_retries
            )));
        }
        
        // Check timeout
        if step.timeout_ms > self.max_timeout_ms {
            return Err(zoey_core::ZoeyError::Validation(format!(
                "Step '{}' has timeout {}ms, maximum is {}ms",
                step.name, step.timeout_ms, self.max_timeout_ms
            )));
        }
        
        // Validate step name (no special characters that could cause issues)
        if step.name.is_empty() {
            return Err(zoey_core::ZoeyError::Validation(
                "Step name cannot be empty".to_string()
            ));
        }
        
        if step.name.contains('.') || step.name.contains('[') || step.name.contains(']') {
            return Err(zoey_core::ZoeyError::Validation(format!(
                "Step name '{}' contains invalid characters",
                step.name
            )));
        }
        
        Ok(())
    }
    
    /// Calculate maximum dependency depth
    fn calculate_max_depth(&self, workflow: &WorkflowDefinition) -> Result<usize> {
        use std::collections::HashMap;
        
        // Build dependency graph
        let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
        for step in &workflow.steps {
            graph.insert(step.name.as_str(), step.depends_on.iter().map(|s| s.as_str()).collect());
        }
        
        // Calculate depth for each step
        let mut depths: HashMap<&str, usize> = HashMap::new();
        
        fn calculate_depth<'a>(
            node: &'a str,
            graph: &HashMap<&'a str, Vec<&'a str>>,
            depths: &mut HashMap<&'a str, usize>,
        ) -> usize {
            if let Some(&depth) = depths.get(node) {
                return depth;
            }
            
            let deps = graph.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
            let max_dep_depth = deps.iter()
                .map(|dep| calculate_depth(dep, graph, depths))
                .max()
                .unwrap_or(0);
            
            let depth = max_dep_depth + 1;
            depths.insert(node, depth);
            depth
        }
        
        let mut max_depth = 0;
        for step in &workflow.steps {
            let depth = calculate_depth(step.name.as_str(), &graph, &mut depths);
            max_depth = max_depth.max(depth);
        }
        
        Ok(max_depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_default() {
        let validator = WorkflowValidator::default();
        assert_eq!(validator.max_steps, 100);
        assert_eq!(validator.max_depth, 20);
    }

    #[test]
    fn test_validator_with_whitelist() {
        let validator = WorkflowValidator::with_allowed_actions(
            vec!["REPLY".to_string(), "SEND_MESSAGE".to_string()]
        );
        assert_eq!(validator.allowed_actions.len(), 2);
    }

    #[test]
    fn test_validate_step_count() {
        let validator = WorkflowValidator::new();
        let mut validator = validator;
        validator.set_max_steps(2);
        
        let yaml = r#"
name: test
version: "1.0"
steps:
  - name: step1
    action: REPLY
  - name: step2
    action: REPLY
  - name: step3
    action: REPLY
"#;
        
        let workflow = crate::workflow::WorkflowDefinition::from_yaml(yaml).unwrap();
        assert!(validator.validate(&workflow).is_err());
    }
}

