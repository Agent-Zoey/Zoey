//! AgentRuntime implementation
//!
//! # Thread Safety and Lock Ordering
//!
//! The AgentRuntime uses multiple `RwLock`s for thread-safe access to its components.
//! To prevent deadlocks, locks should be acquired in a consistent order when multiple
//! locks are needed simultaneously.
//!
//! ## Lock Ordering (acquire in this order to prevent deadlocks):
//! 1. `plugins` - Plugin registry
//! 2. `actions` - Action handlers
//! 3. `providers` - Context providers
//! 4. `evaluators` - Post-processing evaluators
//! 5. `services` - Long-lived services
//! 6. `models` - Model handlers
//! 7. `events` - Event handlers
//! 8. `routes` - HTTP routes
//! 9. `settings` - Configuration settings
//! 10. `state_cache` - State caching
//! 11. `adapter` - Database adapter
//! 12. `task_workers` - Background workers
//! 13. `send_handlers` - Message send handlers
//! 14. `message_service` - Message queue
//! 15. `action_results` - Action result cache
//! 16. `current_run_id` - Current execution run
//! 17. `observability` - Metrics and monitoring
//! 18. `zoey_os` - Framework integration
//! 19. `logger` - Logging span
//!
//! ## Poisoned Lock Recovery
//!
//! The runtime includes comprehensive poisoned lock handling with multiple strategies:
//!
//! ### Recovery Strategies
//!
//! 1. **AlwaysFail** (default, recommended): Fails fast on poisoned locks
//! 2. **AlwaysRecover**: Always attempts recovery (backward compatible, risky)
//! 3. **RecoverWithLimit**: Recover up to N times, then fail
//! 4. **RecoverWithBackoff**: Recover with exponential backoff delays
//!
//! ### Usage
//!
//! ```rust,ignore
//! use zoey_core::runtime::{RuntimeOpts, LockRecoveryStrategy, AgentRuntime};
//!
//! // Use fail-fast strategy (default, safest)
//! let opts = RuntimeOpts::default();
//!
//! // Or configure custom strategy
//! let opts = RuntimeOpts::default()
//!     .with_lock_recovery_strategy(
//!         LockRecoveryStrategy::RecoverWithLimit { max_recoveries: 3 }
//!     );
//!
//! let runtime = AgentRuntime::default();
//! // Check lock health
//! let health = runtime.get_lock_health_status();
//! if !health.is_healthy {
//!     println!("Warning: {} locks have been poisoned", health.total_poisoned);
//! }
//!
//! // Get metrics
//! let metrics = runtime.get_lock_poison_metrics();
//! println!("Total poisoned: {}", metrics.total_poisoned);
//! ```
//!
//! ### Metrics
//!
//! The runtime tracks:
//! - Total poisoned lock events
//! - Successful recoveries
//! - Failures (when recovery is disabled)
//! - Per-lock poison counts
//! - Last poison timestamp

use crate::dynamic_prompts::{DynamicPromptExecutor, DynamicPromptOptions, SchemaRow};
use crate::error::Result;
use crate::types::*;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Lock recovery strategy configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockRecoveryStrategy {
    /// Always recover from poisoned locks (backward compatible, risky)
    /// May operate on corrupted state
    AlwaysRecover,

    /// Always fail fast on poisoned locks (safest, recommended)
    /// Prevents operating on potentially corrupted state
    AlwaysFail,

    /// Recover up to N times, then fail
    /// Allows some recovery but prevents infinite recovery loops
    RecoverWithLimit { max_recoveries: u64 },

    /// Recover with exponential backoff
    /// Attempts recovery but with increasing delays
    RecoverWithBackoff {
        max_recoveries: u64,
        initial_delay_ms: u64,
    },
}

impl Default for LockRecoveryStrategy {
    fn default() -> Self {
        LockRecoveryStrategy::AlwaysFail
    }
}

/// Metrics for tracking poisoned lock events
#[derive(Debug, Default)]
pub struct LockPoisonMetrics {
    /// Total number of poisoned lock events detected
    pub total_poisoned: AtomicU64,

    /// Number of successful recoveries
    pub recoveries: AtomicU64,

    /// Number of failures (when recovery strategy is fail-fast)
    pub failures: AtomicU64,

    /// Number of read lock poisonings
    pub read_poisoned: AtomicU64,

    /// Number of write lock poisonings
    pub write_poisoned: AtomicU64,

    /// Timestamp of last poisoned lock event
    pub last_poisoned_at: Arc<RwLock<Option<u64>>>,

    /// Map of lock name to poison count
    pub lock_poison_counts: Arc<RwLock<HashMap<String, u64>>>,
}

