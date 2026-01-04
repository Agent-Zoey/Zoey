//! Goal tracking evaluator - tracks conversation goals and progress

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

/// Goal tracking evaluator
pub struct GoalTrackingEvaluator;

#[async_trait]
impl Evaluator for GoalTrackingEvaluator {
    fn name(&self) -> &str {
        "goal_tracking"
    }

    fn description(&self) -> &str {
        "Tracks conversation goals and monitors progress towards achieving them"
    }

    fn examples(&self) -> Vec<EvaluationExample> {
        vec![EvaluationExample {
            prompt: "Track conversation goals".to_string(),
            messages: vec![ActionExample {
                name: "User".to_string(),
                text: "I want to learn Rust programming".to_string(),
            }],
            outcome: "Goal identified: Learn Rust programming. Status: Not started".to_string(),
        }]
    }

    fn always_run(&self) -> bool {
        false // Run selectively
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        // Simple heuristic: look for goal-related keywords
        let text = message.content.text.to_lowercase();
        Ok(text.contains("want to")
            || text.contains("goal")
            || text.contains("trying to")
            || text.contains("need to")
            || text.contains("planning to"))
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _did_respond: bool,
        responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        // In real implementation, would:
        // 1. Use LLM to identify stated goals
        // 2. Create goal records in database
        // 3. Track progress over time
        // 4. Check if goals have been achieved
        // 5. Remind user of incomplete goals

        tracing::debug!("Goal tracking: Analyzing message {} for goals", message.id);

        let text = &message.content.text;
        tracing::info!("Detected potential goal in message: {}", text);

        if let Some(resps) = responses {
            tracing::debug!("Checking {} responses for goal progress", resps.len());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_goal_tracking_evaluator() {
        let evaluator = GoalTrackingEvaluator;
        assert_eq!(evaluator.name(), "goal_tracking");
        assert!(!evaluator.always_run());

        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "I want to build a web application".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        let state = State::new();
        let should_run = evaluator
            .validate(Arc::new(()), &message, &state)
            .await
            .unwrap();
        assert!(should_run);

        let result = evaluator
            .handler(Arc::new(()), &message, &state, true, None)
            .await;
        assert!(result.is_ok());
    }
}
