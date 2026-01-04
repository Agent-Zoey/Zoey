/*!
# Pipeline Orchestration

Provides data pipeline management.
*/

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Pipeline stage result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    /// Stage name
    pub stage_name: String,

    /// Success flag
    pub success: bool,

    /// Output data
    pub output: Option<serde_json::Value>,

    /// Error message
    pub error: Option<String>,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl StageResult {
    pub fn success(stage_name: String, output: serde_json::Value, duration_ms: u64) -> Self {
        Self {
            stage_name,
            success: true,
            output: Some(output),
            error: None,
            duration_ms,
            metadata: HashMap::new(),
        }
    }

    pub fn failure(stage_name: String, error: String, duration_ms: u64) -> Self {
        Self {
            stage_name,
            success: false,
            output: None,
            error: Some(error),
            duration_ms,
            metadata: HashMap::new(),
        }
    }
}

/// Pipeline stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    /// Stage name
    pub name: String,

    /// Stage type
    pub stage_type: StageType,

    /// Configuration
    pub config: HashMap<String, serde_json::Value>,

    /// Dependencies
    pub dependencies: Vec<String>,

    /// Enabled flag
    pub enabled: bool,
}

impl PipelineStage {
    pub fn new(name: impl Into<String>, stage_type: StageType) -> Self {
        Self {
            name: name.into(),
            stage_type,
            config: HashMap::new(),
            dependencies: Vec::new(),
            enabled: true,
        }
    }

    pub fn with_config(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.config.insert(key.into(), value);
        self
    }

    pub fn depends_on(mut self, stage: impl Into<String>) -> Self {
        self.dependencies.push(stage.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Stage types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageType {
    /// Data extraction
    Extract,
    /// Data transformation
    Transform,
    /// Data loading
    Load,
    /// Data validation
    Validate,
    /// Model training
    Train,
    /// Model inference
    Predict,
    /// Custom stage
    Custom,
}

/// Pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Pipeline name
    pub name: String,

    /// Description
    pub description: String,

    /// Version
    pub version: String,

    /// Enable parallel stages
    pub parallel_execution: bool,

    /// Continue on stage failure
    pub continue_on_failure: bool,

    /// Timeout per stage (seconds)
    pub stage_timeout_secs: u64,

    /// Tags
    pub tags: Vec<String>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            version: "1.0.0".to_string(),
            parallel_execution: false,
            continue_on_failure: false,
            stage_timeout_secs: 300,
            tags: Vec::new(),
        }
    }
}

/// A data pipeline
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// Pipeline ID
    pub id: Uuid,

    /// Configuration
    pub config: PipelineConfig,

    /// Stages
    pub stages: Vec<PipelineStage>,

    /// Stage results
    results: HashMap<String, StageResult>,

    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

impl Pipeline {
    /// Create a new pipeline
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: Uuid::new_v4(),
            config: PipelineConfig {
                name,
                ..Default::default()
            },
            stages: Vec::new(),
            results: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Add a stage
    pub fn add_stage(&mut self, stage: PipelineStage) {
        self.stages.push(stage);
    }

    /// Get stage by name
    pub fn get_stage(&self, name: &str) -> Option<&PipelineStage> {
        self.stages.iter().find(|s| s.name == name)
    }

    /// Store result
    pub fn store_result(&mut self, result: StageResult) {
        self.results.insert(result.stage_name.clone(), result);
    }

    /// Get result
    pub fn get_result(&self, stage_name: &str) -> Option<&StageResult> {
        self.results.get(stage_name)
    }

    /// Get all results
    pub fn results(&self) -> &HashMap<String, StageResult> {
        &self.results
    }

    /// Check if all stages completed
    pub fn is_complete(&self) -> bool {
        self.stages
            .iter()
            .filter(|s| s.enabled)
            .all(|s| self.results.contains_key(&s.name))
    }

    /// Check if any stage failed
    pub fn has_failures(&self) -> bool {
        self.results.values().any(|r| !r.success)
    }

