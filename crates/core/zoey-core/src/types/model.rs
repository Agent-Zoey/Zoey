//! Model and LLM types

use crate::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Model type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelType {
    /// Small text model (fast, cheaper)
    TextSmall,
    /// Medium text model (balanced)
    TextMedium,
    /// Large text model (most capable)
    TextLarge,
    /// Embedding model
    TextEmbedding,
    /// Image description/understanding
    ImageDescription,
    /// Image generation
    Image,
    /// Audio processing
    Audio,
    /// Video processing
    Video,
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::TextSmall => write!(f, "TEXT_SMALL"),
            ModelType::TextMedium => write!(f, "TEXT_MEDIUM"),
            ModelType::TextLarge => write!(f, "TEXT_LARGE"),
            ModelType::TextEmbedding => write!(f, "TEXT_EMBEDDING"),
            ModelType::ImageDescription => write!(f, "IMAGE_DESCRIPTION"),
            ModelType::Image => write!(f, "IMAGE"),
            ModelType::Audio => write!(f, "AUDIO"),
            ModelType::Video => write!(f, "VIDEO"),
        }
    }
}

/// Parameters for text generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateTextParams {
    /// Input prompt
    pub prompt: String,

    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,

    /// Temperature (0.0 - 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Top P sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,

    /// Specific model to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Frequency penalty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,

    /// Presence penalty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
}

/// Options for generateText convenience method
#[derive(Debug, Clone, Default)]
pub struct GenerateTextOptions {
    /// Maximum tokens to generate
    pub max_tokens: Option<usize>,

    /// Temperature (0.0 - 2.0)
    pub temperature: Option<f32>,

    /// Top P sampling
    pub top_p: Option<f32>,

    /// Stop sequences
    pub stop: Option<Vec<String>>,
}

/// Result from text generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateTextResult {
    /// Generated text
    pub text: String,

    /// Finish reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,

    /// Tokens used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    /// Prompt tokens
    pub prompt_tokens: usize,

    /// Completion tokens
    pub completion_tokens: usize,

    /// Total tokens
    pub total_tokens: usize,
}

/// Model handler function type
pub type ModelHandler = Arc<dyn Fn(ModelHandlerParams) -> ModelHandlerFuture + Send + Sync>;

/// Future returned by model handlers
pub type ModelHandlerFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>;

/// Parameters passed to model handlers
#[derive(Clone)]
pub struct ModelHandlerParams {
    /// The runtime (type-erased)
    pub runtime: Arc<dyn std::any::Any + Send + Sync>,

    /// Generation parameters
    pub params: GenerateTextParams,
}

/// Model provider information
#[derive(Clone)]
pub struct ModelProvider {
    /// Provider name
    pub name: String,

    /// Model handler function
    pub handler: ModelHandler,

    /// Priority (higher = preferred)
    pub priority: i32,
}

impl std::fmt::Debug for ModelProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelProvider")
            .field("name", &self.name)
            .field("handler", &"<function>")
            .field("priority", &self.priority)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_display() {
        assert_eq!(ModelType::TextLarge.to_string(), "TEXT_LARGE");
        assert_eq!(ModelType::ImageDescription.to_string(), "IMAGE_DESCRIPTION");
    }

    #[test]
    fn test_generate_text_params() {
        let params = GenerateTextParams {
            prompt: "Hello".to_string(),
            max_tokens: Some(100),
            temperature: Some(0.7),
            top_p: None,
            stop: None,
            model: None,
            frequency_penalty: None,
            presence_penalty: None,
        };

        assert_eq!(params.prompt, "Hello");
        assert_eq!(params.max_tokens, Some(100));
    }
}