impl LockPoisonMetrics {
    pub fn new() -> Self {
        Self {
            total_poisoned: AtomicU64::new(0),
            recoveries: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            read_poisoned: AtomicU64::new(0),
            write_poisoned: AtomicU64::new(0),
            last_poisoned_at: Arc::new(RwLock::new(None)),
            lock_poison_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn record_poisoned(&self, lock_name: &str, is_write: bool) {
        self.total_poisoned.fetch_add(1, Ordering::Relaxed);
        if is_write {
            self.write_poisoned.fetch_add(1, Ordering::Relaxed);
        } else {
            self.read_poisoned.fetch_add(1, Ordering::Relaxed);
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if let Ok(mut last) = self.last_poisoned_at.write() {
            *last = Some(timestamp);
        }

        if let Ok(mut counts) = self.lock_poison_counts.write() {
            *counts.entry(lock_name.to_string()).or_insert(0) += 1;
        }
    }

    fn record_recovery(&self) {
        self.recoveries.fetch_add(1, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_summary(&self) -> LockPoisonSummary {
        LockPoisonSummary {
            total_poisoned: self.total_poisoned.load(Ordering::Relaxed),
            recoveries: self.recoveries.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            read_poisoned: self.read_poisoned.load(Ordering::Relaxed),
            write_poisoned: self.write_poisoned.load(Ordering::Relaxed),
            last_poisoned_at: self.last_poisoned_at.read().ok().and_then(|g| *g),
            lock_poison_counts: self
                .lock_poison_counts
                .read()
                .ok()
                .map(|g| g.clone())
                .unwrap_or_default(),
        }
    }
}

/// Summary of lock poison metrics
#[derive(Debug, Clone)]
pub struct LockPoisonSummary {
    pub total_poisoned: u64,
    pub recoveries: u64,
    pub failures: u64,
    pub read_poisoned: u64,
    pub write_poisoned: u64,
    pub last_poisoned_at: Option<u64>,
    pub lock_poison_counts: HashMap<String, u64>,
}

/// Context for lock operations
struct LockContext {
    name: String,
    strategy: LockRecoveryStrategy,
    metrics: Arc<LockPoisonMetrics>,
    recovery_count: Arc<RwLock<HashMap<String, u64>>>,
}

impl LockContext {
    fn new(name: String, strategy: LockRecoveryStrategy, metrics: Arc<LockPoisonMetrics>) -> Self {
        Self {
            name,
            strategy,
            metrics,
            recovery_count: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn should_recover(&self) -> bool {
        match self.strategy {
            LockRecoveryStrategy::AlwaysRecover => true,
            LockRecoveryStrategy::AlwaysFail => false,
            LockRecoveryStrategy::RecoverWithLimit { max_recoveries } => {
                let count = self
                    .recovery_count
                    .read()
                    .ok()
                    .and_then(|g| g.get(&self.name).copied())
                    .unwrap_or(0);
                count < max_recoveries
            }
            LockRecoveryStrategy::RecoverWithBackoff { max_recoveries, .. } => {
                let count = self
                    .recovery_count
                    .read()
                    .ok()
                    .and_then(|g| g.get(&self.name).copied())
                    .unwrap_or(0);
                count < max_recoveries
            }
        }
    }

    fn record_recovery_attempt(&self) {
        if let Ok(mut counts) = self.recovery_count.write() {
            *counts.entry(self.name.clone()).or_insert(0) += 1;
        }
    }
}

/// Helper trait for recovering from poisoned locks
///
/// # Poisoned Lock Handling
///
/// A poisoned lock indicates that a thread panicked while holding the lock.
/// This is a serious error condition that suggests data corruption or inconsistent state.
///
/// This implementation provides multiple strategies:
/// - `AlwaysRecover`: Always recover (backward compatible, risky)
/// - `AlwaysFail`: Always fail fast (safest, recommended)
/// - `RecoverWithLimit`: Recover up to N times, then fail
/// - `RecoverWithBackoff`: Recover with exponential backoff
pub trait LockRecovery<T> {
    /// Acquire read lock with context-aware recovery
    fn read_with_context(
        &self,
        ctx: &LockContext,
    ) -> std::result::Result<RwLockReadGuard<'_, T>, crate::ZoeyError>;

    /// Acquire write lock with context-aware recovery
    fn write_with_context(
        &self,
        ctx: &LockContext,
    ) -> std::result::Result<RwLockWriteGuard<'_, T>, crate::ZoeyError>;

    /// Acquire read lock, recovering from poison if necessary
    ///
    /// # Warning
    /// This method recovers from poisoned locks by extracting the inner value.
    /// This may result in reading inconsistent state. Use `read_or_fail()` for safer behavior.
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T>;

    /// Acquire write lock, recovering from poison if necessary
    ///
    /// # Warning
    /// This method recovers from poisoned locks by extracting the inner value.
    /// This may result in writing to inconsistent state. Use `write_or_fail()` for safer behavior.
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T>;

    /// Acquire read lock, failing fast on poison
    ///
    /// Returns an error if the lock is poisoned, indicating a previous panic.
    /// This is the safer approach as it prevents operating on potentially corrupted state.
    fn read_or_fail(&self) -> std::result::Result<RwLockReadGuard<'_, T>, crate::ZoeyError>;

    /// Acquire write lock, failing fast on poison
    ///
    /// Returns an error if the lock is poisoned, indicating a previous panic.
    /// This is the safer approach as it prevents operating on potentially corrupted state.
    fn write_or_fail(&self) -> std::result::Result<RwLockWriteGuard<'_, T>, crate::ZoeyError>;
}

impl<T> LockRecovery<T> for RwLock<T> {
    fn read_with_context(
        &self,
        ctx: &LockContext,
    ) -> std::result::Result<RwLockReadGuard<'_, T>, crate::ZoeyError> {
        match self.read() {
            Ok(guard) => Ok(guard),
            Err(poisoned) => {
                ctx.metrics.record_poisoned(&ctx.name, false);

                if ctx.should_recover() {
                    ctx.record_recovery_attempt();
                    ctx.metrics.record_recovery();

                    error!(
                        lock_name = %ctx.name,
                        "CRITICAL: Recovered from poisoned read lock. This indicates a previous panic. \
                         State may be inconsistent. Recovery attempt #{}",
                        ctx.recovery_count.read()
                            .ok()
                            .and_then(|g| g.get(&ctx.name).copied())
                            .unwrap_or(0)
                    );

                    #[cfg(any())]
                    {
                        error!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
                    }

                    // Apply backoff if configured
                    if let LockRecoveryStrategy::RecoverWithBackoff {
                        initial_delay_ms, ..
                    } = ctx.strategy
                    {
                        let recovery_count = ctx
                            .recovery_count
                            .read()
                            .ok()
                            .and_then(|g| g.get(&ctx.name).copied())
                            .unwrap_or(0);
                        let delay_ms = initial_delay_ms * (1 << recovery_count.min(10)); // Cap at 2^10
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    }

                    Ok(poisoned.into_inner())
                } else {
                    ctx.metrics.record_failure();

                    error!(
                        lock_name = %ctx.name,
                        "CRITICAL: Attempted to acquire read lock but it is poisoned. \
                         A previous thread panicked while holding this lock. State may be corrupted. \
                         Recovery strategy: {:?}",
                        ctx.strategy
                    );

                    #[cfg(any())]
                    {
                        error!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
                    }

                    Err(crate::ZoeyError::runtime(format!(
                        "Lock '{}' is poisoned - a previous thread panicked while holding this lock. \
                         This indicates a serious error. The runtime state may be corrupted. \
                         Recovery strategy: {:?}",
                        ctx.name, ctx.strategy
                    )))
                }
            }
        }
    }

    fn write_with_context(
        &self,
        ctx: &LockContext,
    ) -> std::result::Result<RwLockWriteGuard<'_, T>, crate::ZoeyError> {
        match self.write() {
            Ok(guard) => Ok(guard),
            Err(poisoned) => {
                ctx.metrics.record_poisoned(&ctx.name, true);

                if ctx.should_recover() {
                    ctx.record_recovery_attempt();
                    ctx.metrics.record_recovery();

                    error!(
                        lock_name = %ctx.name,
                        "CRITICAL: Recovered from poisoned write lock. This indicates a previous panic. \
                         State may be inconsistent. Recovery attempt #{}",
                        ctx.recovery_count.read()
                            .ok()
                            .and_then(|g| g.get(&ctx.name).copied())
                            .unwrap_or(0)
                    );

                    #[cfg(any())]
                    {
                        error!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
                    }

                    // Apply backoff if configured
                    if let LockRecoveryStrategy::RecoverWithBackoff {
                        initial_delay_ms, ..
                    } = ctx.strategy
                    {
                        let recovery_count = ctx
                            .recovery_count
                            .read()
                            .ok()
                            .and_then(|g| g.get(&ctx.name).copied())
                            .unwrap_or(0);
                        let delay_ms = initial_delay_ms * (1 << recovery_count.min(10)); // Cap at 2^10
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    }

                    Ok(poisoned.into_inner())
                } else {
                    ctx.metrics.record_failure();

                    error!(
                        lock_name = %ctx.name,
                        "CRITICAL: Attempted to acquire write lock but it is poisoned. \
                         A previous thread panicked while holding this lock. State may be corrupted. \
                         Recovery strategy: {:?}",
                        ctx.strategy
                    );

                    #[cfg(any())]
                    {
                        error!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
                    }

                    Err(crate::ZoeyError::runtime(format!(
                        "Lock '{}' is poisoned - a previous thread panicked while holding this lock. \
                         This indicates a serious error. The runtime state may be corrupted. \
                         Recovery strategy: {:?}",
                        ctx.name, ctx.strategy
                    )))
                }
            }
        }
    }

    fn read_or_recover(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| {
            error!(
                "CRITICAL: Recovered from poisoned read lock. This indicates a previous panic. \
                 State may be inconsistent. Consider using read_or_fail() for safer behavior."
            );
            #[cfg(any())]
            {
                error!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
            }
            poisoned.into_inner()
        })
    }

    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|poisoned| {
            error!(
                "CRITICAL: Recovered from poisoned write lock. This indicates a previous panic. \
                 State may be inconsistent. Consider using write_or_fail() for safer behavior."
            );
            #[cfg(any())]
            {
                error!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
            }
            poisoned.into_inner()
        })
    }

    fn read_or_fail(&self) -> std::result::Result<RwLockReadGuard<'_, T>, crate::ZoeyError> {
        self.read().map_err(|_poisoned| {
            error!(
                "CRITICAL: Attempted to acquire read lock but it is poisoned. \
                 A previous thread panicked while holding this lock. State may be corrupted."
            );
            #[cfg(any())]
            {
                error!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
            }
            crate::ZoeyError::runtime(
                "Lock is poisoned - a previous thread panicked while holding this lock. \
                 This indicates a serious error. The runtime state may be corrupted.",
            )
        })
    }

    fn write_or_fail(&self) -> std::result::Result<RwLockWriteGuard<'_, T>, crate::ZoeyError> {
        self.write().map_err(|_poisoned| {
            error!(
                "CRITICAL: Attempted to acquire write lock but it is poisoned. \
                 A previous thread panicked while holding this lock. State may be corrupted."
            );
            #[cfg(any())]
            {
                error!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
            }
            crate::ZoeyError::runtime(
                "Lock is poisoned - a previous thread panicked while holding this lock. \
                 This indicates a serious error. The runtime state may be corrupted.",
            )
        })
    }
}

