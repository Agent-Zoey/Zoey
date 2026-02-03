//! Teacher selection for dynamic model routing
//!
//! This module provides intelligent teacher/model selection based on:
//! - Task complexity assessment
//! - Task type classification
//! - Model capabilities (reasoning, generation, coding)
//! - Routing preferences (speed, quality, balanced)

use crate::planner::complexity::{ComplexityAssessment, ComplexityLevel};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Task type for routing (compatible with local provider)
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
    /// Reasoning/analysis (new for CoT)
    Reasoning,
    /// Critique/review (new for CoT)
    Critique,
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskType::Chat => write!(f, "chat"),
            TaskType::CodeGeneration => write!(f, "code_generation"),
            TaskType::Completion => write!(f, "completion"),
            TaskType::Summarization => write!(f, "summarization"),
            TaskType::QuestionAnswering => write!(f, "question_answering"),
            TaskType::Embedding => write!(f, "embedding"),
            TaskType::Classification => write!(f, "classification"),
            TaskType::Translation => write!(f, "translation"),
            TaskType::Reasoning => write!(f, "reasoning"),
            TaskType::Critique => write!(f, "critique"),
        }
    }
}

/// Routing preference for model selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingPreference {
    /// Prefer faster models
    Speed,
    /// Prefer higher quality models
    Quality,
    /// Balance speed and quality
    Balanced,
}

impl Default for RoutingPreference {
    fn default() -> Self {
        Self::Balanced
    }
}

/// Teacher capabilities for specialized tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeacherCapabilities {
    /// Maximum context length in tokens
    pub context_length: usize,
    /// Whether the model supports structured JSON output
    pub supports_structured_output: bool,
    /// Strength for reasoning/chain-of-thought tasks (0.0 - 1.0)
    pub reasoning_strength: f32,
    /// Strength for text generation/drafting (0.0 - 1.0)
    pub generation_strength: f32,
    /// Strength for coding tasks (0.0 - 1.0)
    pub coding_strength: f32,
    /// Strength for summarization tasks (0.0 - 1.0)
    pub summarization_strength: f32,
    /// Strength for critique/review tasks (0.0 - 1.0)
    pub critique_strength: f32,
}

impl Default for TeacherCapabilities {
    fn default() -> Self {
        Self {
            context_length: 4096,
            supports_structured_output: false,
            reasoning_strength: 0.5,
            generation_strength: 0.5,
            coding_strength: 0.5,
            summarization_strength: 0.5,
            critique_strength: 0.5,
        }
    }
}

impl TeacherCapabilities {
    /// Get the strength score for a specific task type
    pub fn strength_for_task(&self, task_type: TaskType) -> f32 {
        match task_type {
            TaskType::Reasoning | TaskType::QuestionAnswering => self.reasoning_strength,
            TaskType::CodeGeneration => self.coding_strength,
            TaskType::Summarization => self.summarization_strength,
            TaskType::Critique => self.critique_strength,
            TaskType::Chat | TaskType::Completion | TaskType::Translation => {
                self.generation_strength
            }
            TaskType::Classification => self.reasoning_strength,
            TaskType::Embedding => 0.5, // Not applicable, neutral score
        }
    }
}

/// Teacher represents a local model specialized for certain tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Teacher {
    /// Unique identifier
    pub id: Uuid,
    /// Display name for the teacher
    pub name: String,
    /// Ollama model name (e.g., "phi3:mini", "llama3.1:8b")
    pub model_name: String,
    /// Task specializations this teacher excels at
    pub specializations: Vec<TaskType>,
    /// Detailed capabilities
    pub capabilities: TeacherCapabilities,
    /// Quality rating (0.0 - 1.0)
    pub quality_rating: f32,
    /// Speed rating (0.0 - 1.0)
    pub speed_rating: f32,
    /// Model size in GB (for memory constraint checking)
    pub size_gb: f32,
    /// Whether this teacher is available (loaded/accessible)
    pub available: bool,
}

