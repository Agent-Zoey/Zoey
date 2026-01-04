use crate::runtime::AgentRuntime;
use crate::types::{Action, Evaluator, ModelProvider, Provider};
use std::collections::HashMap;
use std::sync::Arc;

pub fn register_provider(rt: &mut AgentRuntime, provider: Arc<dyn Provider>) {
    rt.providers.write().unwrap().push(provider.clone());
    let mut models = rt.models.write().unwrap();
    let entry = models
        .entry("PROVIDER_CAPS".to_string())
        .or_insert_with(Vec::new);
    entry.push(ModelProvider {
        name: provider.name().to_string(),
        handler: Arc::new(|_| Box::pin(async { Ok(String::new()) })),
        priority: 0,
    });
}

pub fn register_action(rt: &mut AgentRuntime, action: Arc<dyn Action>) {
    rt.actions.write().unwrap().push(action);
}

pub fn register_evaluator(rt: &mut AgentRuntime, evaluator: Arc<dyn Evaluator>) {
    rt.evaluators.write().unwrap().push(evaluator);
}

pub fn get_actions(rt: &AgentRuntime) -> Vec<Arc<dyn Action>> {
    rt.actions.read().unwrap().clone()
}

pub fn get_providers(rt: &AgentRuntime) -> Vec<Arc<dyn Provider>> {
    rt.providers.read().unwrap().clone()
}

pub fn get_evaluators(rt: &AgentRuntime) -> Vec<Arc<dyn Evaluator>> {
    rt.evaluators.read().unwrap().clone()
}

pub fn get_models(rt: &AgentRuntime) -> HashMap<String, Vec<ModelProvider>> {
    rt.models.read().unwrap().clone()
}
