#[derive(Debug, Clone)]
pub struct LockHealthStatus {
    pub is_healthy: bool,
    pub total_poisoned: u64,
    pub recoveries: u64,
    pub failures: u64,
    pub most_poisoned_locks: Vec<(String, u64)>,
}
use super::AgentRuntime;
use crate::runtime::{LockPoisonSummary, LockRecoveryStrategy};
use crate::types::runtime::InitializeOptions;
use crate::Result;
use crate::RuntimeOpts;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

pub fn get_lock_recovery_strategy(rt: &AgentRuntime) -> LockRecoveryStrategy {
    rt.get_lock_recovery_strategy()
}

pub fn set_lock_recovery_strategy(rt: &mut AgentRuntime, strategy: LockRecoveryStrategy) {
    rt.set_lock_recovery_strategy(strategy)
}

pub fn get_lock_poison_metrics(rt: &AgentRuntime) -> LockPoisonSummary {
    rt.get_lock_poison_metrics()
}

pub fn reset_lock_poison_metrics(rt: &AgentRuntime) {
    rt.reset_lock_poison_metrics()
}

pub fn has_poisoned_locks(rt: &AgentRuntime) -> bool {
    rt.has_poisoned_locks()
}

pub fn get_lock_health_status(rt: &AgentRuntime) -> super::lifecycle::LockHealthStatus {
    rt.get_lock_health_status()
}

pub async fn new_runtime(opts: RuntimeOpts) -> Result<Arc<RwLock<AgentRuntime>>> {
    AgentRuntime::new(opts).await
}

pub async fn initialize_runtime(rt: &mut AgentRuntime, options: InitializeOptions) -> Result<()> {
    rt.initialize(options).await
}

pub fn create_run_id(_rt: &AgentRuntime) -> Uuid {
    Uuid::new_v4()
}

pub fn start_run(rt: &mut AgentRuntime) -> Uuid {
    let run_id = Uuid::new_v4();
    *rt.current_run_id.write().unwrap() = Some(run_id);
    run_id
}

pub fn end_run(rt: &mut AgentRuntime) {
    *rt.current_run_id.write().unwrap() = None;
}

pub fn get_current_run_id(rt: &AgentRuntime) -> Option<Uuid> {
    *rt.current_run_id.read().unwrap()
}
