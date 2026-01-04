use super::AgentRuntime;
use crate::types::ActionResult;
use crate::types::State;
use crate::types::{IDatabaseAdapter, IMessageService, Service, TaskWorker};
use crate::{types::Memory, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;
type ServiceTypeName = String;
use crate::runtime::legacy::LockRecovery;

pub struct RuntimeState;

impl RuntimeState {
    pub fn new() -> Self {
        Self
    }

    pub fn get_cached_state(&self, runtime: &AgentRuntime, room_id: &str) -> Option<State> {
        runtime.state_cache.read_or_recover().get(room_id).cloned()
    }

    pub fn set_cached_state(&self, runtime: &AgentRuntime, room_id: String, state: State) {
        runtime
            .state_cache
            .write_or_recover()
            .insert(room_id.clone(), state.clone());
    }

    pub fn estimate_load(&self, runtime: &AgentRuntime) -> u32 {
        let actions = runtime.actions.read_or_recover().len();
        let events = runtime.events.read_or_recover().len();
        let services = runtime.services.read_or_recover().len();
        let tasks = runtime.get_task_workers().len();
        let messages = runtime.get_conversation_length();
        (actions + events + services + tasks + messages) as u32
    }

    pub async fn compose_state_impl(
        &self,
        runtime: &AgentRuntime,
        message: &Memory,
        include_list: Option<Vec<String>>,
        only_include: bool,
        skip_cache: bool,
    ) -> Result<State> {
        let mut state = State::new();

        if !skip_cache {
            let cache_key = format!("{}:{}", message.id, message.room_id);
            if let Some(cached_state) = runtime.state_cache.read_or_recover().get(&cache_key) {
                return Ok(cached_state.clone());
            }
        }

        let providers = runtime.providers.read_or_recover();
        let runtime_ref: std::sync::Arc<dyn std::any::Any + Send + Sync> = std::sync::Arc::new(());

        for provider in providers.iter() {
            if only_include {
                if let Some(ref list) = include_list {
                    if !list.contains(&provider.name().to_uppercase()) {
                        continue;
                    }
                }
            }
            if let Some(ref list) = include_list {
                if !only_include && list.contains(&format!("!{}", provider.name().to_uppercase())) {
                    continue;
                }
            }

            debug!("Running provider: {}", provider.name());
            if let Ok(result) = provider.get(runtime_ref.clone(), message, &state).await {
                if let Some(text) = result.text {
                    state.set_value(provider.name().to_uppercase(), text);
                }
                if let Some(values) = result.values {
                    for (k, v) in values {
                        state.set_value(k, v);
                    }
                }
                if let Some(data) = result.data {
                    for (k, v) in data {
                        state.set_data(k, v);
                    }
                }
            }
        }

        let mut last_thoughts: Vec<String> = Vec::new();
        let room_prefix = format!("ui:lastThought:{}:", message.room_id);
        let entries = runtime.get_settings_with_prefix(&room_prefix);
        for (_k, v) in entries {
            last_thoughts.push(v);
        }
        if !last_thoughts.is_empty() {
            let summary = last_thoughts.join(" ");
            state.set_value("CONTEXT_LAST_THOUGHT".to_string(), summary);
        }

        if !skip_cache {
            let cache_key = format!("{}:{}", message.id, message.room_id);
            runtime
                .state_cache
                .write_or_recover()
                .insert(cache_key, state.clone());
        }

        Ok(state)
    }

    pub fn set_setting(rt: &mut AgentRuntime, key: &str, value: Value) {
        rt.settings
            .write_or_recover()
            .insert(key.to_string(), value);
    }

    pub fn get_setting(rt: &AgentRuntime, key: &str) -> Option<Value> {
        rt.settings.read_or_recover().get(key).cloned()
    }

    pub fn get_setting_string(rt: &AgentRuntime, key: &str) -> Option<String> {
        Self::get_setting(rt, key).and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    pub fn get_settings_with_prefix(rt: &AgentRuntime, prefix: &str) -> Vec<(String, String)> {
        let settings = rt.settings.read_or_recover();
        let mut results = Vec::new();
        for (k, v) in settings.iter() {
            if k.starts_with(prefix) {
                if let Some(s) = v.as_str() {
                    results.push((k.clone(), s.to_string()));
                }
            }
        }
        results
    }

    pub fn get_conversation_length(rt: &AgentRuntime) -> usize {
        rt.conversation_length
    }

    pub fn get_service(rt: &AgentRuntime, service_type: &str) -> Option<Arc<dyn Service>> {
        rt.services
            .read_or_recover()
            .get(service_type)
            .and_then(|services| services.first().cloned())
    }

    pub fn get_services_count(rt: &AgentRuntime) -> usize {
        rt.services.read_or_recover().len()
    }

    pub fn get_all_services(rt: &AgentRuntime) -> HashMap<ServiceTypeName, Vec<Arc<dyn Service>>> {
        rt.services.read_or_recover().clone()
    }

    pub fn message_service(rt: &AgentRuntime) -> Option<Arc<dyn IMessageService>> {
        rt.message_service()
    }

    pub fn register_task_worker(rt: &mut AgentRuntime, name: String, worker: Arc<dyn TaskWorker>) {
        rt.register_task_worker(name, worker)
    }

    pub fn get_task_worker(rt: &AgentRuntime, name: &str) -> Option<Arc<dyn TaskWorker>> {
        rt.get_task_worker(name)
    }

    pub fn get_task_workers(rt: &AgentRuntime) -> HashMap<String, Arc<dyn TaskWorker>> {
        rt.get_task_workers()
    }

    pub fn zoey_os(rt: &AgentRuntime) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        rt.zoey_os()
    }

    pub fn set_zoey_os(rt: &mut AgentRuntime, instance: Arc<dyn std::any::Any + Send + Sync>) {
        rt.set_zoey_os(instance)
    }

    pub fn get_adapter(rt: &AgentRuntime) -> Option<Arc<dyn IDatabaseAdapter + Send + Sync>> {
        rt.adapter.read_or_recover().clone()
    }

    pub fn get_action_results(rt: &AgentRuntime, message_id: Uuid) -> Vec<ActionResult> {
        rt.get_action_results(message_id)
    }

    pub fn set_action_results(rt: &AgentRuntime, message_id: Uuid, results: Vec<ActionResult>) {
        rt.set_action_results(message_id, results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_constructs() {
        let _ = RuntimeState::new();
    }
}
