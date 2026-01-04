//! Soul State Provider
//!
//! Provides comprehensive soul state context including personality,
//! identity, and current behavioral mode.

use crate::types::SoulConfig;
use async_trait::async_trait;
use std::sync::Arc;
use zoey_core::{types::*, Result};

/// Provides soul state context to LLM prompts
pub struct SoulStateProvider {
    /// Default soul configuration
    default_soul: SoulConfig,
}

impl SoulStateProvider {
    /// Create a new provider
    pub fn new() -> Self {
        Self { 
            default_soul: SoulConfig::default(),
        }
    }
    
    /// Create with a custom default soul
    pub fn with_soul(soul: SoulConfig) -> Self {
        Self { default_soul: soul }
    }
}

impl Default for SoulStateProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for SoulStateProvider {
    fn name(&self) -> &str {
        "soul_state"
    }
    
    fn description(&self) -> Option<String> {
        Some("Provides soul personality, identity, and behavioral mode context".to_string())
    }
    
    fn position(&self) -> i32 {
        -10 // Run early to set soul context
    }
    
    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        state: &State,
    ) -> Result<ProviderResult> {
        let mut result = ProviderResult::default();
        
        // Generate soul context from configuration
        let context = self.default_soul.to_context();
        
        // Check state for any soul-related overrides
        let mode = state
            .get_value("soul_mode")
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        
        tracing::info!(
            soul_name = %self.default_soul.name,
            mode = %mode,
            context_len = context.len(),
            "SoulStateProvider: Providing soul context"
        );
        
        result.text = Some(context.clone());
        result.values = Some({
            let mut values = std::collections::HashMap::new();
            values.insert("SOUL_CONTEXT".to_string(), context);
            values.insert("SOUL_MODE".to_string(), mode);
            values.insert("SOUL_NAME".to_string(), self.default_soul.name.clone());
            values
        });
        
        result.data = Some({
            let mut data = std::collections::HashMap::new();
            data.insert("soul_name".to_string(), serde_json::json!(self.default_soul.name));
            data.insert("personality".to_string(), serde_json::json!({
                "openness": self.default_soul.personality.openness,
                "conscientiousness": self.default_soul.personality.conscientiousness,
                "extraversion": self.default_soul.personality.extraversion,
                "agreeableness": self.default_soul.personality.agreeableness,
                "neuroticism": self.default_soul.personality.neuroticism,
            }));
            data
        });
        
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_soul_state_provider() {
        let provider = SoulStateProvider::new();
        assert_eq!(provider.name(), "soul_state");
        
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
    }
}

