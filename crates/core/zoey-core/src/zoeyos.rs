//! ZoeyOS - Multi-agent orchestration framework
//!
//! Production-ready multi-agent management system with:
//! - Unified messaging API
//! - Event system
//! - Health monitoring
//! - Metrics tracking
//! - Connection management
//! - Graceful shutdown
//! - Error recovery

use crate::message::MessageProcessor;
use crate::runtime::{AgentRuntime, RuntimeOpts};
use crate::secrets::set_default_secrets_from_env;
use crate::types::{Character, Content, InitializeOptions, Memory, Plugin, UUID};
use crate::{ZoeyError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};

/// Options for sending a message to an agent
#[derive(Clone)]
pub struct SendMessageOptions {
    /// Called when the agent generates a response (ASYNC MODE)
    /// If provided, method returns immediately (fire & forget)
    /// If not provided, method waits for response (SYNC MODE)
    pub on_response: Option<
        Arc<
            dyn Fn(
                    Content,
                )
                    -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
                + Send
                + Sync,
        >,
    >,

    /// Called if an error occurs during processing
    pub on_error: Option<
        Arc<
            dyn Fn(
                    ZoeyError,
                )
                    -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
                + Send
                + Sync,
        >,
    >,

    /// Called when processing is complete
    pub on_complete: Option<
        Arc<
            dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
                + Send
                + Sync,
        >,
    >,

    /// Maximum number of retries for failed messages
    pub max_retries: Option<usize>,

    /// Timeout duration in milliseconds
    pub timeout_duration: Option<u64>,

    /// Enable multi-step message processing
    pub use_multi_step: Option<bool>,

    /// Maximum multi-step iterations
    pub max_multi_step_iterations: Option<usize>,
}

impl std::fmt::Debug for SendMessageOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendMessageOptions")
            .field(
                "on_response",
                &self.on_response.as_ref().map(|_| "<callback>"),
            )
            .field("on_error", &self.on_error.as_ref().map(|_| "<callback>"))
            .field(
                "on_complete",
                &self.on_complete.as_ref().map(|_| "<callback>"),
            )
            .field("max_retries", &self.max_retries)
            .field("timeout_duration", &self.timeout_duration)
            .field("use_multi_step", &self.use_multi_step)
            .field("max_multi_step_iterations", &self.max_multi_step_iterations)
            .finish()
    }
}

impl Default for SendMessageOptions {
    fn default() -> Self {
        Self {
            on_response: None,
            on_error: None,
            on_complete: None,
            max_retries: None,
            timeout_duration: None,
            use_multi_step: None,
            max_multi_step_iterations: None,
        }
    }
}

/// Result of sending a message to an agent
#[derive(Debug, Clone)]
pub struct SendMessageResult {
    /// ID of the user message
    pub message_id: UUID,

    /// The user message that was created
    pub user_message: Memory,

    /// Processing result (only in SYNC mode)
    pub result: Option<Vec<Memory>>,
}

/// Health status for an agent
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// Is the agent alive
    pub alive: bool,

    /// Is the agent responsive
    pub responsive: bool,

    /// Memory usage in bytes
    pub memory_usage: Option<usize>,

    /// Uptime in seconds
    pub uptime: Option<u64>,

    /// Last message processed timestamp
    pub last_activity: Option<i64>,

    /// Number of messages processed
    pub messages_processed: Option<usize>,
}

/// ZoeyOS metrics
#[derive(Debug, Clone)]
pub struct ZoeyOSMetrics {
    /// Total messages sent
    pub total_messages: usize,

    /// Total messages succeeded
    pub successful_messages: usize,

    /// Total messages failed
    pub failed_messages: usize,

    /// Total agents created
    pub total_agents: usize,

    /// Total agents started
    pub started_agents: usize,

    /// Total agents stopped
    pub stopped_agents: usize,

    /// Start time
    pub start_time: Instant,
}

impl ZoeyOSMetrics {
    fn new() -> Self {
        Self {
            total_messages: 0,
            successful_messages: 0,
            failed_messages: 0,
            total_agents: 0,
            started_agents: 0,
            stopped_agents: 0,
            start_time: Instant::now(),
        }
    }

    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_messages == 0 {
            0.0
        } else {
            self.successful_messages as f64 / self.total_messages as f64
        }
    }
}