impl Teacher {
    /// Create a new teacher
    pub fn new(
        name: impl Into<String>,
        model_name: impl Into<String>,
        specializations: Vec<TaskType>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            model_name: model_name.into(),
            specializations,
            capabilities: TeacherCapabilities::default(),
            quality_rating: 0.5,
            speed_rating: 0.5,
            size_gb: 4.0,
            available: true,
        }
    }

    /// Builder pattern: set capabilities
    pub fn with_capabilities(mut self, capabilities: TeacherCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Builder pattern: set quality rating
    pub fn with_quality_rating(mut self, rating: f32) -> Self {
        self.quality_rating = rating.clamp(0.0, 1.0);
        self
    }

    /// Builder pattern: set speed rating
    pub fn with_speed_rating(mut self, rating: f32) -> Self {
        self.speed_rating = rating.clamp(0.0, 1.0);
        self
    }

    /// Builder pattern: set size
    pub fn with_size_gb(mut self, size: f32) -> Self {
        self.size_gb = size;
        self
    }

    /// Check if teacher supports a task type
    pub fn supports_task(&self, task_type: TaskType) -> bool {
        self.specializations.contains(&task_type)
    }

    /// Calculate composite score for a task with given preference
    pub fn score_for_task(&self, task_type: TaskType, preference: RoutingPreference) -> f32 {
        let task_strength = self.capabilities.strength_for_task(task_type);

        match preference {
            RoutingPreference::Speed => {
                // Weight: 60% speed, 30% task strength, 10% quality
                self.speed_rating * 0.6 + task_strength * 0.3 + self.quality_rating * 0.1
            }
            RoutingPreference::Quality => {
                // Weight: 60% quality, 30% task strength, 10% speed
                self.quality_rating * 0.6 + task_strength * 0.3 + self.speed_rating * 0.1
            }
            RoutingPreference::Balanced => {
                // Weight: 40% task strength, 30% quality, 30% speed
                task_strength * 0.4 + self.quality_rating * 0.3 + self.speed_rating * 0.3
            }
        }
    }
}

/// Teacher selection based on complexity level
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexityTeacherMapping {
    /// Teacher for trivial tasks
    pub trivial: Option<Uuid>,
    /// Teacher for simple tasks
    pub simple: Option<Uuid>,
    /// Teacher for moderate tasks
    pub moderate: Option<Uuid>,
    /// Teacher for complex tasks
    pub complex: Option<Uuid>,
    /// Teacher for very complex tasks
    pub very_complex: Option<Uuid>,
}

impl Default for ComplexityTeacherMapping {
    fn default() -> Self {
        Self {
            trivial: None,
            simple: None,
            moderate: None,
            complex: None,
            very_complex: None,
        }
    }
}

/// Teacher selector that dynamically chooses models based on task requirements
pub struct TeacherSelector {
    /// Available teachers
    teachers: HashMap<Uuid, Teacher>,
    /// Mapping from complexity to preferred teacher
    complexity_mapping: ComplexityTeacherMapping,
    /// Available memory constraint (GB)
    available_memory_gb: f32,
}

impl TeacherSelector {
    /// Create a new teacher selector
    pub fn new() -> Self {
        Self {
            teachers: HashMap::new(),
            complexity_mapping: ComplexityTeacherMapping::default(),
            available_memory_gb: 8.0,
        }
    }

    /// Create with default teachers pre-configured
    pub fn with_default_teachers() -> Self {
        let mut selector = Self::new();
        selector.register_default_teachers();
        selector
    }

    /// Set available memory constraint
    pub fn with_memory_constraint(mut self, memory_gb: f32) -> Self {
        self.available_memory_gb = memory_gb;
        self
    }

    /// Register a teacher
    pub fn register_teacher(&mut self, teacher: Teacher) {
        self.teachers.insert(teacher.id, teacher);
    }

    /// Unregister a teacher
    pub fn unregister_teacher(&mut self, id: &Uuid) -> Option<Teacher> {
        self.teachers.remove(id)
    }

    /// Get a teacher by ID
    pub fn get_teacher(&self, id: &Uuid) -> Option<&Teacher> {
        self.teachers.get(id)
    }

    /// Get all registered teachers
    pub fn get_all_teachers(&self) -> Vec<&Teacher> {
        self.teachers.values().collect()
    }

    /// Get teachers that fit memory constraints
    fn get_available_teachers(&self) -> Vec<&Teacher> {
        self.teachers
            .values()
            .filter(|t| t.available && t.size_gb <= self.available_memory_gb)
            .collect()
    }

