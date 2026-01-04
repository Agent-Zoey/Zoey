//! Model Routing Module
//!
//! Routes tasks to optimal local models based on:
//! - Task type (chat, completion, embedding, etc.)
//! - Model capabilities
//! - Hardware constraints

use zoey_core::{ZoeyError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model router that selects optimal models based on task and hardware
pub struct ModelRouter {
    /// Available models and their capabilities
    models: HashMap<String, ModelCapability>,
    /// Hardware constraints
    hardware_constraints: HardwareConstraints,
}

/// Model capability information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapability {
    pub name: String,
    pub backend: String,
    pub size_gb: f64,
    pub context_length: usize,
    pub supported_tasks: Vec<TaskType>,
    pub speed_rating: SpeedRating,
    pub quality_rating: QualityRating,
}

/// Task type for routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    /// Chat/conversation
    Chat,
    /// Code generation
    CodeGeneration,
    /// Text completion
    Completion,
    /// Summarization
    Summarization,
    /// Question answering
    QuestionAnswering,
    /// Text embedding
    Embedding,
    /// Classification
    Classification,
    /// Translation
    Translation,
}

/// Speed rating for models
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SpeedRating {
    VerySlow = 1,
    Slow = 2,
    Medium = 3,
    Fast = 4,
    VeryFast = 5,
}

/// Quality rating for models
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QualityRating {
    Low = 1,
    Medium = 2,
    High = 3,
    VeryHigh = 4,
    Excellent = 5,
}

/// Hardware constraints for model selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareConstraints {
    pub available_memory_gb: f64,
    pub has_gpu: bool,
    pub max_context_length: usize,
}

impl Default for HardwareConstraints {
    fn default() -> Self {
        Self {
            available_memory_gb: 8.0,
            has_gpu: false,
            max_context_length: 4096,
        }
    }
}

/// Routing preference
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingPreference {
    /// Prefer faster models
    Speed,
    /// Prefer higher quality models
    Quality,
    /// Balance speed and quality
    Balanced,
}

impl ModelRouter {
    /// Create a new model router
    pub fn new(hardware_constraints: HardwareConstraints) -> Self {
        let mut router = Self {
            models: HashMap::new(),
            hardware_constraints,
        };

        // Add default model configurations
        router.add_default_models();

        router
    }

    /// Add default model configurations
    fn add_default_models(&mut self) {
        // Small, fast models
        self.add_model(ModelCapability {
            name: "phi3:mini".to_string(),
            backend: "ollama".to_string(),
            size_gb: 2.3,
            context_length: 4096,
            supported_tasks: vec![
                TaskType::Chat,
                TaskType::Completion,
                TaskType::QuestionAnswering,
                TaskType::Summarization,
            ],
            speed_rating: SpeedRating::VeryFast,
            quality_rating: QualityRating::Medium,
        });

        self.add_model(ModelCapability {
            name: "qwen2.5:3b".to_string(),
            backend: "ollama".to_string(),
            size_gb: 2.0,
            context_length: 32768,
            supported_tasks: vec![
                TaskType::Chat,
                TaskType::Completion,
                TaskType::QuestionAnswering,
                TaskType::CodeGeneration,
            ],
            speed_rating: SpeedRating::VeryFast,
            quality_rating: QualityRating::Medium,
        });

        // Medium models
        self.add_model(ModelCapability {
            name: "llama3.2:3b".to_string(),
            backend: "ollama".to_string(),
            size_gb: 4.0,
            context_length: 8192,
            supported_tasks: vec![
                TaskType::Chat,
                TaskType::Completion,
                TaskType::QuestionAnswering,
                TaskType::Summarization,
                TaskType::Translation,
            ],
            speed_rating: SpeedRating::Fast,
            quality_rating: QualityRating::High,
        });

        self.add_model(ModelCapability {
            name: "mistral:7b".to_string(),
            backend: "ollama".to_string(),
            size_gb: 4.5,
            context_length: 8192,
            supported_tasks: vec![
                TaskType::Chat,
                TaskType::Completion,
                TaskType::QuestionAnswering,
                TaskType::CodeGeneration,
                TaskType::Summarization,
            ],
            speed_rating: SpeedRating::Fast,
            quality_rating: QualityRating::High,
        });

        // Large models
        self.add_model(ModelCapability {
            name: "llama3.1:8b".to_string(),
            backend: "ollama".to_string(),
            size_gb: 8.0,
            context_length: 131072,
            supported_tasks: vec![
                TaskType::Chat,
                TaskType::Completion,
                TaskType::QuestionAnswering,
                TaskType::CodeGeneration,
                TaskType::Summarization,
                TaskType::Translation,
            ],
            speed_rating: SpeedRating::Medium,
            quality_rating: QualityRating::VeryHigh,
        });

        self.add_model(ModelCapability {
            name: "codellama:13b".to_string(),
            backend: "ollama".to_string(),
            size_gb: 13.0,
            context_length: 16384,
            supported_tasks: vec![TaskType::CodeGeneration, TaskType::Completion],
            speed_rating: SpeedRating::Slow,
            quality_rating: QualityRating::Excellent,
        });

        // Embedding models
        self.add_model(ModelCapability {
            name: "nomic-embed-text".to_string(),
            backend: "ollama".to_string(),
            size_gb: 0.5,
            context_length: 8192,
            supported_tasks: vec![TaskType::Embedding],
            speed_rating: SpeedRating::VeryFast,
            quality_rating: QualityRating::High,
        });
    }