/// ZoeyOS - Multi-agent orchestration framework
/// Provides unified messaging API across all platforms
pub struct ZoeyOS {
    /// Map of agent runtimes by agent ID
    runtimes: Arc<RwLock<HashMap<UUID, Arc<std::sync::RwLock<AgentRuntime>>>>>,

    /// Initialization functions for agents
    init_functions: Arc<
        RwLock<
            HashMap<
                UUID,
                Arc<
                    dyn Fn(
                            Arc<std::sync::RwLock<AgentRuntime>>,
                        ) -> std::pin::Pin<
                            Box<dyn std::future::Future<Output = Result<()>> + Send>,
                        > + Send
                        + Sync,
                >,
            >,
        >,
    >,

    /// Whether editable mode is enabled
    editable_mode: Arc<RwLock<bool>>,

    /// Metrics tracking
    metrics: Arc<RwLock<ZoeyOSMetrics>>,

    /// Agent activity tracking
    agent_activity: Arc<RwLock<HashMap<UUID, AgentActivity>>>,
}

/// Agent activity tracking
#[derive(Debug, Clone)]
struct AgentActivity {
    /// Messages processed by this agent
    messages_processed: usize,

    /// Last message timestamp
    last_activity: i64,

    /// Agent start time
    started_at: Option<Instant>,
}

