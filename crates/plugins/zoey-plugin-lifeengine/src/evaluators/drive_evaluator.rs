//! Drive Evaluator
//!
//! Updates drive states based on interaction outcomes.

use crate::types::SoulConfig;
use async_trait::async_trait;
use std::sync::Arc;
use zoey_core::{types::*, Result};

/// Evaluator that updates drive states after interactions
pub struct DriveEvaluator;

impl DriveEvaluator {
    /// Create a new evaluator
    pub fn new() -> Self {
        Self
    }
}

impl Default for DriveEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Evaluator for DriveEvaluator {
    fn name(&self) -> &str {
        "drive_update"
    }
    
    fn description(&self) -> &str {
        "Updates drive/motivation states based on interaction outcomes"
    }
    
    fn always_run(&self) -> bool {
        false // Run periodically, not always
    }
    
    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        // Run every 3rd message on average
        Ok(rand::random::<u8>() % 3 == 0)
    }
    
    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        did_respond: bool,
        _responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        let text = message.content.text.to_lowercase();
        
        // Get default drives
        let drives = SoulConfig::default().drives;
        
        let mut updates = Vec::new();
        
        // Analyze drive changes based on message content
        for drive in &drives {
            let mut adjustment = 0.0f64;
            
            // Check satisfiers
            for satisfier in &drive.satisfiers {
                if text.contains(&satisfier.to_lowercase()) {
                    adjustment -= 0.15;
                    tracing::debug!("Drive {} satisfied by '{}'", drive.name, satisfier);
                }
            }
            
            // Check frustrators
            for frustrator in &drive.frustrators {
                if text.contains(&frustrator.to_lowercase()) {
                    adjustment += 0.1;
                    tracing::debug!("Drive {} frustrated by '{}'", drive.name, frustrator);
                }
            }
            
            if adjustment != 0.0 {
                updates.push((drive.name.clone(), adjustment));
            }
        }
        
        // Special drive updates based on interaction patterns
        
        // Connection drive: satisfied by personal disclosure
        if contains_personal_disclosure(&text) {
            updates.push(("connection".to_string(), -0.1));
        }
        
        // Helpfulness drive: satisfied by gratitude
        if contains_gratitude(&text) && did_respond {
            updates.push(("helpfulness".to_string(), -0.2));
        }
        
        // Curiosity drive: increased by questions from user
        if text.contains("?") && text.len() > 20 {
            updates.push(("curiosity".to_string(), 0.05));
        }
        
        // Accuracy drive: frustrated by corrections
        if contains_correction(&text) {
            updates.push(("accuracy".to_string(), 0.15));
        }
        
        tracing::debug!(
            "Drive evaluator: Processed {} drive updates for message {}",
            updates.len(),
            message.id
        );
        
        Ok(())
    }
}

/// Check if text contains personal disclosure
fn contains_personal_disclosure(text: &str) -> bool {
    let indicators = [
        "i feel", "i think", "i believe", "i'm worried", "i'm afraid",
        "i love", "i hate", "my family", "my friend", "my partner",
        "personally", "for me", "in my experience", "when i was",
    ];
    
    indicators.iter().any(|i| text.contains(i))
}

/// Check if text contains gratitude
fn contains_gratitude(text: &str) -> bool {
    let indicators = [
        "thank", "thanks", "appreciate", "grateful", "helpful",
        "that's great", "that's perfect", "exactly what i needed",
    ];
    
    indicators.iter().any(|i| text.contains(i))
}

/// Check if text contains a correction
fn contains_correction(text: &str) -> bool {
    let indicators = [
        "that's wrong", "that's incorrect", "you're wrong", "actually,",
        "not quite", "that's not right", "you made a mistake",
    ];
    
    indicators.iter().any(|i| text.contains(i))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_contains_personal_disclosure() {
        assert!(contains_personal_disclosure("i feel really happy today"));
        assert!(contains_personal_disclosure("in my experience, this works"));
        assert!(!contains_personal_disclosure("how does this function work?"));
    }
    
    #[test]
    fn test_contains_gratitude() {
        assert!(contains_gratitude("thanks so much!"));
        assert!(contains_gratitude("i really appreciate your help"));
        assert!(!contains_gratitude("what time is it?"));
    }
    
    #[test]
    fn test_contains_correction() {
        assert!(contains_correction("actually, that's wrong"));
        assert!(contains_correction("you made a mistake there"));
        assert!(!contains_correction("that's interesting"));
    }
    
    #[tokio::test]
    async fn test_drive_evaluator() {
        let evaluator = DriveEvaluator::new();
        assert_eq!(evaluator.name(), "drive_update");
        
        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "Thanks for helping me!".to_string(),
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

