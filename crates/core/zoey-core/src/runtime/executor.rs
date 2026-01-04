use super::AgentRuntime;
use crate::types::Action;
use crate::types::{ActionResult, Memory, State};
use crate::types::{HandlerCallback, HandlerOptions};
use crate::Result;
use std::sync::Arc;
use tracing::debug;

pub struct Executor;

impl Executor {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute_action(
        &self,
        runtime: &AgentRuntime,
        action_index: usize,
        message: &Memory,
        state: &State,
    ) -> Result<Option<ActionResult>> {
        let actions = runtime.get_actions();
        if let Some(action) = actions.get(action_index) {
            let ok = action.validate(Arc::new(()), message, state).await?;
            if !ok {
                return Ok(None);
            }
            action
                .handler(Arc::new(()), message, state, None, None)
                .await
        } else {
            Ok(None)
        }
    }

    pub async fn execute_all_actions(
        &self,
        runtime: &AgentRuntime,
        message: &Memory,
        state: &State,
    ) -> Result<Vec<ActionResult>> {
        let actions: Vec<Arc<dyn Action>> = runtime.get_actions();
        let mut results = Vec::new();
        for action in actions.iter() {
            debug!("Validating action: {}", action.name());
            if action.validate(Arc::new(()), message, state).await? {
                debug!("Executing action: {}", action.name());
                let opts = HandlerOptions {
                    action_context: None,
                    action_plan: None,
                    custom: std::collections::HashMap::new(),
                };
                let cb: Option<HandlerCallback> = None;
                if let Some(r) = action
                    .handler(Arc::new(()), message, state, Some(opts), cb)
                    .await?
                {
                    results.push(r);
                }
            }
        }
        let id = message.id;
        crate::runtime::RuntimeState::set_action_results(runtime, id, results.clone());
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn executor_constructs() {
        let _ = Executor::new();
    }
}