/// Agent runtime - main implementation of IAgentRuntime
///
/// See module-level documentation for thread safety guarantees and lock ordering.
pub struct AgentRuntime {
    /// Agent ID
    pub agent_id: Uuid,

    /// Character configuration
    pub character: Character,

    /// Database adapter
    pub(crate) adapter: Arc<RwLock<Option<Arc<dyn IDatabaseAdapter + Send + Sync>>>>,

    /// Registered actions
    pub(crate) actions: Arc<RwLock<Vec<Arc<dyn Action>>>>,

    /// Registered evaluators
    pub(crate) evaluators: Arc<RwLock<Vec<Arc<dyn Evaluator>>>>,

    /// Registered providers
    pub(crate) providers: Arc<RwLock<Vec<Arc<dyn Provider>>>>,

    /// Registered services
    pub(crate) services: Arc<RwLock<HashMap<ServiceTypeName, Vec<Arc<dyn Service>>>>>,
    pub(crate) typed_services: Arc<RwLock<HashMap<String, Arc<dyn Service>>>>,

    /// Model handlers
    pub(crate) models: Arc<RwLock<HashMap<String, Vec<ModelProvider>>>>,

    /// Registered plugins
    plugins: Arc<RwLock<Vec<Arc<dyn Plugin>>>>,

    /// Event handlers
    pub(crate) events: Arc<RwLock<HashMap<String, Vec<EventHandler>>>>,

    /// State cache
    pub(crate) state_cache: Arc<RwLock<HashMap<String, State>>>,

    /// Logger for structured logging
    logger: Arc<RwLock<tracing::Span>>,

    /// Settings
    pub(crate) settings: Arc<RwLock<HashMap<String, serde_json::Value>>>,

    /// Routes
    routes: Arc<RwLock<Vec<Route>>>,

    /// Task workers for background task management
    task_workers: Arc<RwLock<HashMap<String, Arc<dyn TaskWorker>>>>,

    /// Send handlers for custom message sending
    send_handlers: Arc<RwLock<HashMap<String, SendHandlerFunction>>>,

    /// Message service for message queue integration
    message_service: Arc<RwLock<Option<Arc<dyn IMessageService>>>>,

    /// Conversation length for context window management
    pub(crate) conversation_length: usize,

    /// Current run ID
    pub(crate) current_run_id: Arc<RwLock<Option<Uuid>>>,

    /// Action results cache
    pub(crate) action_results: Arc<RwLock<HashMap<Uuid, Vec<ActionResult>>>>,

    /// ZoeyOS instance reference for framework integration
    zoey_os: Arc<RwLock<Option<Arc<dyn std::any::Any + Send + Sync>>>>,

    /// Dynamic prompt executor for schema-based prompting
    dynamic_prompt_executor: Arc<DynamicPromptExecutor>,

    /// Observability for cost tracking and monitoring
    pub observability: Arc<RwLock<Option<Arc<crate::observability::Observability>>>>,

    /// Lock recovery strategy configuration
    lock_recovery_strategy: LockRecoveryStrategy,

    /// Metrics for tracking poisoned lock events
    lock_poison_metrics: Arc<LockPoisonMetrics>,

    /// Training data collector for RLHF and model fine-tuning
    training_collector: Option<Arc<crate::training::TrainingCollector>>,
}