    /// Add a model to the router
    pub fn add_model(&mut self, capability: ModelCapability) {
        self.models.insert(capability.name.clone(), capability);
    }

    /// Route a task to the optimal model
    pub fn route(&self, task: TaskType, preference: RoutingPreference) -> Result<&ModelCapability> {
        let mut candidates: Vec<&ModelCapability> = self
            .models
            .values()
            .filter(|m| {
                // Model must support the task
                m.supported_tasks.contains(&task) &&
                // Model must fit in available memory
                m.size_gb <= self.hardware_constraints.available_memory_gb
            })
            .collect();

        if candidates.is_empty() {
            return Err(ZoeyError::NotFound(format!(
                "No suitable model found for task {:?} with {} GB available memory",
                task, self.hardware_constraints.available_memory_gb
            )));
        }

        // Sort candidates based on preference
        match preference {
            RoutingPreference::Speed => {
                candidates.sort_by(|a, b| {
                    b.speed_rating
                        .cmp(&a.speed_rating)
                        .then(a.size_gb.partial_cmp(&b.size_gb).unwrap())
                });
            }
            RoutingPreference::Quality => {
                candidates.sort_by(|a, b| {
                    b.quality_rating
                        .cmp(&a.quality_rating)
                        .then(a.size_gb.partial_cmp(&b.size_gb).unwrap())
                });
            }
            RoutingPreference::Balanced => {
                candidates.sort_by(|a, b| {
                    let a_score = (a.speed_rating as i32 + a.quality_rating as i32) as f64;
                    let b_score = (b.speed_rating as i32 + b.quality_rating as i32) as f64;
                    b_score
                        .partial_cmp(&a_score)
                        .unwrap()
                        .then(a.size_gb.partial_cmp(&b.size_gb).unwrap())
                });
            }
        }

        Ok(candidates[0])
    }

    /// Get all models that support a task
    pub fn get_models_for_task(&self, task: TaskType) -> Vec<&ModelCapability> {
        self.models
            .values()
            .filter(|m| {
                m.supported_tasks.contains(&task)
                    && m.size_gb <= self.hardware_constraints.available_memory_gb
            })
            .collect()
    }

    /// Update hardware constraints
    pub fn update_hardware_constraints(&mut self, constraints: HardwareConstraints) {
        self.hardware_constraints = constraints;
    }

    /// Get current hardware constraints
    pub fn get_hardware_constraints(&self) -> &HardwareConstraints {
        &self.hardware_constraints
    }

    /// Get all available models
    pub fn get_all_models(&self) -> Vec<&ModelCapability> {
        self.models.values().collect()
    }
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new(HardwareConstraints::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_chat_task() {
        let router = ModelRouter::default();
        let model = router
            .route(TaskType::Chat, RoutingPreference::Speed)
            .unwrap();

        assert!(model.supported_tasks.contains(&TaskType::Chat));
        assert!(model.size_gb <= 8.0);
    }

    #[test]
    fn test_route_code_generation() {
        let router = ModelRouter::default();
        let model = router
            .route(TaskType::CodeGeneration, RoutingPreference::Quality)
            .unwrap();

        assert!(model.supported_tasks.contains(&TaskType::CodeGeneration));
    }

    #[test]
    fn test_route_embedding() {
        let router = ModelRouter::default();
        let model = router
            .route(TaskType::Embedding, RoutingPreference::Speed)
            .unwrap();

        assert_eq!(model.supported_tasks, vec![TaskType::Embedding]);
    }

    #[test]
    fn test_low_memory_constraint() {
        let constraints = HardwareConstraints {
            available_memory_gb: 2.5,
            has_gpu: false,
            max_context_length: 4096,
        };

        let router = ModelRouter::new(constraints);
        let models = router.get_models_for_task(TaskType::Chat);

        // Should only return small models
        assert!(models.iter().all(|m| m.size_gb <= 2.5));
    }

    #[test]
    fn test_update_constraints() {
        let mut router = ModelRouter::default();

        let new_constraints = HardwareConstraints {
            available_memory_gb: 16.0,
            has_gpu: true,
            max_context_length: 32768,
        };

        router.update_hardware_constraints(new_constraints);

        assert_eq!(router.get_hardware_constraints().available_memory_gb, 16.0);
        assert!(router.get_hardware_constraints().has_gpu);
    }
}
