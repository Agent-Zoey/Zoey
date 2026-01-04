//! State types for Agent API
//!
//! Shared state structures used by server and handlers

use super::auth::ApiAuthManager;
use super::task::TaskManager;
use crate::{security::RateLimiter, AgentRuntime};
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Shared state for API handlers
#[derive(Clone)]
pub struct ApiState {
    /// Agent runtime
    pub runtime: Arc<RwLock<AgentRuntime>>,

    /// Server start time
    pub start_time: Instant,
}

impl ApiState {
    /// Create new API state
    pub fn new(runtime: Arc<RwLock<AgentRuntime>>) -> Self {
        Self {
            runtime,
            start_time: Instant::now(),
        }
    }
}

/// Agent API server state
#[derive(Clone)]
pub struct ServerState {
    /// API state (runtime, etc.)
    pub api_state: ApiState,

    /// Authentication manager
    pub auth_manager: Arc<ApiAuthManager>,

    /// Rate limiter
    pub rate_limiter: Arc<RwLock<RateLimiter>>,

    /// Task manager for async operations
    pub task_manager: TaskManager,

    /// Configuration
    pub config: Arc<super::server::AgentApiConfig>,
}
