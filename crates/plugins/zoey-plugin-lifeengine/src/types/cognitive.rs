//! Cognitive Steps - Functions that transform working memory
//!
//! A CognitiveStep is the fundamental building block of soul reasoning.
//! It takes WorkingMemory as input and produces both transformed memory
//! and a typed response.

use super::{ThoughtFragment, ThoughtSource, ThoughtType, WorkingMemory};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Result of executing a cognitive step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveResult<T> {
    /// The updated working memory
    pub memory: WorkingMemory,
    
    /// The typed output of this step
    pub output: T,
    
    /// Thoughts generated during this step
    pub thoughts_generated: Vec<ThoughtFragment>,
    
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    
    /// Whether this step completed successfully
    pub success: bool,
    
    /// Error message if failed
    pub error: Option<String>,
}

impl<T: Default> CognitiveResult<T> {
    /// Create a successful result
    pub fn success(memory: WorkingMemory, output: T) -> Self {
        Self {
            memory,
            output,
            thoughts_generated: Vec::new(),
            execution_time_ms: 0,
            success: true,
            error: None,
        }
    }
    
    /// Create a failed result
    pub fn failure(memory: WorkingMemory, error: impl Into<String>) -> Self {
        Self {
            memory,
            output: T::default(),
            thoughts_generated: Vec::new(),
            execution_time_ms: 0,
            success: false,
            error: Some(error.into()),
        }
    }
}

/// Configuration for a cognitive step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveConfig {
    /// Name of this step
    pub name: String,
    
    /// Description of what this step does
    pub description: String,
    
    /// Model to use (if AI-powered)
    pub model: Option<String>,
    
    /// Temperature for generation
    pub temperature: f32,
    
    /// Maximum tokens to generate
    pub max_tokens: Option<usize>,
    
    /// Whether to stream responses
    pub stream: bool,
    
    /// Custom parameters
    pub params: HashMap<String, serde_json::Value>,
}

impl Default for CognitiveConfig {
    fn default() -> Self {
        Self {
            name: "unnamed_step".to_string(),
            description: "A cognitive processing step".to_string(),
            model: None,
            temperature: 0.7,
            max_tokens: None,
            stream: false,
            params: HashMap::new(),
        }
    }
}

/// Trait for implementing cognitive steps
/// 
/// Cognitive steps are the building blocks of soul reasoning.
/// They transform working memory and produce typed outputs.
#[async_trait]
pub trait CognitiveStep: Send + Sync {
    /// The output type of this step
    type Output: Clone + Send + Sync;
    
    /// Get the step configuration
    fn config(&self) -> &CognitiveConfig;
    
    /// Execute this cognitive step
    async fn execute(
        &self,
        memory: WorkingMemory,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> CognitiveResult<Self::Output>;
    
    /// Chain this step with another
    fn then<S: CognitiveStep>(self, next: S) -> ChainedStep<Self, S>
    where
        Self: Sized,
    {
        ChainedStep { first: self, second: next }
    }
}

/// A cognitive step chained with another
pub struct ChainedStep<A, B> {
    first: A,
    second: B,
}

/// Common cognitive step types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepType {
    /// Analyzes input and extracts understanding
    Comprehension,
    /// Generates emotional response
    EmotionalProcessing,
    /// Plans actions or responses
    Planning,
    /// Generates output text
    Generation,
    /// Evaluates and reflects
    Evaluation,
    /// Retrieves relevant memories
    Retrieval,
    /// Updates internal state
    StateUpdate,
}

/// Built-in comprehension step - analyzes input to extract understanding
pub struct ComprehensionStep {
    config: CognitiveConfig,
}

impl ComprehensionStep {
    pub fn new() -> Self {
        Self {
            config: CognitiveConfig {
                name: "comprehension".to_string(),
                description: "Analyzes input to extract understanding".to_string(),
                temperature: 0.3,
                ..Default::default()
            },
        }
    }
}

impl Default for ComprehensionStep {
    fn default() -> Self {
        Self::new()
    }
}

/// Output of comprehension step
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComprehensionOutput {
    /// Main topic or intent detected
    pub intent: String,
    /// Entities mentioned
    pub entities: Vec<String>,
    /// Sentiment (-1.0 to 1.0)
    pub sentiment: f32,
    /// Key points extracted
    pub key_points: Vec<String>,
    /// Questions or requests identified
    pub questions: Vec<String>,
    /// Confidence in understanding (0.0 to 1.0)
    pub confidence: f32,
}

#[async_trait]
impl CognitiveStep for ComprehensionStep {
    type Output = ComprehensionOutput;
    
    fn config(&self) -> &CognitiveConfig {
        &self.config
    }
    