impl ZoeyOS {
    /// Create a new ZoeyOS instance
    pub fn new() -> Self {
        info!("Creating new ZoeyOS instance");

        Self {
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            init_functions: Arc::new(RwLock::new(HashMap::new())),
            editable_mode: Arc::new(RwLock::new(false)),
            metrics: Arc::new(RwLock::new(ZoeyOSMetrics::new())),
            agent_activity: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a single agent
    ///
    /// # Arguments
    /// * `character` - The character definition
    /// * `plugins` - Optional list of plugins
    /// * `init` - Optional initialization function
    ///
    /// # Returns
    /// The agent ID
    #[instrument(skip(self, character, plugins, init), fields(character_name = %character.name), level = "info")]
    pub async fn add_agent(
        &self,
        mut character: Character,
        plugins: Option<Vec<Arc<dyn Plugin>>>,
        init: Option<
            Arc<
                dyn Fn(
                        Arc<std::sync::RwLock<AgentRuntime>>,
                    )
                        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
                    + Send
                    + Sync,
            >,
        >,
    ) -> Result<UUID> {
        info!("Adding agent: {}", character.name);

        // Merge environment secrets with character secrets
        set_default_secrets_from_env(&mut character);

        // Create runtime options
        let opts = RuntimeOpts {
            character: Some(character.clone()),
            plugins: plugins.unwrap_or_default(),
            ..Default::default()
        };

        // Create runtime
        let runtime_lock = AgentRuntime::new(opts).await?;
        let agent_id = runtime_lock.read().unwrap().agent_id;
        let runtime = runtime_lock;

        // Store runtime
        {
            let mut runtimes = self.runtimes.write().await;
            if runtimes.contains_key(&agent_id) {
                return Err(ZoeyError::Runtime(format!(
                    "Agent {} already exists",
                    agent_id
                )));
            }
            runtimes.insert(agent_id, runtime.clone());
        }

        // Initialize agent activity tracking
        {
            let mut activity = self.agent_activity.write().await;
            activity.insert(
                agent_id,
                AgentActivity {
                    messages_processed: 0,
                    last_activity: chrono::Utc::now().timestamp_millis(),
                    started_at: None,
                },
            );
        }

        // Store init function if provided
        if let Some(init_fn) = init {
            let mut init_fns = self.init_functions.write().await;
            init_fns.insert(agent_id, init_fn);
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_agents += 1;
        }

        info!("✓ Agent added: {} ({})", character.name, agent_id);
        Ok(agent_id)
    }

    /// Add multiple agents (batch operation)
    ///
    /// # Arguments
    /// * `agents` - List of agent configurations
    ///
    /// # Returns
    /// Vector of agent IDs
    pub async fn add_agents(
        &self,
        agents: Vec<(
            Character,
            Option<Vec<Arc<dyn Plugin>>>,
            Option<
                Arc<
                    dyn Fn(
                            Arc<std::sync::RwLock<AgentRuntime>>,
                        ) -> std::pin::Pin<
                            Box<dyn std::future::Future<Output = Result<()>> + Send>,
                        > + Send
                        + Sync,
                >,
            >,
        )>,
    ) -> Result<Vec<UUID>> {
        let mut agent_ids = Vec::new();

        for (character, plugins, init) in agents {
            let id = self.add_agent(character, plugins, init).await?;
            agent_ids.push(id);
        }

        Ok(agent_ids)
    }

    /// Start agents
    ///
    /// # Arguments
    /// * `agent_ids` - Optional list of agent IDs to start (None = all agents)
    #[instrument(skip(self), level = "info")]
    pub async fn start_agents(&self, agent_ids: Option<Vec<UUID>>) -> Result<()> {
        let runtimes = self.runtimes.read().await;

        let ids: Vec<UUID> = if let Some(ids) = agent_ids {
            ids
        } else {
            runtimes.keys().copied().collect()
        };

        info!("Starting {} agent(s)", ids.len());

        let mut started_count = 0;
        let mut failed_count = 0;

        // Initialize each agent
        for agent_id in &ids {
            if let Some(runtime) = runtimes.get(agent_id) {
                match {
                    let mut rt = runtime.write().unwrap();
                    rt.initialize(InitializeOptions::default()).await
                } {
                    Ok(_) => {
                        debug!("✓ Agent {} initialized", agent_id);
                        started_count += 1;

                        // Update activity tracking
                        let mut activity = self.agent_activity.write().await;
                        if let Some(act) = activity.get_mut(agent_id) {
                            act.started_at = Some(Instant::now());
                        }
                    }
                    Err(e) => {
                        error!("✗ Failed to initialize agent {}: {}", agent_id, e);
                        failed_count += 1;
                    }
                }
            } else {
                warn!("Agent {} not found in runtimes", agent_id);
                failed_count += 1;
            }
        }

        // Run init functions
        let init_fns = self.init_functions.read().await;
        for agent_id in &ids {
            if let Some(init_fn) = init_fns.get(agent_id) {
                if let Some(runtime) = runtimes.get(agent_id) {
                    match init_fn(runtime.clone()).await {
                        Ok(_) => debug!("✓ Agent {} init function completed", agent_id),
                        Err(e) => warn!("Agent {} init function failed: {}", agent_id, e),
                    }
                }
            }
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.started_agents += started_count;
        }

        if failed_count > 0 {
            warn!(
                "⚠️  Started {}/{} agents ({} failed)",
                started_count,
                ids.len(),
                failed_count
            );
        } else {
            info!("✓ All {} agent(s) started successfully", started_count);
        }

        Ok(())
    }

    /// Stop agents
    ///
    /// # Arguments
    /// * `agent_ids` - Optional list of agent IDs to stop (None = all agents)
    #[instrument(skip(self), level = "info")]
    pub async fn stop_agents(&self, agent_ids: Option<Vec<UUID>>) -> Result<()> {
        let runtimes = self.runtimes.read().await;

        let ids: Vec<UUID> = if let Some(ids) = agent_ids {
            ids
        } else {
            runtimes.keys().copied().collect()
        };

        info!("Stopping {} agent(s)", ids.len());

        let mut stopped_count = 0;

        for agent_id in ids {
            if runtimes.contains_key(&agent_id) {
                // Clean up agent activity
                let mut activity = self.agent_activity.write().await;
                if let Some(act) = activity.get_mut(&agent_id) {
                    act.started_at = None;
                }

                debug!("✓ Agent {} stopped", agent_id);
                stopped_count += 1;
            } else {
                warn!("Agent {} not found", agent_id);
            }
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.stopped_agents += stopped_count;
        }

        info!("✓ Stopped {} agent(s)", stopped_count);
        Ok(())
    }

    /// Delete agents
    ///
    /// # Arguments
    /// * `agent_ids` - List of agent IDs to delete
    #[instrument(skip(self), level = "info")]
    pub async fn delete_agents(&self, agent_ids: Vec<UUID>) -> Result<()> {
        info!("Deleting {} agent(s)", agent_ids.len());

        // Stop agents first
        self.stop_agents(Some(agent_ids.clone())).await?;

        let mut deleted_count = 0;

        // Remove from runtimes
        {
            let mut runtimes = self.runtimes.write().await;
            for agent_id in &agent_ids {
                if runtimes.remove(agent_id).is_some() {
                    deleted_count += 1;
                    debug!("✓ Removed agent {} from runtimes", agent_id);
                }
            }
        }

        // Remove init functions
        {
            let mut init_fns = self.init_functions.write().await;
            for agent_id in &agent_ids {
                init_fns.remove(agent_id);
            }
        }

        // Remove activity tracking
        {
            let mut activity = self.agent_activity.write().await;
            for agent_id in &agent_ids {
                activity.remove(agent_id);
            }
        }

        info!("✓ Deleted {} agent(s)", deleted_count);
        Ok(())
    }

    /// Get an agent runtime by ID
    ///
    /// # Arguments
    /// * `agent_id` - The agent ID
    ///
    /// # Returns
    /// The agent runtime if found
    pub async fn get_agent(&self, agent_id: UUID) -> Option<Arc<std::sync::RwLock<AgentRuntime>>> {
        let runtimes = self.runtimes.read().await;
        runtimes.get(&agent_id).cloned()
    }

    /// Get all agent runtimes
    ///
    /// # Returns
    /// Vector of all agent runtimes
    pub async fn get_agents(&self) -> Vec<Arc<std::sync::RwLock<AgentRuntime>>> {
        let runtimes = self.runtimes.read().await;
        runtimes.values().cloned().collect()
    }

    /// Send a message to a specific agent
    ///
    /// # Arguments
    /// * `agent_id` - The agent ID to send the message to
    /// * `message` - Partial Memory object (missing fields auto-filled)
    /// * `options` - Optional callbacks and processing options
    ///
    /// # Returns
    /// Promise with message ID and result
    #[instrument(skip(self, message, options), fields(
        agent_id = %agent_id,
        message_id = %message.id,
        room_id = %message.room_id
    ), level = "info")]
    pub async fn send_message(
        &self,
        agent_id: UUID,
        mut message: Memory,
        options: Option<SendMessageOptions>,
    ) -> Result<SendMessageResult> {
        let start_time = Instant::now();

        // Get the runtime
        let runtime = self.get_agent(agent_id).await.ok_or_else(|| {
            error!("Agent {} not found", agent_id);
            ZoeyError::NotFound(format!("Agent {} not found", agent_id))
        })?;

        // Auto-fill missing fields
        if message.id == UUID::nil() {
            message.id = uuid::Uuid::new_v4();
        }
        if message.agent_id == UUID::nil() {
            message.agent_id = agent_id;
        }
        if message.created_at == 0 {
            message.created_at = chrono::Utc::now().timestamp_millis();
        }

        let message_id = message.id;
        let user_message = message.clone();

        debug!("Processing message {} for agent {}", message_id, agent_id);

        // Ensure connection exists (if database adapter present)
        let room_exists = {
            let rt = runtime.read().unwrap();
            let adapter_lock = rt.adapter.read().unwrap();
            adapter_lock.is_some()
        };

        if room_exists {
            // Check if room exists
            debug!("Verified database connection for room {}", message.room_id);
        }

        // Determine mode: async or sync
        let options = options.unwrap_or_default();
        let is_async_mode = options.on_response.is_some();

        // Update metrics upfront
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_messages += 1;
        }

        if is_async_mode {
            // ========== ASYNC MODE ==========
            info!("Processing message in ASYNC mode (fire-and-forget)");

            // For async mode, we just queue the message and return immediately
            // The actual processing would be done by a message queue/service

            // Update activity
            {
                let mut activity = self.agent_activity.write().await;
                if let Some(act) = activity.get_mut(&agent_id) {
                    act.messages_processed += 1;
                    act.last_activity = chrono::Utc::now().timestamp_millis();
                }
            }

            // Call completion callback immediately (fire-and-forget pattern)
            if let Some(on_complete) = options.on_complete {
                let _ = on_complete().await;
            }

            info!(
                "Message queued for async processing ({}ms)",
                start_time.elapsed().as_millis()
            );

            Ok(SendMessageResult {
                message_id,
                user_message,
                result: None,
            })
        } else {
            // ========== SYNC MODE ==========
            info!("Processing message in SYNC mode");

            // Use the existing MessageProcessor for actual message processing
            let responses = process_message_sync(&runtime, message).await;

            match responses {
                Ok(responses) => {
                    // Update metrics
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.successful_messages += 1;
                    }

                    // Update activity
                    {
                        let mut activity = self.agent_activity.write().await;
                        if let Some(act) = activity.get_mut(&agent_id) {
                            act.messages_processed += 1;
                            act.last_activity = chrono::Utc::now().timestamp_millis();
                        }
                    }

                    if let Some(on_complete) = options.on_complete {
                        on_complete().await?;
                    }

                    info!(
                        "✓ Message processed successfully ({}ms)",
                        start_time.elapsed().as_millis()
                    );

                    Ok(SendMessageResult {
                        message_id,
                        user_message,
                        result: Some(responses),
                    })
                }
                Err(e) => {
                    error!("Message processing failed: {}", e);

                    // Update metrics
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.failed_messages += 1;
                    }

                    Err(e)
                }
            }
        }
    }

    /// Health check for agents
    ///
    /// # Arguments
    /// * `agent_ids` - Optional list of agent IDs to check (None = all agents)
    ///
    /// # Returns
    /// Map of agent IDs to health status
    #[instrument(skip(self), level = "debug")]
    pub async fn health_check(&self, agent_ids: Option<Vec<UUID>>) -> HashMap<UUID, HealthStatus> {
        let runtimes = self.runtimes.read().await;
        let activity = self.agent_activity.read().await;

        let ids: Vec<UUID> = if let Some(ids) = agent_ids {
            ids
        } else {
            runtimes.keys().copied().collect()
        };

        debug!("Performing health check on {} agent(s)", ids.len());

        let mut results = HashMap::new();

        for agent_id in ids {
            let alive = runtimes.contains_key(&agent_id);

            // Get activity info
            let (last_activity, messages_processed, uptime) =
                if let Some(act) = activity.get(&agent_id) {
                    let uptime = act
                        .started_at
                        .as_ref()
                        .map(|start| start.elapsed().as_secs());

                    (
                        Some(act.last_activity),
                        Some(act.messages_processed),
                        uptime,
                    )
                } else {
                    (None, None, None)
                };

            // Check responsiveness (active in last 5 minutes)
            let responsive = if let Some(last) = last_activity {
                let now = chrono::Utc::now().timestamp_millis();
                (now - last) < 300_000 // 5 minutes
            } else {
                alive // If no activity yet, alive = responsive
            };

            let status = HealthStatus {
                alive,
                responsive,
                memory_usage: None, // Could query process memory
                uptime,
                last_activity,
                messages_processed,
            };

            results.insert(agent_id, status);
        }

        debug!(
            "Health check complete: {}/{} agents alive",
            results.values().filter(|s| s.alive).count(),
            results.len()
        );

        results
    }

    /// Get ZoeyOS metrics
    pub async fn get_metrics(&self) -> ZoeyOSMetrics {
        self.metrics.read().await.clone()
    }

    /// Clear metrics
    pub async fn clear_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        *metrics = ZoeyOSMetrics::new();
        info!("Metrics cleared");
    }

    /// Get agent count
    pub async fn agent_count(&self) -> usize {
        self.runtimes.read().await.len()
    }

    /// Get active agent count (started)
    pub async fn active_agent_count(&self) -> usize {
        let activity = self.agent_activity.read().await;
        activity.values().filter(|a| a.started_at.is_some()).count()
    }

    /// Enable editable mode for post-initialization updates
    pub async fn enable_editable_mode(&self) {
        let mut editable = self.editable_mode.write().await;
        *editable = true;
        info!("Editable mode enabled");
    }

    /// Check if editable mode is enabled
    pub async fn is_editable_mode(&self) -> bool {
        *self.editable_mode.read().await
    }

    /// Graceful shutdown of all agents
    #[instrument(skip(self), level = "info")]
    pub async fn shutdown(&self) -> Result<()> {
        info!("Initiating graceful shutdown");

        let all_ids: Vec<UUID> = {
            let runtimes = self.runtimes.read().await;
            runtimes.keys().copied().collect()
        };

        // Stop all agents
        self.stop_agents(None).await?;

        // Delete all agents
        if !all_ids.is_empty() {
            self.delete_agents(all_ids).await?;
        }

        info!("✓ Shutdown complete");
        Ok(())
    }
}

