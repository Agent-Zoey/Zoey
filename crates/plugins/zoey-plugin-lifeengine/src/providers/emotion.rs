//! Emotion Provider
//!
//! Provides current emotional state context to LLM prompts.

use crate::types::{DiscreteEmotion, EmotionalState};
use async_trait::async_trait;
use std::sync::Arc;
use zoey_core::{types::*, Result};

/// Provides emotional state context to LLM prompts
pub struct EmotionProvider;

impl EmotionProvider {
    /// Create a new provider
    pub fn new() -> Self {
        Self
    }
}

impl Default for EmotionProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for EmotionProvider {
    fn name(&self) -> &str {
        "emotion"
    }
    
    fn description(&self) -> Option<String> {
        Some("Provides current emotional state and mood context".to_string())
    }
    
    fn position(&self) -> i32 {
        -5 // Run after soul_state but before others
    }
    
    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        state: &State,
    ) -> Result<ProviderResult> {
        let mut result = ProviderResult::default();
        
        // Try to restore emotional state from state cache
        let emotional_state = if let Some(emotion_data) = state.get_data("emotional_state") {
            serde_json::from_value(emotion_data.clone()).unwrap_or_else(|_| EmotionalState::new())
        } else {
            EmotionalState::new()
        };
        
        // Generate emotion context
        let context = emotional_state.to_context();
        
        tracing::info!(
            primary_emotion = ?emotional_state.primary_emotion,
            intensity = %emotional_state.intensity,
            valence = %emotional_state.affect.valence,
            arousal = %emotional_state.affect.arousal,
            "EmotionProvider: Providing emotional context"
        );
        
        result.text = Some(context.clone());
        result.values = Some({
            let mut values = std::collections::HashMap::new();
            values.insert("EMOTION_CONTEXT".to_string(), context);
            values.insert(
                "PRIMARY_EMOTION".to_string(),
                format!("{:?}", emotional_state.primary_emotion).to_lowercase(),
            );
            values.insert(
                "EMOTION_INTENSITY".to_string(),
                format!("{:.2}", emotional_state.intensity),
            );
            values.insert(
                "EMOTIONAL_VALENCE".to_string(),
                format!("{:.2}", emotional_state.affect.valence),
            );
            values
        });
        
        result.data = Some({
            let mut data = std::collections::HashMap::new();
            data.insert(
                "primary_emotion".to_string(),
                serde_json::json!(format!("{:?}", emotional_state.primary_emotion)),
            );
            data.insert(
                "intensity".to_string(),
                serde_json::json!(emotional_state.intensity),
            );
            data.insert(
                "valence".to_string(),
                serde_json::json!(emotional_state.affect.valence),
            );
            data.insert(
                "arousal".to_string(),
                serde_json::json!(emotional_state.affect.arousal),
            );
            data.insert(
                "dominance".to_string(),
                serde_json::json!(emotional_state.affect.dominance),
            );
            
            // Add secondary emotions
            let secondary: std::collections::HashMap<String, f32> = emotional_state.secondary_emotions
                .iter()
                .map(|(e, v)| (format!("{:?}", e).to_lowercase(), *v))
                .collect();
            data.insert(
                "secondary_emotions".to_string(),
                serde_json::json!(secondary),
            );
            
            data
        });
        
        Ok(result)
    }
}

/// Generate style hints based on emotional state
pub fn emotion_to_style_hints(state: &EmotionalState) -> Vec<String> {
    let mut hints = Vec::new();
    
    // Based on primary emotion
    match state.primary_emotion {
        DiscreteEmotion::Joy => {
            hints.push("Express positivity naturally".to_string());
            hints.push("Mirror enthusiastic energy".to_string());
        }
        DiscreteEmotion::Sadness => {
            hints.push("Be gentle and supportive".to_string());
            hints.push("Acknowledge feelings".to_string());
        }
        DiscreteEmotion::Anger | DiscreteEmotion::Contempt => {
            hints.push("Remain calm and respectful".to_string());
            hints.push("Validate frustrations".to_string());
        }
        DiscreteEmotion::Fear => {
            hints.push("Be reassuring".to_string());
            hints.push("Provide clear information".to_string());
        }
        DiscreteEmotion::Trust | DiscreteEmotion::Love => {
            hints.push("Maintain warm connection".to_string());
            hints.push("Be open and authentic".to_string());
        }
        _ => {}
    }
    
    // Based on arousal level
    if state.affect.arousal > 0.7 {
        hints.push("Match energetic pace".to_string());
    } else if state.affect.arousal < 0.3 {
        hints.push("Use calming tone".to_string());
    }
    
    // Based on valence
    if state.affect.valence < -0.5 {
        hints.push("Prioritize empathy over problem-solving".to_string());
    }
    
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_emotion_provider() {
        let provider = EmotionProvider::new();
        assert_eq!(provider.name(), "emotion");
        
        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content::default(),
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };
        
        let state = State::new();
        let result = provider.get(Arc::new(()), &message, &state).await.unwrap();
        
        assert!(result.text.is_some());
        assert!(result.data.is_some());
    }
    
    #[test]
    fn test_emotion_style_hints() {
        let mut state = EmotionalState::new();
        state.process_event("test", DiscreteEmotion::Sadness, 0.8);
        
        let hints = emotion_to_style_hints(&state);
        assert!(!hints.is_empty());
        assert!(hints.iter().any(|h| h.contains("gentle") || h.contains("supportive")));
    }
}