/// Runtime options for constructing an AgentRuntime.
///
/// # Required vs Optional Fields
///
/// While all fields have defaults, you typically want to provide:
/// - `character` - Defines the agent's identity and behavior (defaults to empty character)
/// - `adapter` - Database for persistence (defaults to None, meaning no persistence)
///
/// # Example
///
/// ```rust,ignore
/// use zoey_core::*;
/// use std::sync::Arc;
/// use zoey_plugin_bootstrap::BootstrapPlugin;
/// use zoey_plugin_sql::SqliteAdapter;
///
/// // Minimal setup (uses defaults)
/// let minimal = RuntimeOpts::default();
///
/// // Typical setup with character and database
/// let typical = RuntimeOpts {
///     character: Some(Character {
///         name: "MyAgent".to_string(),
///         ..Default::default()
///     }),
///     adapter: Some(Arc::new(SqliteAdapter::new("sqlite::memory:").await.unwrap())),
///     plugins: vec![Arc::new(BootstrapPlugin::new())],
///     ..Default::default()
/// };
///
/// // Using the builder pattern
/// let built = RuntimeOpts::new()
///     .with_character(Character::default())
///     .with_plugins(vec![Arc::new(BootstrapPlugin::new())]);
/// ```
#[derive(Default)]
pub struct RuntimeOpts {
    /// Agent ID - if not provided, generated deterministically from character name.
    /// Providing an explicit ID ensures stability across restarts.
    pub agent_id: Option<Uuid>,

    /// Character configuration defining the agent's identity, personality, and behavior.
    /// If not provided, a minimal default character is used.
    pub character: Option<Character>,

    /// Initial plugins to register with the runtime.
    /// Plugins provide actions, providers, evaluators, and services.
    /// Empty by default - you typically want at least BootstrapPlugin.
    pub plugins: Vec<Arc<dyn Plugin>>,

    /// Database adapter for persistence.
    /// If None, the runtime operates without persistence (in-memory only).
    /// For production use, provide SqliteAdapter or PostgresAdapter.
    pub adapter: Option<Arc<dyn IDatabaseAdapter + Send + Sync>>,

    /// Additional configuration settings as key-value pairs.
    /// These can be accessed via runtime.get_setting().
    pub settings: Option<HashMap<String, serde_json::Value>>,

    /// Maximum conversation length for context window management.
    /// Defaults to 32 messages if not specified.
    pub conversation_length: Option<usize>,

    /// All available plugins for dependency resolution.
    /// Used when plugins have dependencies on other plugins.
    /// If not provided, only the plugins in `plugins` field are considered.
    pub all_available_plugins: Option<Vec<Arc<dyn Plugin>>>,

    /// Lock recovery strategy
    /// Defaults to `AlwaysFail` for safety
    pub lock_recovery_strategy: Option<LockRecoveryStrategy>,

    /// Test mode: minimal initialization suitable for unit tests
    /// When enabled, background workers and heavy initializations are skipped
    pub test_mode: Option<bool>,
}

impl RuntimeOpts {
    /// Create a new RuntimeOpts with all defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the character configuration.
    pub fn with_character(mut self, character: Character) -> Self {
        self.character = Some(character);
        self
    }

    /// Set the agent ID explicitly.
    pub fn with_agent_id(mut self, agent_id: Uuid) -> Self {
        self.agent_id = Some(agent_id);
        self
    }

    /// Set the database adapter.
    pub fn with_adapter(mut self, adapter: Arc<dyn IDatabaseAdapter + Send + Sync>) -> Self {
        self.adapter = Some(adapter);
        self
    }

    /// Set the plugins to register.
    pub fn with_plugins(mut self, plugins: Vec<Arc<dyn Plugin>>) -> Self {
        self.plugins = plugins;
        self
    }