    /// Select optimal teacher based on complexity and task type
    pub fn select_teacher(
        &self,
        complexity: &ComplexityAssessment,
        task_type: TaskType,
        preference: RoutingPreference,
    ) -> Option<&Teacher> {
        // First, try to use complexity mapping
        let preferred_id = match complexity.level {
            ComplexityLevel::Trivial => self.complexity_mapping.trivial,
            ComplexityLevel::Simple => self.complexity_mapping.simple,
            ComplexityLevel::Moderate => self.complexity_mapping.moderate,
            ComplexityLevel::Complex => self.complexity_mapping.complex,
            ComplexityLevel::VeryComplex => self.complexity_mapping.very_complex,
        };

        // If we have a preferred teacher and it's available, use it
        if let Some(id) = preferred_id {
            if let Some(teacher) = self.teachers.get(&id) {
                if teacher.available
                    && teacher.size_gb <= self.available_memory_gb
                    && teacher.supports_task(task_type)
                {
                    return Some(teacher);
                }
            }
        }

        // Otherwise, find the best teacher for this task
        let available = self.get_available_teachers();

        // Filter by task support and score
        let mut candidates: Vec<(&Teacher, f32)> = available
            .into_iter()
            .filter(|t| t.supports_task(task_type))
            .map(|t| (t, t.score_for_task(task_type, preference)))
            .collect();

        // Sort by score (highest first)
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        candidates.first().map(|(t, _)| *t)
    }

    /// Select teacher specifically optimized for reasoning/thinking
    pub fn select_for_reasoning(&self, complexity: &ComplexityAssessment) -> Option<&Teacher> {
        let available = self.get_available_teachers();

        // Filter and score by reasoning strength
        let mut candidates: Vec<(&Teacher, f32)> = available
            .into_iter()
            .filter(|t| {
                t.supports_task(TaskType::Reasoning)
                    || t.supports_task(TaskType::QuestionAnswering)
            })
            .map(|t| {
                // Score: reasoning strength + quality (for complex tasks we want quality)
                let quality_bonus = match complexity.level {
                    ComplexityLevel::Complex | ComplexityLevel::VeryComplex => {
                        t.quality_rating * 0.3
                    }
                    _ => 0.0,
                };
                (t, t.capabilities.reasoning_strength + quality_bonus)
            })
            .collect();

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        candidates.first().map(|(t, _)| *t)
    }

    /// Select teacher optimized for response generation/drafting
    pub fn select_for_generation(&self, complexity: &ComplexityAssessment) -> Option<&Teacher> {
        let available = self.get_available_teachers();

        // Filter and score by generation strength
        let mut candidates: Vec<(&Teacher, f32)> = available
            .into_iter()
            .filter(|t| {
                t.supports_task(TaskType::Chat)
                    || t.supports_task(TaskType::Completion)
                    || t.supports_task(TaskType::CodeGeneration)
            })
            .map(|t| {
                // For simple tasks, prefer speed; for complex, prefer quality
                let preference_bonus = match complexity.level {
                    ComplexityLevel::Trivial | ComplexityLevel::Simple => t.speed_rating * 0.2,
                    ComplexityLevel::Complex | ComplexityLevel::VeryComplex => {
                        t.quality_rating * 0.2
                    }
                    _ => 0.0,
                };
                (t, t.capabilities.generation_strength + preference_bonus)
            })
            .collect();

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        candidates.first().map(|(t, _)| *t)
    }

    /// Select teacher optimized for critique/review
    pub fn select_for_critique(&self, _complexity: &ComplexityAssessment) -> Option<&Teacher> {
        let available = self.get_available_teachers();

        // Filter and score by critique strength (need quality for good critique)
        let mut candidates: Vec<(&Teacher, f32)> = available
            .into_iter()
            .map(|t| {
                (
                    t,
                    t.capabilities.critique_strength * 0.5 + t.quality_rating * 0.5,
                )
            })
            .collect();

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        candidates.first().map(|(t, _)| *t)
    }

