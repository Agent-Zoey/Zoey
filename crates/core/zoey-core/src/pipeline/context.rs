use crate::types::*;
use std::sync::{Arc, RwLock};

pub struct PipelineContext {
    pub runtime: Arc<RwLock<crate::AgentRuntime>>,
    pub message: Memory,
    pub state: State,
}

impl PipelineContext {
    pub fn new(runtime: Arc<RwLock<crate::AgentRuntime>>, message: Memory) -> Self {
        Self {
            runtime,
            message,
            state: State::new(),
        }
    }
}
