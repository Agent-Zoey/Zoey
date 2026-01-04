//! Emotion Evaluator
//!
//! Analyzes conversation turns and updates emotional state accordingly.

use crate::types::{DiscreteEmotion, EmotionalState};
use async_trait::async_trait;
use std::sync::Arc;
use zoey_core::{types::*, Result};

/// Evaluator that updates emotional state after each interaction
pub struct EmotionEvaluator;

impl EmotionEvaluator {
    /// Create a new evaluator
    pub fn new() -> Self {
        Self
    }
}

impl Default for EmotionEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Evaluator for EmotionEvaluator {
    fn name(&self) -> &str {
        "emotion_update"
    }
    
    fn description(&self) -> &str {
        "Updates emotional state based on conversation turns"
    }
    
    fn always_run(&self) -> bool {
        true // Always update emotions
    }
    
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }
    
    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        did_respond: bool,
        responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        // Analyze message sentiment
        let sentiment = analyze_sentiment(&message.content.text);
        
        // Map sentiment to emotion
        let (emotion, intensity) = sentiment_to_emotion(sentiment);
        
        // Create emotional state and process
        let mut emotional_state = EmotionalState::new();
        emotional_state.process_event("user_message", emotion, intensity);
        
        // If we responded, check response quality signals
        if did_respond {
            if let Some(responses) = responses {
                for response in &responses {
                    // Analyze our own response for emotional congruence
                    let response_sentiment = analyze_sentiment(&response.content.text);
                    
                    // Self-satisfaction if response aligns with goals
                    if response_sentiment > 0.0 && sentiment < 0.0 {
                        // We responded positively to negative input - empathy
                        emotional_state.process_event(
                            "empathetic_response",
                            DiscreteEmotion::Trust,
                            0.3,
                        );
                    }
                }
            }
        }
        
        // Apply natural emotional decay
        emotional_state.decay();
        
        tracing::debug!(
            "Emotion evaluator: Processed emotional state for message {} - {} (intensity: {:.2})",
            message.id,
            format!("{:?}", emotion).to_lowercase(),
            intensity
        );
        
        Ok(())
    }
}

/// Simple sentiment analysis
fn analyze_sentiment(text: &str) -> f32 {
    let text = text.to_lowercase();
    
    let positive_words = [
        "thank", "thanks", "great", "good", "excellent", "amazing", "wonderful",
        "helpful", "appreciate", "love", "happy", "glad", "perfect", "awesome",
        "fantastic", "brilliant", "nice", "beautiful", "yes", "correct", "right",
    ];
    
    let negative_words = [
        "bad", "terrible", "horrible", "awful", "hate", "angry", "frustrated",
        "annoyed", "sad", "wrong", "incorrect", "useless", "stupid", "idiot",
        "never", "cant", "won't", "fail", "failed", "broken", "problem",
    ];
    
    let intensifiers = ["very", "really", "extremely", "so", "absolutely"];
    
    let mut score = 0.0;
    let mut intensity = 1.0;
    
    // Check for intensifiers
    for word in &intensifiers {
        if text.contains(word) {
            intensity = 1.5;
            break;
        }
    }
    
    // Count positive and negative words
    let pos_count = positive_words.iter().filter(|w| text.contains(*w)).count() as f32;
    let neg_count = negative_words.iter().filter(|w| text.contains(*w)).count() as f32;
    
    if pos_count + neg_count > 0.0 {
        score = (pos_count - neg_count) / (pos_count + neg_count) * intensity;
    }
    
    // Check for explicit sentiment markers
    if text.contains("!") && pos_count > 0.0 {
        score += 0.1;
    }
    
    // Questions often indicate neutral/curious sentiment
    if text.contains("?") && score.abs() < 0.3 {
        score = 0.0;
    }
    
    score.clamp(-1.0, 1.0)
}

/// Map sentiment to emotion and intensity
fn sentiment_to_emotion(sentiment: f32) -> (DiscreteEmotion, f32) {
    let intensity = sentiment.abs() * 0.8 + 0.1; // Scale to 0.1-0.9 range
    
    let emotion = if sentiment > 0.6 {
        DiscreteEmotion::Joy
    } else if sentiment > 0.3 {
        DiscreteEmotion::Trust
    } else if sentiment > -0.3 {
        DiscreteEmotion::Neutral
    } else if sentiment > -0.6 {
        DiscreteEmotion::Sadness
    } else {
        DiscreteEmotion::Anger
    };
    
    (emotion, intensity)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_analyze_sentiment() {
        assert!(analyze_sentiment("Thank you so much!") > 0.5);
        assert!(analyze_sentiment("This is terrible and frustrating") < -0.3);
        assert!(analyze_sentiment("How does this work?").abs() < 0.3);
    }
    
    #[test]
    fn test_sentiment_to_emotion() {
        let (emotion, intensity) = sentiment_to_emotion(0.8);
        assert_eq!(emotion, DiscreteEmotion::Joy);
        assert!(intensity > 0.5);
        
        let (emotion, _) = sentiment_to_emotion(-0.7);
        assert_eq!(emotion, DiscreteEmotion::Anger);
    }
    
    #[tokio::test]
    async fn test_emotion_evaluator() {
        let evaluator = EmotionEvaluator::new();
        assert_eq!(evaluator.name(), "emotion_update");
        assert!(evaluator.always_run());
        
        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "This is great!".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };
        
        let state = State::new();
        let result = evaluator.handler(Arc::new(()), &message, &state, true, None).await;
        
        assert!(result.is_ok());
    }
}