    /// Select teacher for coding tasks
    pub fn select_for_coding(&self, complexity: &ComplexityAssessment) -> Option<&Teacher> {
        let available = self.get_available_teachers();

        let mut candidates: Vec<(&Teacher, f32)> = available
            .into_iter()
            .filter(|t| t.supports_task(TaskType::CodeGeneration))
            .map(|t| {
                let quality_bonus = match complexity.level {
                    ComplexityLevel::Complex | ComplexityLevel::VeryComplex => {
                        t.quality_rating * 0.3
                    }
                    _ => t.speed_rating * 0.2,
                };
                (t, t.capabilities.coding_strength + quality_bonus)
            })
            .collect();

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        candidates.first().map(|(t, _)| *t)
    }

    /// Register default teachers with common local models
    fn register_default_teachers(&mut self) {
        // Fast responder - phi3:mini
        let fast_responder = Teacher::new(
            "FastResponder",
            "phi3:mini",
            vec![
                TaskType::Chat,
                TaskType::Completion,
                TaskType::QuestionAnswering,
            ],
        )
        .with_capabilities(TeacherCapabilities {
            context_length: 4096,
            supports_structured_output: false,
            reasoning_strength: 0.5,
            generation_strength: 0.8,
            coding_strength: 0.4,
            summarization_strength: 0.6,
            critique_strength: 0.4,
        })
        .with_quality_rating(0.5)
        .with_speed_rating(0.95)
        .with_size_gb(2.3);

        // Balanced reasoner - qwen2.5:3b
        let reasoner = Teacher::new(
            "Reasoner",
            "qwen2.5:3b",
            vec![
                TaskType::Chat,
                TaskType::QuestionAnswering,
                TaskType::Reasoning,
                TaskType::CodeGeneration,
            ],
        )
        .with_capabilities(TeacherCapabilities {
            context_length: 32768,
            supports_structured_output: true,
            reasoning_strength: 0.85,
            generation_strength: 0.7,
            coding_strength: 0.75,
            summarization_strength: 0.7,
            critique_strength: 0.65,
        })
        .with_quality_rating(0.7)
        .with_speed_rating(0.85)
        .with_size_gb(2.0);

        // Quality generator - llama3.2:3b
        let generator = Teacher::new(
            "Generator",
            "llama3.2:3b",
            vec![
                TaskType::Chat,
                TaskType::Completion,
                TaskType::Translation,
                TaskType::Summarization,
            ],
        )
        .with_capabilities(TeacherCapabilities {
            context_length: 8192,
            supports_structured_output: false,
            reasoning_strength: 0.6,
            generation_strength: 0.85,
            coding_strength: 0.5,
            summarization_strength: 0.8,
            critique_strength: 0.55,
        })
        .with_quality_rating(0.75)
        .with_speed_rating(0.8)
        .with_size_gb(4.0);

        // Quality model - llama3.1:8b
        let quality_model = Teacher::new(
            "QualityExpert",
            "llama3.1:8b",
            vec![
                TaskType::Chat,
                TaskType::QuestionAnswering,
                TaskType::Reasoning,
                TaskType::CodeGeneration,
                TaskType::Summarization,
                TaskType::Translation,
                TaskType::Critique,
            ],
        )
        .with_capabilities(TeacherCapabilities {
            context_length: 131072,
            supports_structured_output: true,
            reasoning_strength: 0.9,
            generation_strength: 0.85,
            coding_strength: 0.8,
            summarization_strength: 0.85,
            critique_strength: 0.85,
        })
        .with_quality_rating(0.9)
        .with_speed_rating(0.5)
        .with_size_gb(8.0);

        // Code expert - codellama:13b
        let code_expert = Teacher::new(
            "CodeExpert",
            "codellama:13b",
            vec![TaskType::CodeGeneration, TaskType::Completion],
        )
        .with_capabilities(TeacherCapabilities {
            context_length: 16384,
            supports_structured_output: false,
            reasoning_strength: 0.7,
            generation_strength: 0.75,
            coding_strength: 0.95,
            summarization_strength: 0.5,
            critique_strength: 0.6,
        })
        .with_quality_rating(0.95)
        .with_speed_rating(0.3)
        .with_size_gb(13.0);

        // Register teachers
        let fast_id = fast_responder.id;
        let reasoner_id = reasoner.id;
        let quality_id = quality_model.id;

        self.register_teacher(fast_responder);
        self.register_teacher(reasoner);
        self.register_teacher(generator);
        self.register_teacher(quality_model);
        self.register_teacher(code_expert);

        // Set up complexity mapping
        self.complexity_mapping = ComplexityTeacherMapping {
            trivial: Some(fast_id),
            simple: Some(fast_id),
            moderate: Some(reasoner_id),
            complex: Some(quality_id),
            very_complex: Some(quality_id),
        };
    }

