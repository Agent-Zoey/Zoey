//! Graceful shutdown with state persistence
//!
//! Provides mechanisms for:
//! - Graceful service shutdown
//! - State checkpointing
//! - In-flight request handling
//! - Resource cleanup

use crate::{ZoeyError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, RwLock,
};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Semaphore};
use tracing::{debug, error, info, warn};

/// Shutdown signal receiver
pub type ShutdownReceiver = broadcast::Receiver<ShutdownSignal>;

/// Shutdown signal sender
pub type ShutdownSender = broadcast::Sender<ShutdownSignal>;

/// Shutdown signal types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownSignal {
    /// Graceful shutdown - complete in-flight requests
    Graceful,
    /// Immediate shutdown - abort everything
    Immediate,
    /// Checkpoint - save state but continue running
    Checkpoint,
}

/// Shutdown state for tracking progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownState {
    /// Number of in-flight requests
    pub in_flight_requests: u64,
    /// Number of pending tasks
    pub pending_tasks: u64,
    /// Services that have been stopped
    pub stopped_services: Vec<String>,
    /// Services still running
    pub running_services: Vec<String>,
    /// Shutdown start time
    pub shutdown_started_at: Option<i64>,
    /// Whether shutdown is in progress
    pub is_shutting_down: bool,
}

/// Graceful shutdown manager
pub struct ShutdownManager {
    /// Shutdown signal sender
    sender: ShutdownSender,
    /// Shutdown in progress flag
    shutting_down: Arc<AtomicBool>,
    /// In-flight request counter
    in_flight: Arc<AtomicU64>,
    /// Drain semaphore for limiting new requests during shutdown
    drain_semaphore: Arc<Semaphore>,
    /// Registered shutdown hooks (Arc for cloning without holding lock across await)
    hooks: Arc<RwLock<Vec<Arc<dyn ShutdownHook>>>>,
    /// Shutdown timeout
    timeout: Duration,
    /// State persistence handler (Arc for cloning without holding lock across await)
    state_persister: Arc<RwLock<Option<Arc<dyn StatePersister>>>>,
}

impl ShutdownManager {
    /// Create a new shutdown manager
    pub fn new(timeout: Duration) -> Self {
        let (sender, _) = broadcast::channel(16);

        Self {
            sender,
            shutting_down: Arc::new(AtomicBool::new(false)),
            in_flight: Arc::new(AtomicU64::new(0)),
            drain_semaphore: Arc::new(Semaphore::new(1000)), // Max concurrent during drain
            hooks: Arc::new(RwLock::new(Vec::new())),
            timeout,
            state_persister: Arc::new(RwLock::new(None)),
        }
    }

    /// Get a shutdown receiver
    pub fn subscribe(&self) -> ShutdownReceiver {
        self.sender.subscribe()
    }