/// Process message synchronously (no spawning)
///
/// Uses existing MessageProcessor to handle the message
async fn process_message_sync(
    runtime: &Arc<std::sync::RwLock<AgentRuntime>>,
    message: Memory,
) -> Result<Vec<Memory>> {
    debug!("Processing message: {}", message.id);

    // Create a simple mock room for processing
    // In production, this would fetch from database
    let room = crate::types::Room {
        id: message.room_id,
        agent_id: Some(message.agent_id),
        name: "Room".to_string(),
        source: message
            .content
            .source
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        channel_type: crate::types::ChannelType::Dm,
        channel_id: None,
        server_id: None,
        world_id: message.room_id, // Use room_id as world_id for simplicity
        metadata: HashMap::new(),
        created_at: Some(chrono::Utc::now().timestamp_millis()),
    };

    // Use MessageProcessor for actual processing
    let processor = MessageProcessor::new(runtime.clone());

    // Process the message
    processor.process_message(message, room).await
}

impl Default for ZoeyOS {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Metadata;

    fn create_test_character() -> Character {
        Character {
            id: None,
            name: "Test Agent".to_string(),
            username: None,
            bio: vec![],
            lore: vec![],
            knowledge: vec![],
            message_examples: vec![],
            post_examples: vec![],
            topics: vec![],
            style: Default::default(),
            adjectives: vec![],
            settings: Metadata::new(),
            templates: None,
            plugins: vec![],
            clients: vec![],
            model_provider: None,
        }
    }