    /// Set complexity mapping
    pub fn set_complexity_mapping(&mut self, mapping: ComplexityTeacherMapping) {
        self.complexity_mapping = mapping;
    }

    /// Update memory constraint
    pub fn set_memory_constraint(&mut self, memory_gb: f32) {
        self.available_memory_gb = memory_gb;
    }
}

impl Default for TeacherSelector {
    fn default() -> Self {
        Self::with_default_teachers()
    }
}

/// Result of teacher selection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeacherSelection {
    /// Selected teacher
    pub teacher: Teacher,
    /// Why this teacher was selected
    pub reason: String,
    /// Score for this selection
    pub score: f32,
    /// Alternative teachers considered
    pub alternatives: Vec<(Uuid, f32)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::complexity::{ComplexityFactors, TokenEstimate};

    fn create_test_complexity(level: ComplexityLevel) -> ComplexityAssessment {
        ComplexityAssessment {
            level,
            reasoning: "Test".to_string(),
            estimated_steps: 1,
            estimated_tokens: TokenEstimate {
                input_tokens: 100,
                output_tokens: 100,
                total_tokens: 200,
                confidence: 0.8,
            },
            confidence: 0.8,
            factors: ComplexityFactors {
                length_score: 0.3,
                question_score: 0.3,
                domain_score: 0.3,
                context_score: 0.3,
                reasoning_score: 0.3,
            },
        }
    }

    #[test]
    fn test_teacher_selection_trivial() {
        let selector = TeacherSelector::with_default_teachers();
        let complexity = create_test_complexity(ComplexityLevel::Trivial);

        let teacher = selector
            .select_teacher(&complexity, TaskType::Chat, RoutingPreference::Speed)
            .unwrap();

        // Should select fast model for trivial task
        assert!(teacher.speed_rating > 0.8);
    }

    #[test]
    fn test_teacher_selection_complex() {
        let selector = TeacherSelector::with_default_teachers();
        let complexity = create_test_complexity(ComplexityLevel::Complex);

        let teacher = selector
            .select_teacher(&complexity, TaskType::Reasoning, RoutingPreference::Quality)
            .unwrap();

        // Should select high quality model for complex reasoning
        assert!(teacher.quality_rating > 0.7);
    }

    #[test]
    fn test_select_for_reasoning() {
        let selector = TeacherSelector::with_default_teachers();
        let complexity = create_test_complexity(ComplexityLevel::Moderate);

        let teacher = selector.select_for_reasoning(&complexity).unwrap();

        // Should have good reasoning strength
        assert!(teacher.capabilities.reasoning_strength > 0.5);
    }

    #[test]
    fn test_select_for_coding() {
        let selector = TeacherSelector::with_default_teachers();
        let complexity = create_test_complexity(ComplexityLevel::Complex);

        let teacher = selector.select_for_coding(&complexity).unwrap();

        // Should have good coding strength
        assert!(teacher.capabilities.coding_strength > 0.7);
    }

    #[test]
    fn test_memory_constraint() {
        let selector = TeacherSelector::with_default_teachers().with_memory_constraint(3.0);
        let complexity = create_test_complexity(ComplexityLevel::Complex);

        // With 3GB memory, should only get small models
        let teacher = selector
            .select_teacher(&complexity, TaskType::Chat, RoutingPreference::Quality)
            .unwrap();

        assert!(teacher.size_gb <= 3.0);
    }

    #[test]
    fn test_teacher_score_calculation() {
        let teacher = Teacher::new("Test", "test:model", vec![TaskType::Chat])
            .with_quality_rating(0.8)
            .with_speed_rating(0.6)
            .with_capabilities(TeacherCapabilities {
                generation_strength: 0.9,
                ..Default::default()
            });

        let speed_score = teacher.score_for_task(TaskType::Chat, RoutingPreference::Speed);
        let quality_score = teacher.score_for_task(TaskType::Chat, RoutingPreference::Quality);

        // Speed preference should weight speed higher
        // Quality preference should weight quality higher
        assert!(speed_score < quality_score); // Because speed_rating < quality_rating
    }
}
