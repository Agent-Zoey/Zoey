use super::AgentRuntime;
use crate::types::messaging::SendHandlerFunction;
use crate::types::EventHandler;
use std::sync::{Arc, RwLock};

pub struct EventBus;

impl EventBus {
    pub fn new() -> Self {
        Self
    }

    pub fn handlers_for(&self, runtime: &AgentRuntime, event: &str) -> Vec<EventHandler> {
        runtime
            .events
            .read()
            .ok()
            .and_then(|m| m.get(event).cloned())
            .unwrap_or_default()
    }

    pub fn register_handler(&self, runtime: &AgentRuntime, event: String, handler: EventHandler) {
        if let Ok(mut map) = runtime.events.write() {
            map.entry(event).or_insert_with(Vec::new).push(handler);
        }
    }

    pub fn logger(&self, runtime: &AgentRuntime) -> Arc<RwLock<tracing::Span>> {
        runtime.logger()
    }

    pub fn register_send_handler(
        &self,
        runtime: &mut AgentRuntime,
        source: String,
        handler: SendHandlerFunction,
    ) {
        runtime.register_send_handler(source, handler);
    }

    pub fn get_send_handler(
        &self,
        runtime: &AgentRuntime,
        source: &str,
    ) -> Option<SendHandlerFunction> {
        runtime.get_send_handler(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn events_construct() {
        let _ = EventBus::new();
    }
}
