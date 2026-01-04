use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

pub struct DirectAnswerEvaluator;

#[async_trait]
impl Evaluator for DirectAnswerEvaluator {
    fn name(&self) -> &str {
        "direct_answer"
    }
    fn description(&self) -> &str {
        "Ensures the first sentence answers the main request directly"
    }
    fn always_run(&self) -> bool {
        true
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
        _message: &Memory,
        _state: &State,
        did_respond: bool,
        responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        if !did_respond {
            return Ok(());
        }
        if let Some(resps) = responses {
            if let Some(first) = resps.first() {
                let text = first.content.text.trim();
                let first_sentence = text.split(['.', '!', '?']).next().unwrap_or("").trim();
                let verbose_starts = [
                    "As an AI",
                    "I am an AI",
                    "Sure",
                    "Certainly",
                    "Let me",
                    "Here is",
                    "Here are",
                ];
                let is_verbose = verbose_starts.iter().any(|p| first_sentence.starts_with(p));
                if is_verbose || first_sentence.len() < 3 {
                    tracing::debug!(
                        "DirectAnswerEvaluator: first sentence may not be a direct answer: {}",
                        first_sentence
                    );
                }
            }
        }
        Ok(())
    }
}