    /// Execute pipeline (simple sequential)
    pub async fn execute(
        &mut self,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, PipelineError> {
        let mut current_data = input;

        for stage in self.stages.clone() {
            if !stage.enabled {
                continue;
            }

            let start = std::time::Instant::now();

            // Check dependencies
            for dep in &stage.dependencies {
                if !self.results.contains_key(dep) {
                    return Err(PipelineError::DependencyNotMet(dep.clone()));
                }
                if !self.results[dep].success {
                    return Err(PipelineError::DependencyFailed(dep.clone()));
                }
            }

            // Execute stage
            let result = self.execute_stage(&stage, &current_data).await;
            let duration = start.elapsed().as_millis() as u64;

            match result {
                Ok(output) => {
                    current_data = output.clone();
                    self.store_result(StageResult::success(stage.name.clone(), output, duration));
                }
                Err(e) => {
                    self.store_result(StageResult::failure(
                        stage.name.clone(),
                        e.to_string(),
                        duration,
                    ));
                    if !self.config.continue_on_failure {
                        return Err(e);
                    }
                }
            }
        }

        Ok(current_data)
    }

    async fn execute_stage(
        &self,
        stage: &PipelineStage,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, PipelineError> {
        // Simulate stage execution based on type
        match stage.stage_type {
            StageType::Extract => {
                // Simulate data extraction
                Ok(input.clone())
            }
            StageType::Transform => {
                // Simulate transformation
                let mut output = input.clone();
                if let Some(obj) = output.as_object_mut() {
                    obj.insert("transformed".to_string(), serde_json::json!(true));
                }
                Ok(output)
            }
            StageType::Load => {
                // Simulate data loading
                Ok(serde_json::json!({"loaded": true, "data": input}))
            }
            StageType::Validate => {
                // Simulate validation
                Ok(serde_json::json!({"valid": true}))
            }
            StageType::Train => {
                // Simulate training
                Ok(serde_json::json!({"trained": true, "model_id": Uuid::new_v4().to_string()}))
            }
            StageType::Predict => {
                // Simulate prediction
                Ok(serde_json::json!({"predictions": [0.1, 0.2, 0.7]}))
            }
            StageType::Custom => {
                // Custom stage - pass through
                Ok(input.clone())
            }
        }
    }

    /// Get summary
    pub fn summary(&self) -> PipelineSummary {
        let successful = self.results.values().filter(|r| r.success).count();
        let failed = self.results.values().filter(|r| !r.success).count();
        let total_duration: u64 = self.results.values().map(|r| r.duration_ms).sum();

        PipelineSummary {
            name: self.config.name.clone(),
            total_stages: self.stages.len(),
            completed_stages: self.results.len(),
            successful_stages: successful,
            failed_stages: failed,
            total_duration_ms: total_duration,
        }
    }
}

/// Pipeline summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineSummary {
    pub name: String,
    pub total_stages: usize,
    pub completed_stages: usize,
    pub successful_stages: usize,
    pub failed_stages: usize,
    pub total_duration_ms: u64,
}

/// Pipeline errors
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Stage execution failed: {0}")]
    StageFailed(String),

    #[error("Dependency not met: {0}")]
    DependencyNotMet(String),

    #[error("Dependency failed: {0}")]
    DependencyFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Timeout at stage: {0}")]
    Timeout(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_creation() {
        let mut pipeline = Pipeline::new("test_pipeline");
        pipeline.add_stage(PipelineStage::new("extract", StageType::Extract));
        pipeline
            .add_stage(PipelineStage::new("transform", StageType::Transform).depends_on("extract"));

        assert_eq!(pipeline.stages.len(), 2);
    }

    #[tokio::test]
    async fn test_pipeline_execution() {
        let mut pipeline = Pipeline::new("test");
        pipeline.add_stage(PipelineStage::new("extract", StageType::Extract));
        pipeline
            .add_stage(PipelineStage::new("transform", StageType::Transform).depends_on("extract"));

        let result = pipeline
            .execute(serde_json::json!({"data": [1, 2, 3]}))
            .await;
        assert!(result.is_ok());
        assert!(pipeline.is_complete());
    }

    #[test]
    fn test_stage_result() {
        let result = StageResult::success("test".to_string(), serde_json::json!({}), 100);
        assert!(result.success);
        assert_eq!(result.duration_ms, 100);
    }
}