    /// Check if shutdown is in progress
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    /// Register a shutdown hook
    pub fn register_hook<H: ShutdownHook + 'static>(&self, hook: H) {
        self.hooks.write().unwrap().push(Arc::new(hook));
    }

    /// Set state persister
    pub fn set_state_persister<P: StatePersister + 'static>(&self, persister: P) {
        *self.state_persister.write().unwrap() = Some(Arc::new(persister));
    }

    /// Clear state persister (disable persistence)
    pub fn clear_state_persister(&self) {
        *self.state_persister.write().unwrap() = None;
    }

    /// Track a request (call at request start)
    pub fn track_request(&self) -> Option<RequestGuard> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return None;
        }

        self.in_flight.fetch_add(1, Ordering::SeqCst);
        Some(RequestGuard {
            counter: Arc::clone(&self.in_flight),
        })
    }

    /// Get current in-flight request count
    pub fn in_flight_count(&self) -> u64 {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Initiate graceful shutdown
    pub async fn shutdown(&self) -> Result<ShutdownState> {
        info!("Initiating graceful shutdown...");

        // Mark as shutting down
        if self.shutting_down.swap(true, Ordering::SeqCst) {
            warn!("Shutdown already in progress");
            return Err(ZoeyError::other("Shutdown already in progress"));
        }

        let shutdown_start = Instant::now();
        let shutdown_started_at = chrono::Utc::now().timestamp();

        // Send shutdown signal
        let _ = self.sender.send(ShutdownSignal::Graceful);

        // Wait for in-flight requests to complete
        info!(
            "Waiting for {} in-flight requests to complete...",
            self.in_flight.load(Ordering::SeqCst)
        );

        let drain_result = self.drain_requests().await;
        if let Err(e) = drain_result {
            warn!("Failed to drain all requests: {}", e);
        }

        // Persist state before stopping services
        // Clone the persister Arc to avoid holding RwLockGuard across await
        let persister_opt = self.state_persister.read().unwrap().clone();
        if let Some(persister) = persister_opt {
            info!("Persisting runtime state...");
            if let Err(e) = persister.persist_state().await {
                error!("Failed to persist state: {}", e);
            }
        }

        // Execute shutdown hooks
        // Clone the hooks Vec to avoid holding RwLockGuard across await points
        let mut stopped_services = Vec::new();
        let hooks: Vec<Arc<dyn ShutdownHook>> = self.hooks.read().unwrap().clone();

        for hook in hooks {
            let name = hook.name().to_string();
            info!("Running shutdown hook: {}", name);

            // Now we can safely call async methods since we own an Arc, not a guard
            match tokio::time::timeout(Duration::from_secs(30), hook.on_shutdown()).await {
                Ok(Ok(())) => {
                    stopped_services.push(name);
                }
                Ok(Err(e)) => {
                    error!("Shutdown hook '{}' failed: {}", name, e);
                }
                Err(_) => {
                    error!("Shutdown hook '{}' timed out", name);
                }
            }
        }

        let elapsed = shutdown_start.elapsed();
        info!("Shutdown completed in {:?}", elapsed);

        Ok(ShutdownState {
            in_flight_requests: self.in_flight.load(Ordering::SeqCst),
            pending_tasks: 0,
            stopped_services,
            running_services: Vec::new(),
            shutdown_started_at: Some(shutdown_started_at),
            is_shutting_down: false,
        })
    }

    /// Drain in-flight requests with timeout
    async fn drain_requests(&self) -> Result<()> {
        let start = Instant::now();

        loop {
            let count = self.in_flight.load(Ordering::SeqCst);

            if count == 0 {
                debug!("All requests drained");
                return Ok(());
            }

            if start.elapsed() > self.timeout {
                warn!(
                    "Drain timeout reached with {} requests still in flight",
                    count
                );
                return Err(ZoeyError::other(format!(
                    "Timeout: {} requests still in flight",
                    count
                )));
            }

            debug!("Waiting for {} in-flight requests...", count);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Create a checkpoint (save state without stopping)
    pub async fn checkpoint(&self) -> Result<()> {
        info!("Creating checkpoint...");

        let _ = self.sender.send(ShutdownSignal::Checkpoint);

        // Clone the Arc to avoid holding lock across await
        let persister_opt = self.state_persister.read().unwrap().clone();
        if let Some(persister) = persister_opt {
            persister.persist_state().await?;
        }

        info!("Checkpoint completed");
        Ok(())
    }
}

impl Default for ShutdownManager {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

/// Guard that automatically decrements in-flight counter when dropped
pub struct RequestGuard {
    counter: Arc<AtomicU64>,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Shutdown hook trait for custom cleanup logic
#[async_trait::async_trait]
pub trait ShutdownHook: Send + Sync {
    /// Name of this hook (for logging)
    fn name(&self) -> &str;

    /// Called during shutdown
    async fn on_shutdown(&self) -> Result<()>;

    /// Priority (higher = runs first)
    fn priority(&self) -> i32 {
        0
    }
}

/// State persistence trait
#[async_trait::async_trait]
pub trait StatePersister: Send + Sync {
    /// Persist current state
    async fn persist_state(&self) -> Result<()>;

    /// Restore state
    async fn restore_state(&self) -> Result<()>;
}

/// Runtime state that can be persisted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistableRuntimeState {
    /// Agent ID
    pub agent_id: String,
    /// Current run ID
    pub current_run_id: Option<String>,
    /// Settings snapshot
    pub settings: HashMap<String, serde_json::Value>,
    /// State cache entries
    pub state_cache: HashMap<String, serde_json::Value>,
    /// Pending task IDs
    pub pending_tasks: Vec<String>,
    /// Timestamp
    pub timestamp: i64,
}

/// File-based state persister
pub struct FileStatePersister {
    path: std::path::PathBuf,
    state_provider: Arc<dyn Fn() -> PersistableRuntimeState + Send + Sync>,
}

impl FileStatePersister {
    /// Create a new file-based state persister
    pub fn new<F>(path: std::path::PathBuf, state_provider: F) -> Self
    where
        F: Fn() -> PersistableRuntimeState + Send + Sync + 'static,
    {
        Self {
            path,
            state_provider: Arc::new(state_provider),
        }
    }
}

#[async_trait::async_trait]
impl StatePersister for FileStatePersister {
    async fn persist_state(&self) -> Result<()> {
        let state = (self.state_provider)();
        let json = serde_json::to_string_pretty(&state)
            .map_err(|e| ZoeyError::other(format!("Failed to serialize state: {}", e)))?;

        tokio::fs::write(&self.path, json)
            .await
            .map_err(|e| ZoeyError::other(format!("Failed to write state file: {}", e)))?;

        info!("State persisted to {:?}", self.path);
        Ok(())
    }

    async fn restore_state(&self) -> Result<()> {
        // State restoration is delegated to runtime startup for now
        Ok(())
    }
}

/// Database-based state persister
pub struct DatabaseStatePersister<A> {
    adapter: Arc<A>,
    agent_id: uuid::Uuid,
    state_provider: Arc<dyn Fn() -> PersistableRuntimeState + Send + Sync>,
}

impl<A: Send + Sync + 'static> DatabaseStatePersister<A> {
    /// Create a new database-based state persister
    pub fn new<F>(adapter: Arc<A>, agent_id: uuid::Uuid, state_provider: F) -> Self
    where
        F: Fn() -> PersistableRuntimeState + Send + Sync + 'static,
    {
        Self {
            adapter,
            agent_id,
            state_provider: Arc::new(state_provider),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_manager_creation() {
        let manager = ShutdownManager::new(Duration::from_secs(5));
        assert!(!manager.is_shutting_down());
        assert_eq!(manager.in_flight_count(), 0);
    }

    #[tokio::test]
    async fn test_request_tracking() {
        let manager = ShutdownManager::new(Duration::from_secs(5));

        {
            let _guard = manager.track_request().unwrap();
            assert_eq!(manager.in_flight_count(), 1);

            let _guard2 = manager.track_request().unwrap();
            assert_eq!(manager.in_flight_count(), 2);
        }

        // Guards dropped
        assert_eq!(manager.in_flight_count(), 0);
    }

    #[tokio::test]
    async fn test_shutdown_blocks_new_requests() {
        let manager = ShutdownManager::new(Duration::from_millis(100));

        // Start shutdown in background
        let manager_clone = Arc::new(manager);
        let m = Arc::clone(&manager_clone);
        tokio::spawn(async move {
            let _ = m.shutdown().await;
        });

        // Give shutdown a moment to start
        tokio::time::sleep(Duration::from_millis(10)).await;

        // New requests should be rejected
        assert!(manager_clone.is_shutting_down());
    }

    struct TestHook {
        name: String,
        executed: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl ShutdownHook for TestHook {
        fn name(&self) -> &str {
            &self.name
        }

        async fn on_shutdown(&self) -> Result<()> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_shutdown_hooks() {
        let manager = ShutdownManager::new(Duration::from_secs(1));
        let executed = Arc::new(AtomicBool::new(false));

        manager.register_hook(TestHook {
            name: "test_hook".to_string(),
            executed: Arc::clone(&executed),
        });

        let _ = manager.shutdown().await;

        assert!(executed.load(Ordering::SeqCst));
    }
}