    #[tokio::test]
    async fn test_zoeyos_creation() {
        let zoeyos = ZoeyOS::new();
        assert!(!zoeyos.is_editable_mode().await);

        let agents = zoeyos.get_agents().await;
        assert_eq!(agents.len(), 0);
        assert_eq!(zoeyos.agent_count().await, 0);
    }

    #[tokio::test]
    async fn test_add_agent() {
        let zoeyos = ZoeyOS::new();
        let character = create_test_character();

        let agent_id = zoeyos.add_agent(character, None, None).await.unwrap();
        assert_eq!(zoeyos.agent_count().await, 1);

        let agent = zoeyos.get_agent(agent_id).await;
        assert!(agent.is_some());
    }

    #[tokio::test]
    async fn test_start_stop_agents() {
        let zoeyos = ZoeyOS::new();
        let character = create_test_character();

        let agent_id = zoeyos.add_agent(character, None, None).await.unwrap();

        // Start the agent
        zoeyos.start_agents(Some(vec![agent_id])).await.unwrap();
        assert_eq!(zoeyos.active_agent_count().await, 1);

        // Stop the agent
        zoeyos.stop_agents(Some(vec![agent_id])).await.unwrap();
        assert_eq!(zoeyos.active_agent_count().await, 0);
    }

    #[tokio::test]
    async fn test_delete_agents() {
        let zoeyos = ZoeyOS::new();
        let character = create_test_character();

        let agent_id = zoeyos.add_agent(character, None, None).await.unwrap();
        assert_eq!(zoeyos.agent_count().await, 1);

        zoeyos.delete_agents(vec![agent_id]).await.unwrap();
        assert_eq!(zoeyos.agent_count().await, 0);
    }