    async fn execute(
        &self,
        memory: WorkingMemory,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> CognitiveResult<Self::Output> {
        // Get recent perceptions to analyze
        let perceptions = memory.thoughts_by_type(ThoughtType::Perception);
        
        // Basic analysis (in production, this would use LLM)
        let content: String = perceptions
            .iter()
            .map(|t| t.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        
        let output = ComprehensionOutput {
            intent: "general_conversation".to_string(),
            entities: Vec::new(),
            sentiment: 0.0,
            key_points: if content.is_empty() { vec![] } else { vec![content.clone()] },
            questions: content
                .split('?')
                .filter(|s| !s.trim().is_empty())
                .map(|s| format!("{}?", s.trim()))
                .collect(),
            confidence: 0.7,
        };
        
        // Add comprehension thought
        let thought = ThoughtFragment::new(
            format!("Understood: intent={}, sentiment={:.2}", output.intent, output.sentiment),
            ThoughtType::Reasoning,
            ThoughtSource::Internal { process: "comprehension".to_string() }
        ).with_salience(0.6);
        
        let mut result = CognitiveResult::success(memory.push(thought.clone()), output);
        result.thoughts_generated.push(thought);
        result
    }
}

/// Emotional processing step - generates emotional response
pub struct EmotionalProcessingStep {
    config: CognitiveConfig,
}

impl EmotionalProcessingStep {
    pub fn new() -> Self {
        Self {
            config: CognitiveConfig {
                name: "emotional_processing".to_string(),
                description: "Processes emotional reactions".to_string(),
                temperature: 0.5,
                ..Default::default()
            },
        }
    }
}

impl Default for EmotionalProcessingStep {
    fn default() -> Self {
        Self::new()
    }
}

/// Output of emotional processing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmotionalOutput {
    /// Primary emotion
    pub primary_emotion: String,
    /// Emotion intensity (0.0 to 1.0)
    pub intensity: f32,
    /// Secondary emotions
    pub secondary_emotions: Vec<(String, f32)>,
    /// Emotional valence (-1.0 to 1.0)
    pub valence: f32,
    /// Emotional arousal (0.0 to 1.0)
    pub arousal: f32,
}

#[async_trait]
impl CognitiveStep for EmotionalProcessingStep {
    type Output = EmotionalOutput;
    
    fn config(&self) -> &CognitiveConfig {
        &self.config
    }
    
    async fn execute(
        &self,
        memory: WorkingMemory,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> CognitiveResult<Self::Output> {
        // Analyze recent thoughts for emotional content
        let recent = memory.most_salient(5);
        
        // Simple emotion detection (would use LLM in production)
        let positive_words = ["happy", "great", "love", "wonderful", "excited", "thanks"];
        let negative_words = ["sad", "angry", "frustrated", "hate", "terrible", "annoyed"];
        
        let text: String = recent.iter().map(|t| t.content.to_lowercase()).collect();
        
        let pos_count = positive_words.iter().filter(|w| text.contains(*w)).count();
        let neg_count = negative_words.iter().filter(|w| text.contains(*w)).count();
        
        let valence = if pos_count + neg_count > 0 {
            (pos_count as f32 - neg_count as f32) / (pos_count + neg_count) as f32
        } else {
            0.0
        };
        
        let output = EmotionalOutput {
            primary_emotion: if valence > 0.3 {
                "positive".to_string()
            } else if valence < -0.3 {
                "negative".to_string()
            } else {
                "neutral".to_string()
            },
            intensity: (pos_count + neg_count) as f32 / 10.0,
            secondary_emotions: Vec::new(),
            valence,
            arousal: 0.5,
        };
        
        // Add emotional thought
        let thought = ThoughtFragment::new(
            format!("Feeling {} (valence: {:.2})", output.primary_emotion, output.valence),
            ThoughtType::Emotion,
            ThoughtSource::Internal { process: "emotional_processing".to_string() }
        ).with_salience(0.7);
        
        let mut result = CognitiveResult::success(memory.push(thought.clone()), output);
        result.thoughts_generated.push(thought);
        result
    }
}

/// Response generation step
pub struct GenerationStep {
    config: CognitiveConfig,
}

impl GenerationStep {
    pub fn new() -> Self {
        Self {
            config: CognitiveConfig {
                name: "generation".to_string(),
                description: "Generates response text".to_string(),
                temperature: 0.7,
                ..Default::default()
            },
        }
    }
}

impl Default for GenerationStep {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerationOutput {
    /// Generated response text
    pub text: String,
    /// Confidence in response
    pub confidence: f32,
    /// Alternative responses considered
    pub alternatives: Vec<String>,
}

#[async_trait]
impl CognitiveStep for GenerationStep {
    type Output = GenerationOutput;
    
    fn config(&self) -> &CognitiveConfig {
        &self.config
    }
    
    async fn execute(
        &self,
        memory: WorkingMemory,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> CognitiveResult<Self::Output> {
        // Gather context from working memory
        let context = memory.to_context_string();
        
        // In production, this would call the LLM
        let output = GenerationOutput {
            text: format!("[Generated response based on: {}]", 
                if context.is_empty() { "no context" } else { "working memory" }),
            confidence: 0.8,
            alternatives: Vec::new(),
        };
        
        // Add generation thought
        let thought = ThoughtFragment::new(
            format!("Generated response with confidence {:.2}", output.confidence),
            ThoughtType::Response,
            ThoughtSource::Internal { process: "generation".to_string() }
        ).with_salience(0.8);
        
        let mut result = CognitiveResult::success(memory.push(thought.clone()), output);
        result.thoughts_generated.push(thought);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_comprehension_step() {
        let step = ComprehensionStep::new();
        let memory = WorkingMemory::new().push(
            ThoughtFragment::new(
                "What is the weather like?",
                ThoughtType::Perception,
                ThoughtSource::External { entity_id: None, channel: "chat".to_string() }
            )
        );
        
        let result = step.execute(memory, Arc::new(())).await;
        assert!(result.success);
        assert!(!result.output.questions.is_empty());
    }
    
    #[tokio::test]
    async fn test_emotional_processing() {
        let step = EmotionalProcessingStep::new();
        let memory = WorkingMemory::new().push(
            ThoughtFragment::new(
                "I'm so happy and excited about this!",
                ThoughtType::Perception,
                ThoughtSource::External { entity_id: None, channel: "chat".to_string() }
            )
        );
        
        let result = step.execute(memory, Arc::new(())).await;
        assert!(result.success);
        assert!(result.output.valence > 0.0);
    }
}

