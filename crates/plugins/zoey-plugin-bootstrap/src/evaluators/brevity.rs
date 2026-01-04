use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

pub struct BrevityEvaluator {
    pub max_chars: usize,
}

impl Default for BrevityEvaluator {
    fn default() -> Self {
        Self { max_chars: 600 }
    }
}

#[async_trait]
impl Evaluator for BrevityEvaluator {
    fn name(&self) -> &str {
        "brevity"
    }
    fn description(&self) -> &str {
        "Encourages concise replies; logs if exceeding length"
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
            for r in resps {
                let len = r.content.text.len();
                if len > self.max_chars {
                    tracing::debug!(
                        "BrevityEvaluator: response length {} exceeds {}",
                        len,
                        self.max_chars
                    );
                }
            }
        }
        Ok(())
    }
}