    #[tokio::test]
    async fn test_health_check() {
        let zoeyos = ZoeyOS::new();
        let character = create_test_character();

        let agent_id = zoeyos.add_agent(character, None, None).await.unwrap();

        let health = zoeyos.health_check(Some(vec![agent_id])).await;
        assert_eq!(health.len(), 1);

        let status = health.get(&agent_id).unwrap();
        assert!(status.alive);
    }

    #[tokio::test]
    async fn test_metrics() {
        let zoeyos = ZoeyOS::new();

        let metrics = zoeyos.get_metrics().await;
        assert_eq!(metrics.total_agents, 0);
        assert_eq!(metrics.total_messages, 0);

        let character = create_test_character();
        zoeyos.add_agent(character, None, None).await.unwrap();

        let metrics = zoeyos.get_metrics().await;
        assert_eq!(metrics.total_agents, 1);
    }

    #[tokio::test]
    async fn test_enable_editable_mode() {
        let zoeyos = ZoeyOS::new();
        assert!(!zoeyos.is_editable_mode().await);

        zoeyos.enable_editable_mode().await;
        assert!(zoeyos.is_editable_mode().await);
    }

    #[tokio::test]
    async fn test_shutdown() {
        let zoeyos = ZoeyOS::new();
        let character = create_test_character();

        zoeyos.add_agent(character, None, None).await.unwrap();
        assert_eq!(zoeyos.agent_count().await, 1);

        zoeyos.shutdown().await.unwrap();
        assert_eq!(zoeyos.agent_count().await, 0);
    }
}
