//! Soul Reflection Evaluator
//!
//! Periodic self-reflection on conversation quality and soul state.

use async_trait::async_trait;
use std::sync::Arc;
use zoey_core::{types::*, Result};

/// Evaluator that performs periodic soul reflection
pub struct SoulReflectionEvaluator {
    /// How often to run reflection (1 in N messages)
    frequency: u8,
}

impl SoulReflectionEvaluator {
    /// Create a new evaluator
    pub fn new() -> Self {
        Self { frequency: 5 }
    }
    
    /// Set reflection frequency
    pub fn with_frequency(mut self, frequency: u8) -> Self {
        self.frequency = frequency.max(1);
        self
    }
}

impl Default for SoulReflectionEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Evaluator for SoulReflectionEvaluator {
    fn name(&self) -> &str {
        "soul_reflection"
    }
    
    fn description(&self) -> &str {
        "Performs periodic self-reflection on conversation and soul state"
    }
    
    fn always_run(&self) -> bool {
        false
    }
    
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        // Run based on frequency
        Ok(rand::random::<u8>() % self.frequency == 0)
    }
    
    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
        did_respond: bool,
        _responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        // Generate reflection based on state data
        let reflection = generate_reflection(state, did_respond);
        
        tracing::debug!(
            "Soul reflection for message {}: {}",
            message.id,
            reflection
        );
        
        Ok(())
    }
}

/// Generate a reflection based on current state
fn generate_reflection(state: &State, did_respond: bool) -> String {
    let mut reflections = Vec::new();
    
    // Check emotional state from state cache
    if let Some(emotion_data) = state.get_data("emotional_state") {
        if let Some(intensity) = emotion_data.get("intensity").and_then(|v| v.as_f64()) {
            if intensity > 0.7 {
                let emotion = emotion_data.get("primary_emotion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                reflections.push(format!(
                    "Strong {} feeling present - should I explore why?",
                    emotion
                ));
            }
        }
        
        if let Some(valence) = emotion_data.get("valence").and_then(|v| v.as_f64()) {
            if valence < -0.3 {
                reflections.push("Detecting some negative emotional undercurrent - worth addressing gently".to_string());
            }
        }
    }
    
    // Response quality reflection
    if did_respond {
        reflections.push("How was my last response? Did it serve the user's needs?".to_string());
    }
    
    // Check turn count
    if let Some(turn_count) = state.get_data("turn_count").and_then(|v| v.as_u64()) {
        if turn_count > 20 {
            reflections.push("Long conversation - check if we're making progress or going in circles".to_string());
        } else if turn_count < 3 {
            reflections.push("Early in conversation - focus on understanding what they need".to_string());
        }
    }
    
    // Check drive states
    if let Some(drive_states) = state.get_data("drive_states") {
        let high_drives: Vec<_> = drive_states.as_object()
            .map(|obj| {
                obj.iter()
                    .filter(|(_, v)| v.as_f64().unwrap_or(0.0) > 0.75)
                    .map(|(k, _)| k.as_str())
                    .collect()
            })
            .unwrap_or_default();
        
        if !high_drives.is_empty() {
            reflections.push(format!(
                "High drives active: {} - letting these guide responses",
                high_drives.join(", ")
            ));
        }
    }
    
    // Combine reflections
    if reflections.is_empty() {
        "Conversation flowing naturally - staying present and attentive".to_string()
    } else {
        reflections.join(". ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SoulConfig;
    
    #[test]
    fn test_generate_reflection() {
        let state = State::default();
        let reflection = generate_reflection(&state, true);
        assert!(!reflection.is_empty());
    }
    
    #[tokio::test]
    async fn test_soul_reflection_evaluator() {
        let evaluator = SoulReflectionEvaluator::new();
        assert_eq!(evaluator.name(), "soul_reflection");
        
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
        let result = evaluator.handler(Arc::new(()), &message, &state, true, None).await;
        
        assert!(result.is_ok());
    }
}