    /// Add a single plugin.
    pub fn with_plugin(mut self, plugin: Arc<dyn Plugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// Set the conversation length.
    pub fn with_conversation_length(mut self, length: usize) -> Self {
        self.conversation_length = Some(length);
        self
    }

    /// Set configuration settings.
    pub fn with_settings(mut self, settings: HashMap<String, serde_json::Value>) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Set lock recovery strategy
    pub fn with_lock_recovery_strategy(mut self, strategy: LockRecoveryStrategy) -> Self {
        self.lock_recovery_strategy = Some(strategy);
        self
    }
}

impl AgentRuntime {
    /// Create a new agent runtime
    pub async fn new(opts: RuntimeOpts) -> Result<Arc<RwLock<Self>>> {
        let character = opts.character.unwrap_or_default();

        // Generate deterministic UUID from character name
        let agent_id = opts.agent_id.unwrap_or_else(|| {
            character
                .id
                .unwrap_or_else(|| crate::utils::string_to_uuid(&character.name))
        });

        #[cfg(feature = "otel")]
        {
            crate::infrastructure::otel::init_otel();
        }
        let logger_span = tracing::span!(tracing::Level::INFO, "agent", name = %character.name);

        // Get max entries for dynamic prompt cache from env or use default
        let max_cache_entries = std::env::var("DYNAMIC_PROMPT_MAX_ENTRIES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000);

        let lock_recovery_strategy = opts.lock_recovery_strategy.unwrap_or_default();
        let lock_poison_metrics = Arc::new(LockPoisonMetrics::new());

        let mut initial_settings = opts.settings.unwrap_or_default();
        initial_settings
            .entry("ui:streaming".to_string())
            .or_insert(serde_json::json!(true));
        initial_settings
            .entry("ui:provider_racing".to_string())
            .or_insert(serde_json::json!(true));
        initial_settings
            .entry("ui:fast_mode".to_string())
            .or_insert(serde_json::json!(true));
        initial_settings
            .entry("ui:verbosity".to_string())
            .or_insert(serde_json::json!("short"));
        initial_settings
            .entry("ui:avoid_cutoff".to_string())
            .or_insert(serde_json::json!(false));
        let test_mode = opts.test_mode.unwrap_or_else(|| {
            std::env::var("ZOEY_TEST_MODE")
                .ok()
                .and_then(|v| v.parse::<bool>().ok())
                .unwrap_or(false)
        });
        if test_mode {
            initial_settings.insert("ui:streaming".to_string(), serde_json::json!(false));
            initial_settings.insert("ui:provider_racing".to_string(), serde_json::json!(false));
            initial_settings.insert("ui:fast_mode".to_string(), serde_json::json!(true));
        }
        // Create training collector with default config (RLHF enabled)
        let training_config = crate::training::TrainingConfig {
            enabled: true,
            min_quality_score: 0.5,
            max_samples: 10000,
            auto_save_interval: 300, // 5 minutes
            output_dir: std::path::PathBuf::from("./training_data"),
            default_format: crate::training::TrainingFormat::Jsonl,
            include_thoughts: true,
            include_negative_examples: true,
            negative_example_ratio: 0.1,
            enable_rlhf: true,
            auto_label: true,
        };
        let training_collector = Some(Arc::new(crate::training::TrainingCollector::new(training_config)));
        
        let runtime = Self {
            agent_id,
            character,
            adapter: Arc::new(RwLock::new(opts.adapter)),
            actions: Arc::new(RwLock::new(Vec::new())),
            evaluators: Arc::new(RwLock::new(Vec::new())),
            providers: Arc::new(RwLock::new(Vec::new())),
            services: Arc::new(RwLock::new(HashMap::new())),
            typed_services: Arc::new(RwLock::new(HashMap::new())),
            models: Arc::new(RwLock::new(HashMap::new())),
            plugins: Arc::new(RwLock::new(Vec::new())),
            events: Arc::new(RwLock::new(HashMap::new())),
            state_cache: Arc::new(RwLock::new(HashMap::new())),
            logger: Arc::new(RwLock::new(logger_span)),
            settings: Arc::new(RwLock::new(initial_settings)),
            routes: Arc::new(RwLock::new(Vec::new())),
            task_workers: Arc::new(RwLock::new(HashMap::new())),
            send_handlers: Arc::new(RwLock::new(HashMap::new())),
            message_service: Arc::new(RwLock::new(None)),
            conversation_length: opts.conversation_length.unwrap_or(32),
            current_run_id: Arc::new(RwLock::new(None)),
            action_results: Arc::new(RwLock::new(HashMap::new())),
            zoey_os: Arc::new(RwLock::new(None)),
            dynamic_prompt_executor: Arc::new(DynamicPromptExecutor::new(Some(max_cache_entries))),
            observability: Arc::new(RwLock::new(None)),
            lock_recovery_strategy,
            lock_poison_metrics,
            training_collector,
        };

        let runtime_arc = Arc::new(RwLock::new(runtime));

        // Register initial plugins
        if !test_mode {
            for plugin in opts.plugins {
                let mut rt = runtime_arc.write().unwrap();
                rt.register_plugin_internal(plugin).await?;
            }
        }

        debug!("runtime_new:post_plugins");

        if !test_mode {
            {
                let mut rt = runtime_arc.write().unwrap();
                // Register embedding worker
                rt.register_task_worker(
                    "embedding_generation".to_string(),
                    Arc::new(crate::workers::embedding_worker::EmbeddingWorker::new()),
                );
            }
        }

        debug!("runtime_new:return");

        Ok(runtime_arc)
    }

    /// Register a plugin (internal method without lock)
    ///
    /// Acquires locks in order: plugins -> actions -> providers -> evaluators ->
    /// services -> models -> events -> routes (following the documented lock ordering)
    async fn register_plugin_internal(&mut self, plugin: Arc<dyn Plugin>) -> Result<()> {
        info!("Registering plugin: {}", plugin.name());
        debug!("plugin_register:start name={}", plugin.name());

        // Register actions
        for action in plugin.actions() {
            self.actions.write_or_recover().push(action);
        }
        debug!("plugin_register:actions name={}", plugin.name());

        // Register providers
        for provider in plugin.providers() {
            self.providers.write_or_recover().push(provider);
        }
        debug!("plugin_register:providers name={}", plugin.name());

        // Register evaluators
        for evaluator in plugin.evaluators() {
            self.evaluators.write_or_recover().push(evaluator);
        }
        debug!("plugin_register:evaluators name={}", plugin.name());

        // Register services
        for service in plugin.services() {
            let service_type = service.service_type().to_string();
            self.services
                .write_or_recover()
                .entry(service_type.clone())
                .or_insert_with(Vec::new)
                .push(service.clone());
            // Store primary service under its type for direct retrieval
            self.typed_services
                .write_or_recover()
                .entry(service_type)
                .or_insert_with(|| service.clone());
        }
        debug!("plugin_register:services name={}", plugin.name());

        // Register model handlers
        for (model_type, handler) in plugin.models() {
            let provider_info = ModelProvider {
                name: plugin.name().to_string(),
                handler,
                priority: plugin.priority(),
            };

            self.models
                .write_or_recover()
                .entry(model_type)
                .or_insert_with(Vec::new)
                .push(provider_info);
        }

        // Sort model handlers by priority (highest first)
        for handlers in self.models.write_or_recover().values_mut() {
            handlers.sort_by(|a, b| b.priority.cmp(&a.priority));
        }
        debug!("plugin_register:models name={}", plugin.name());

        // Register event handlers
        for (event_type, handlers) in plugin.events() {
            self.events
                .write_or_recover()
                .entry(event_type)
                .or_insert_with(Vec::new)
                .extend(handlers);
        }
        debug!("plugin_register:events name={}", plugin.name());

        // Register routes
        self.routes.write_or_recover().extend(plugin.routes());
        debug!("plugin_register:routes name={}", plugin.name());

        // Add to plugins list
        self.plugins.write_or_recover().push(plugin.clone());
        debug!("plugin_register:done name={}", plugin.name());

        Ok(())
    }

    /// Initialize the runtime
    pub async fn initialize(&mut self, options: InitializeOptions) -> Result<()> {
        info!("Initializing runtime for agent: {}", self.character.name);

        // Initialize database adapter if present
        if let Some(adapter) = self.adapter.read_or_recover().clone() {
            debug!("Initializing database connection...");

            // Check if database is ready
            match adapter.is_ready().await {
                Ok(true) => {
                    info!("✓ Database connection is ready");
                }
                Ok(false) => {
                    warn!("Database is not ready, attempting initialization...");
                    // Note: We can't call initialize() here as it requires &mut
                    // This should be done before passing to runtime
                    return Err(crate::ZoeyError::database(
                        "Database is not ready. Please initialize the adapter before passing to runtime."
                    ));
                }
                Err(e) => {
                    warn!("Failed to check database readiness: {}", e);
                    return Err(crate::ZoeyError::database(format!(
                        "Database readiness check failed: {}",
                        e
                    )));
                }
            }

            // Run plugin migrations if needed
            if !options.skip_migrations {
                debug!("Checking for plugin migrations...");
                let plugins = self.plugins.read_or_recover();

                if !plugins.is_empty() {
                    info!("Running migrations for {} plugin(s)...", plugins.len());

                    // Collect plugin migrations
                    let mut plugin_migrations = Vec::new();
                    for plugin in plugins.iter() {
                        let schema = plugin.schema();
                        plugin_migrations.push(PluginMigration {
                            name: plugin.name().to_string(),
                            schema,
                        });
                    }

                    if !plugin_migrations.is_empty() {
                        match adapter
                            .run_plugin_migrations(
                                plugin_migrations,
                                MigrationOptions {
                                    verbose: false,
                                    force: false,
                                    dry_run: false,
                                },
                            )
                            .await
                        {
                            Ok(_) => info!("✓ Plugin migrations completed successfully"),
                            Err(e) => {
                                warn!("Plugin migration failed: {}", e);
                                return Err(crate::ZoeyError::database(format!(
                                    "Plugin migration failed: {}",
                                    e
                                )));
                            }
                        }
                    } else {
                        debug!("No plugin migrations required");
                    }
                } else {
                    debug!("No plugins registered, skipping migrations");
                }
            } else {
                debug!("Skipping migrations (skip_migrations=true)");
            }

            // Ensure agent is registered in the database
            match adapter.get_agent(self.agent_id).await {
                Ok(Some(_)) => debug!("Agent already exists in database"),
                Ok(None) | Err(_) => {
                    info!("Registering agent '{}' in database...", self.character.name);
                    let agent = crate::types::Agent {
                        id: self.agent_id,
                        name: self.character.name.clone(),
                        character: serde_json::to_value(&self.character).unwrap_or_default(),
                        created_at: Some(chrono::Utc::now().timestamp()),
                        updated_at: None,
                    };
                    match adapter.create_agent(&agent).await {
                        Ok(true) => info!("✓ Agent registered successfully"),
                        Ok(false) => warn!("Agent may already exist (create returned false)"),
                        Err(e) => warn!("Failed to register agent: {} - continuing anyway", e),
                    }
                }
            }
        } else {
            warn!("No database adapter configured - running without persistence");
        }

        // Initialize services
        let service_map = self.services.read_or_recover();
        if !service_map.is_empty() {
            info!("Initializing {} service type(s)...", service_map.len());
            for (service_type, services) in service_map.iter() {
                for _service in services {
                    debug!("Initializing service: {}", service_type);
                    // Service initialization would happen here
                    // Note: Service.initialize() requires &mut, should be done before registration
                }
            }
        }

        info!(
            "✓ Runtime initialization complete for agent '{}'",
            self.character.name
        );
        Ok(())
    }

    /// Compose state from providers
    pub async fn compose_state(
        &self,
        message: &Memory,
        include_list: Option<Vec<String>>,
        only_include: bool,
        skip_cache: bool,
    ) -> Result<State> {
        crate::runtime::RuntimeState::new()
            .compose_state_impl(self, message, include_list, only_include, skip_cache)
            .await
    }

    /// Add embedding to memory and persist via adapter
    pub async fn add_embedding_to_memory(&self, memory: &Memory) -> Result<Memory> {
        if let Some(adapter) = crate::runtime::RuntimeState::get_adapter(self) {
            let mut updated = memory.clone();
            let _ = adapter.update_memory(&updated).await?;
            Ok(updated)
        } else {
            Err(crate::ZoeyError::runtime("No adapter configured"))
        }
    }

    /// Queue embedding generation: resolve TEXT_EMBEDDING provider, generate, and persist
    pub async fn queue_embedding_generation(
        &self,
        memory: &Memory,
        _priority: crate::types::EmbeddingPriority,
    ) -> Result<()> {
        // Resolve provider without holding locks across await
        let provider_opt = {
            let models = self.models.read_or_recover();
            models
                .get(&crate::types::ModelType::TextEmbedding.to_string())
                .and_then(|v| v.first().cloned())
        };
        let adapter_opt = crate::runtime::RuntimeState::get_adapter(self);
        if let (Some(provider), Some(adapter)) = (provider_opt, adapter_opt) {
            let params = crate::types::GenerateTextParams {
                prompt: memory.content.text.clone(),
                max_tokens: None,
                temperature: None,
                top_p: None,
                stop: None,
                model: None,
                frequency_penalty: None,
                presence_penalty: None,
            };
            let mh_params = crate::types::ModelHandlerParams {
                runtime: Arc::new(()),
                params,
            };
            let raw = (provider.handler)(mh_params).await?;
            if let Ok(vec) = serde_json::from_str::<Vec<f32>>(&raw) {
                let mut updated = memory.clone();
                updated.embedding = Some(vec);
                let _ = adapter.update_memory(&updated).await?;
            }
            Ok(())
        } else {
            Err(crate::ZoeyError::runtime(
                "TEXT_EMBEDDING provider or adapter not available",
            ))
        }
    }

    /// Get all memories (messages table)
    pub async fn get_all_memories(&self) -> Result<Vec<Memory>> {
        if let Some(adapter) = crate::runtime::RuntimeState::get_adapter(self) {
            adapter
                .get_memories(crate::types::MemoryQuery {
                    table_name: "messages".to_string(),
                    ..Default::default()
                })
                .await
        } else {
            Err(crate::ZoeyError::runtime("No adapter configured"))
        }
    }

    /// Create a new run ID
    pub fn create_run_id(&self) -> Uuid {
        crate::runtime::lifecycle::create_run_id(self)
    }

    /// Start a run
    pub fn start_run(&mut self) -> Uuid {
        crate::runtime::lifecycle::start_run(self)
    }

    /// End the current run
    pub fn end_run(&mut self) {
        crate::runtime::lifecycle::end_run(self)
    }

    /// Get current run ID
    pub fn get_current_run_id(&self) -> Option<Uuid> {
        crate::runtime::lifecycle::get_current_run_id(self)
    }

    /// Get action results for a message
    pub fn get_action_results(&self, message_id: Uuid) -> Vec<ActionResult> {
        self.action_results
            .read_or_recover()
            .get(&message_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Set action results for a message
    pub fn set_action_results(&self, message_id: Uuid, results: Vec<ActionResult>) {
        self.action_results
            .write_or_recover()
            .insert(message_id, results);
    }
}

impl AgentRuntime {
    /// Get the training data collector for RLHF and model fine-tuning
    pub fn get_training_collector(&self) -> Option<Arc<crate::training::TrainingCollector>> {
        self.training_collector.clone()
    }

    /// Get actions (for inspection)
    pub fn get_actions(&self) -> Vec<Arc<dyn Action>> {
        crate::plugin_system::registry::get_actions(self)
    }

    /// Get providers (for inspection)
    pub fn get_providers(&self) -> Vec<Arc<dyn Provider>> {
        crate::plugin_system::registry::get_providers(self)
    }

    /// Get evaluators (for inspection)
    pub fn get_evaluators(&self) -> Vec<Arc<dyn Evaluator>> {
        crate::plugin_system::registry::get_evaluators(self)
    }

    /// Get service by type
    pub fn get_service(&self, service_type: &str) -> Option<Arc<dyn Service>> {
        crate::runtime::RuntimeState::get_service(self, service_type)
    }

    /// Get typed MultiAgentService if present (disabled)
    // removed duplicate

    /// Get services count
    pub fn get_services_count(&self) -> usize {
        crate::runtime::RuntimeState::get_services_count(self)
    }

    /// Get all services as a HashMap
    pub fn get_all_services(&self) -> HashMap<ServiceTypeName, Vec<Arc<dyn Service>>> {
        crate::runtime::RuntimeState::get_all_services(self)
    }

    /// Get a primary service by type name
    pub fn get_service_by_name(&self, name: &str) -> Option<Arc<dyn Service>> {
        self.typed_services.read_or_recover().get(name).cloned()
    }

    /// Register a provider with the runtime
    pub fn register_provider(&mut self, provider: Arc<dyn Provider>) {
        crate::plugin_system::registry::register_provider(self, provider)
    }

    /// Register an action with the runtime
    pub fn register_action(&mut self, action: Arc<dyn Action>) {
        crate::plugin_system::registry::register_action(self, action)
    }

    /// Register an evaluator with the runtime
    pub fn register_evaluator(&mut self, evaluator: Arc<dyn Evaluator>) {
        crate::plugin_system::registry::register_evaluator(self, evaluator)
    }

    /// Set a configuration setting
    pub fn set_setting(&mut self, key: &str, value: serde_json::Value, _secret: bool) {
        crate::runtime::RuntimeState::set_setting(self, key, value);
    }

    /// Get a configuration setting
    pub fn get_setting(&self, key: &str) -> Option<serde_json::Value> {
        crate::runtime::RuntimeState::get_setting(self, key)
    }

    /// Get a configuration setting as string
    pub fn get_setting_string(&self, key: &str) -> Option<String> {
        crate::runtime::RuntimeState::get_setting_string(self, key)
    }

    /// List settings with a given prefix, returning string values
    pub fn get_settings_with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        crate::runtime::RuntimeState::get_settings_with_prefix(self, prefix)
    }

    /// Get the logger span
    pub fn logger(&self) -> Arc<RwLock<tracing::Span>> {
        Arc::clone(&self.logger)
    }

    /// Get conversation length
    pub fn get_conversation_length(&self) -> usize {
        crate::runtime::RuntimeState::get_conversation_length(self)
    }

    /// Get message service
    pub fn message_service(&self) -> Option<Arc<dyn IMessageService>> {
        crate::runtime::RuntimeState::message_service(self)
    }

    /// Register a send handler
    pub fn register_send_handler(&mut self, source: String, handler: SendHandlerFunction) {
        self.send_handlers
            .write_or_recover()
            .insert(source, handler);
    }

    /// Get send handler for source
    pub fn get_send_handler(&self, source: &str) -> Option<SendHandlerFunction> {
        self.send_handlers.read_or_recover().get(source).cloned()
    }

    /// Register a task worker
    pub fn register_task_worker(&mut self, name: String, worker: Arc<dyn TaskWorker>) {
        self.task_workers.write_or_recover().insert(name, worker);
    }

    /// Get task worker
    pub fn get_task_worker(&self, name: &str) -> Option<Arc<dyn TaskWorker>> {
        self.task_workers.read_or_recover().get(name).cloned()
    }

    /// Get all task workers
    pub fn get_task_workers(&self) -> HashMap<String, Arc<dyn TaskWorker>> {
        self.task_workers.read_or_recover().clone()
    }

    /// Get ZoeyOS instance
    pub fn zoey_os(&self) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        self.zoey_os.read_or_recover().clone()
    }

    /// Set ZoeyOS instance
    pub fn set_zoey_os(&mut self, instance: Arc<dyn std::any::Any + Send + Sync>) {
        *self.zoey_os.write_or_recover() = Some(instance);
    }

    /// Get dynamic prompt executor
    pub fn dynamic_prompt_executor(&self) -> Arc<DynamicPromptExecutor> {
        Arc::clone(&self.dynamic_prompt_executor)
    }

    /// Execute dynamic prompt from state
    ///
    /// Schema-driven prompt execution with validation, retries, and metrics.
    /// Replaces ad-hoc `useModel` + manual XML parsing patterns.
    ///
    /// # Arguments
    /// * `state` - Current state for template rendering
    /// * `schema` - Expected output schema
    /// * `prompt_template` - Handlebars template for prompt
    /// * `options` - Execution options (model, retries, validation)
    ///
    /// # Returns
    /// Validated response as HashMap matching schema
    ///
    /// # Example
    /// ```rust,ignore
    /// use zoey_core::*;
    /// use std::collections::HashMap;
    ///
    /// let schema = vec![
    ///     SchemaRow {
    ///         field: "thought".to_string(),
    ///         description: "Agent's reasoning".to_string(),
    ///         field_type: SchemaType::String,
    ///         required: true,
    ///         example: None,
    ///         validation: None,
    ///     },
    ///     SchemaRow {
    ///         field: "action".to_string(),
    ///         description: "Action to take".to_string(),
    ///         field_type: SchemaType::String,
    ///         required: true,
    ///         example: Some("RESPOND".to_string()),
    ///         validation: None,
    ///     },
    /// ];
    ///
    /// let runtime = AgentRuntime::default();
    /// let state = State::new();
    /// let _ = runtime.dynamic_prompt_exec_from_state(
    ///     &state,
    ///     schema,
    ///     "Analyze: {{userMessage}}",
    ///     DynamicPromptOptions::default(),
    /// );
    /// ```
    pub async fn dynamic_prompt_exec_from_state(
        &self,
        state: &State,
        schema: Vec<SchemaRow>,
        prompt_template: &str,
        options: DynamicPromptOptions,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let executor = &self.dynamic_prompt_executor;

        // Create model function that uses runtime's model providers
        let models = self.models.read_or_recover();
        let model_identifier =
            options
                .model
                .clone()
                .unwrap_or_else(|| match options.model_size.as_deref() {
                    Some("small") => "TEXT_SMALL".to_string(),
                    _ => "TEXT_LARGE".to_string(),
                });

        // Get model handler
        let _handlers = models.get(&model_identifier).cloned();

        let model_fn = |_prompt: String, _opts: DynamicPromptOptions| async move {
            // This would call the actual model provider
            // For now, return a placeholder
            Ok(format!(
                "<response><error>Model not connected</error></response>"
            ))
        };

        executor
            .execute_from_state(state, schema, prompt_template, options, model_fn)
            .await
    }

    /// Get dynamic prompt metrics summary
    pub fn get_dynamic_prompt_metrics(&self) -> crate::dynamic_prompts::MetricsSummary {
        self.dynamic_prompt_executor.get_metrics_summary()
    }

    /// Clear dynamic prompt metrics
    pub fn clear_dynamic_prompt_metrics(&self) {
        self.dynamic_prompt_executor.clear_metrics();
    }

    /// Get database adapter
    pub fn get_adapter(&self) -> Option<Arc<dyn IDatabaseAdapter + Send + Sync>> {
        self.adapter.read_or_recover().clone()
    }

    /// Get model providers
    pub fn get_models(&self) -> HashMap<String, Vec<ModelProvider>> {
        crate::plugin_system::registry::get_models(self)
    }

    /// Get lock with context-aware recovery
    ///
    /// This is the recommended method for acquiring locks in the runtime.
    /// It uses the configured recovery strategy and tracks metrics.
    fn get_lock_context(&self, lock_name: &str) -> LockContext {
        LockContext::new(
            lock_name.to_string(),
            self.lock_recovery_strategy,
            Arc::clone(&self.lock_poison_metrics),
        )
    }

    /// Get lock recovery strategy
    pub fn get_lock_recovery_strategy(&self) -> LockRecoveryStrategy {
        self.lock_recovery_strategy
    }

    /// Set lock recovery strategy
    pub fn set_lock_recovery_strategy(&mut self, strategy: LockRecoveryStrategy) {
        self.lock_recovery_strategy = strategy;
        info!("Lock recovery strategy changed to: {:?}", strategy);
    }

    /// Get lock poison metrics summary
    pub fn get_lock_poison_metrics(&self) -> LockPoisonSummary {
        self.lock_poison_metrics.get_summary()
    }

    /// Reset lock poison metrics
    pub fn reset_lock_poison_metrics(&self) {
        self.lock_poison_metrics
            .total_poisoned
            .store(0, Ordering::Relaxed);
        self.lock_poison_metrics
            .recoveries
            .store(0, Ordering::Relaxed);
        self.lock_poison_metrics
            .failures
            .store(0, Ordering::Relaxed);
        self.lock_poison_metrics
            .read_poisoned
            .store(0, Ordering::Relaxed);
        self.lock_poison_metrics
            .write_poisoned
            .store(0, Ordering::Relaxed);
        if let Ok(mut last) = self.lock_poison_metrics.last_poisoned_at.write() {
            *last = None;
        }
        if let Ok(mut counts) = self.lock_poison_metrics.lock_poison_counts.write() {
            counts.clear();
        }
    }

    /// Check if any locks are currently poisoned
    pub fn has_poisoned_locks(&self) -> bool {
        self.lock_poison_metrics
            .total_poisoned
            .load(Ordering::Relaxed)
            > 0
    }

    /// Get health status including lock poison status
    pub fn get_lock_health_status(&self) -> LockHealthStatus {
        let summary = self.lock_poison_metrics.get_summary();
        let mut counts: Vec<(String, u64)> = summary.lock_poison_counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        let most_poisoned_locks = counts.into_iter().take(5).collect();
        let is_healthy = summary.total_poisoned == 0 && summary.failures == 0;
        LockHealthStatus {
            is_healthy,
            total_poisoned: summary.total_poisoned,
            recoveries: summary.recoveries,
            failures: summary.failures,
            most_poisoned_locks,
        }
    }
}

use super::lifecycle::LockHealthStatus;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runtime_creation() {
        let opts = RuntimeOpts {
            character: Some(Character {
                name: "TestAgent".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let runtime = AgentRuntime::new(opts).await.unwrap();
        let rt = runtime.read().unwrap();

        // Verify character was set correctly
        assert_eq!(rt.character.name, "TestAgent");

        // Verify agent ID is deterministically generated from name
        let expected_id = crate::utils::string_to_uuid("TestAgent");
        assert_eq!(rt.agent_id, expected_id);

        // Verify default conversation length
        assert_eq!(rt.get_conversation_length(), 32);
    }

    #[tokio::test]
    async fn test_runtime_with_custom_agent_id() {
        let custom_id = Uuid::new_v4();
        let opts = RuntimeOpts {
            agent_id: Some(custom_id),
            character: Some(Character {
                name: "TestAgent".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let runtime = AgentRuntime::new(opts).await.unwrap();
        let rt = runtime.read().unwrap();

        // Custom agent ID should be used
        assert_eq!(rt.agent_id, custom_id);
    }

    #[tokio::test]
    async fn test_state_composition_empty_providers() {
        let opts = RuntimeOpts {
            character: Some(Character {
                name: "TestAgent".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let runtime = AgentRuntime::new(opts).await.unwrap();
        let rt = runtime.read().unwrap();

        let message = Memory {
            id: Uuid::new_v4(),
            entity_id: Uuid::new_v4(),
            agent_id: rt.agent_id,
            room_id: Uuid::new_v4(),
            content: Content::default(),
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        // With no providers registered, state should be empty
        let state = rt
            .compose_state(&message, None, false, false)
            .await
            .unwrap();
        assert!(
            state.values.is_empty(),
            "State should be empty with no providers registered"
        );
        assert!(
            state.data.is_empty(),
            "Data should be empty with no providers registered"
        );
    }

    #[tokio::test]
    async fn test_settings_management() {
        let opts = RuntimeOpts {
            character: Some(Character {
                name: "TestAgent".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let runtime = AgentRuntime::new(opts).await.unwrap();

        // Set a setting
        {
            let mut rt = runtime.write().unwrap();
            rt.set_setting("test_key", serde_json::json!("test_value"), false);
        }

        // Retrieve the setting
        {
            let rt = runtime.read().unwrap();
            let value = rt.get_setting("test_key");
            assert!(value.is_some(), "Setting should exist");
            assert_eq!(value.unwrap(), serde_json::json!("test_value"));

            // Non-existent setting should return None
            let missing = rt.get_setting("nonexistent");
            assert!(missing.is_none(), "Non-existent setting should return None");
        }
    }

    #[tokio::test]
    async fn test_run_id_management() {
        let opts = RuntimeOpts {
            character: Some(Character {
                name: "TestAgent".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let runtime = AgentRuntime::new(opts).await.unwrap();

        // Initially no run ID
        {
            let rt = runtime.read().unwrap();
            assert!(rt.get_current_run_id().is_none(), "Initially no run ID");
        }

        // Start a run
        let run_id = {
            let mut rt = runtime.write().unwrap();
            rt.start_run()
        };

        // Verify run ID is set
        {
            let rt = runtime.read().unwrap();
            assert_eq!(
                rt.get_current_run_id(),
                Some(run_id),
                "Run ID should be set"
            );
        }

        // End the run
        {
            let mut rt = runtime.write().unwrap();
            rt.end_run();
        }

        // Verify run ID is cleared
        {
            let rt = runtime.read().unwrap();
            assert!(
                rt.get_current_run_id().is_none(),
                "Run ID should be cleared"
            );
        }
    }
}
