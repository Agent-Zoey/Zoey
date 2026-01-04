/*!
# Cross-Plugin Integration Module

This module provides optional integration capabilities that connect LauraAI's
various plugins together. It's designed to gracefully degrade when specific
plugins aren't available.

## Design Principles

1. **Optional Dependencies**: Integration features only activate when their
   required plugins are registered with the runtime.

2. **Graceful Degradation**: If a plugin isn't available, the integration
   returns a sensible default or skips the feature entirely.

3. **Capability-Based**: Plugins advertise their capabilities, and the
   integration layer uses those to determine what bridges to activate.

4. **Non-Breaking**: Core functionality works without any integration.

## Usage

Integration happens automatically through the CapabilityRegistry:

```rust
use zoey_core::integration::{CapabilityRegistry, Capability};

// Plugins register their capabilities
let mut registry = CapabilityRegistry::new();
registry.register(Capability::MLInference);
registry.register(Capability::WorkflowEngine);

// Integration checks what's available
if registry.has_all(&[Capability::MLInference, Capability::WorkflowEngine]) {
    // Enable ML training workflows
}
```
*/

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// ============================================================================
// PLUGIN LIFECYCLE MANAGEMENT
// ============================================================================

/// Plugin lifecycle policy - determines how a plugin is managed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginPolicy {
    /// Always active - cannot be disabled (compliance, security)
    AlwaysOn,
    /// Loaded on demand when capabilities are needed
    OnDemand,
    /// Lazy loaded on first use, stays loaded
    LazyPersistent,
    /// Loaded for specific workflows, unloaded after
    WorkflowScoped,
    /// Can be suspended to free resources, quick resume
    Suspendable,
}

/// Current state of a plugin
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    /// Not loaded, resources freed
    Unloaded,
    /// Loading in progress
    Loading,
    /// Fully loaded and operational
    Active,
    /// Suspended (state preserved, resources freed)
    Suspended,
    /// Error state
    Error,
}

/// Plugin lifecycle metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLifecycle {
    /// Plugin name
    pub name: String,
    /// Lifecycle policy
    pub policy: PluginPolicy,
    /// Current state
    pub state: PluginState,
    /// Capabilities this plugin provides
    pub capabilities: Vec<Capability>,
    /// Last time plugin was used
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
    /// Memory usage estimate in MB
    pub memory_mb: f64,
    /// Whether plugin has been initialized
    pub initialized: bool,
    /// Error message if in error state
    pub error: Option<String>,
}

impl PluginLifecycle {
    /// Create new lifecycle for an always-on plugin
    pub fn always_on(name: &str, capabilities: Vec<Capability>) -> Self {
        Self {
            name: name.to_string(),
            policy: PluginPolicy::AlwaysOn,
            state: PluginState::Active,
            capabilities,
            last_used: Some(chrono::Utc::now()),
            memory_mb: 0.0,
            initialized: true,
            error: None,
        }
    }

    /// Create new lifecycle for an on-demand plugin
    pub fn on_demand(name: &str, capabilities: Vec<Capability>, memory_mb: f64) -> Self {
        Self {
            name: name.to_string(),
            policy: PluginPolicy::OnDemand,
            state: PluginState::Unloaded,
            capabilities,
            last_used: None,
            memory_mb,
            initialized: false,
            error: None,
        }
    }

    /// Check if plugin can be unloaded
    pub fn can_unload(&self) -> bool {
        self.policy != PluginPolicy::AlwaysOn && self.state == PluginState::Active
    }

    /// Check if plugin can be suspended
    pub fn can_suspend(&self) -> bool {
        matches!(
            self.policy,
            PluginPolicy::Suspendable | PluginPolicy::OnDemand
        ) && self.state == PluginState::Active
    }
}

/// Defines which plugins are always-on (critical for compliance/safety)
pub struct AlwaysOnPlugins;

impl AlwaysOnPlugins {
    /// Get the list of plugins that must always be active
    pub fn required() -> Vec<&'static str> {
        vec![
            "bootstrap",      // Core agent functionality
            "judgment",       // PII detection, compliance guardrails
            "explainability", // Audit trails (for compliance)
        ]
    }

    /// Get capabilities that must always be available
    pub fn required_capabilities() -> Vec<Capability> {
        vec![Capability::PIIDetection, Capability::TamperEvidentAudit]
    }

    /// Check if a plugin is always-on
    pub fn is_always_on(plugin_name: &str) -> bool {
        Self::required().contains(&plugin_name)
    }

    /// Check if a capability requires an always-on plugin
    pub fn is_required_capability(cap: Capability) -> bool {
        Self::required_capabilities().contains(&cap)
    }
}

/// Intent categories that trigger dynamic plugin loading
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntentCategory {
    /// General conversation, no special plugins needed
    Conversation,
    /// ML inference requested
    MLInference,
    /// ML training requested
    MLTraining,
    /// Complex workflow orchestration
    WorkflowOrchestration,
    /// Model deployment
    ModelDeployment,
    /// Hardware optimization
    HardwareOptimization,
    /// Adaptive learning/fine-tuning
    AdaptiveLearning,
    /// Knowledge graph operations
    KnowledgeGraph,
}

impl IntentCategory {
    /// Get the capabilities required for this intent
    pub fn required_capabilities(&self) -> Vec<Capability> {
        match self {
            Self::Conversation => vec![],
            Self::MLInference => vec![Capability::MLInference],
            Self::MLTraining => vec![
                Capability::MLTraining,
                Capability::WorkflowEngine, // Training needs workflow orchestration
            ],
            Self::WorkflowOrchestration => {
                vec![Capability::WorkflowEngine, Capability::TaskScheduling]
            }
            Self::ModelDeployment => vec![
                Capability::CascadeInference,
                Capability::TensorFlowBackend, // TF Serving
            ],
            Self::HardwareOptimization => vec![Capability::HardwareOptimization],
            Self::AdaptiveLearning => vec![Capability::AdaptiveLearning, Capability::HumanFeedback],
            Self::KnowledgeGraph => vec![Capability::SourceAttribution],
        }
    }

    /// Get the plugins typically needed for this intent
    pub fn suggested_plugins(&self) -> Vec<&'static str> {
        match self {
            Self::Conversation => vec![],
            Self::MLInference => vec!["ml", "local-llm"],
            Self::MLTraining => vec!["ml", "pytorch", "workflow"],
            Self::WorkflowOrchestration => vec!["workflow"],
            Self::ModelDeployment => vec!["tensorflow", "production"],
            Self::HardwareOptimization => vec!["hardware"],
            Self::AdaptiveLearning => vec!["adaptive"],
            Self::KnowledgeGraph => vec!["knowledge"],
        }
    }
}

/// Intent detector for dynamic plugin loading
pub struct IntentDetector;

impl IntentDetector {
    /// Analyze text to detect required intent categories
    pub fn detect(text: &str) -> Vec<IntentCategory> {
        let text_lower = text.to_lowercase();
        let mut intents = Vec::new();

        // ML Inference
        if text_lower.contains("predict")
            || text_lower.contains("classify")
            || text_lower.contains("inference")
            || text_lower.contains("run model")
            || text_lower.contains("analyze with")
        {
            intents.push(IntentCategory::MLInference);
        }

        // ML Training
        if text_lower.contains("train")
            || text_lower.contains("fine-tune")
            || text_lower.contains("fit model")
            || text_lower.contains("learning rate")
            || text_lower.contains("epoch")
        {
            intents.push(IntentCategory::MLTraining);
        }

        // Workflow
        if text_lower.contains("workflow")
            || text_lower.contains("pipeline")
            || text_lower.contains("orchestrat")
            || text_lower.contains("schedule")
            || text_lower.contains("automat")
        {
            intents.push(IntentCategory::WorkflowOrchestration);
        }

        // Deployment
        if text_lower.contains("deploy")
            || text_lower.contains("production")
            || text_lower.contains("serve model")
            || text_lower.contains("scale")
        {
            intents.push(IntentCategory::ModelDeployment);
        }

        // Hardware
        if text_lower.contains("gpu")
            || text_lower.contains("hardware")
            || text_lower.contains("optimize")
            || text_lower.contains("performance")
        {
            intents.push(IntentCategory::HardwareOptimization);
        }

        // Adaptive Learning
        if text_lower.contains("feedback")
            || text_lower.contains("improve")
            || text_lower.contains("learn from")
            || text_lower.contains("adapt")
        {
            intents.push(IntentCategory::AdaptiveLearning);
        }

        // Default to conversation if no specific intent
        if intents.is_empty() {
            intents.push(IntentCategory::Conversation);
        }

        intents
    }

    /// Get all capabilities needed for detected intents
    pub fn capabilities_for_intents(intents: &[IntentCategory]) -> HashSet<Capability> {
        intents
            .iter()
            .flat_map(|i| i.required_capabilities())
            .collect()
    }

    /// Get all suggested plugins for detected intents
    pub fn plugins_for_intents(intents: &[IntentCategory]) -> HashSet<&'static str> {
        intents.iter().flat_map(|i| i.suggested_plugins()).collect()
    }
}

/// Capabilities that plugins can advertise
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    // ML Capabilities
    /// Core ML operations (model registry, inference engine)
    MLCore,
    /// ML inference capability
    MLInference,
    /// ML training capability
    MLTraining,
    /// Model compression (pruning, quantization)
    MLCompression,
    /// Hyperparameter optimization
    MLHyperparameterOptimization,

    // Framework Capabilities
    /// PyTorch backend available
    PyTorchBackend,
    /// TensorFlow backend available
    TensorFlowBackend,
    /// TensorFlow Lite for edge deployment
    TFLiteEdge,
    /// Distributed training capability
    DistributedTraining,

    // Workflow Capabilities
    /// Workflow orchestration engine
    WorkflowEngine,
    /// Task scheduling
    TaskScheduling,
    /// Pipeline orchestration
    PipelineOrchestration,

    // Production Capabilities
    /// Cascade inference (small -> medium -> large)
    CascadeInference,
    /// Memory compression and management
    MemoryManagement,
    /// Compliance automation (HIPAA, GDPR)
    ComplianceAutomation,

    // Learning Capabilities
    /// Adaptive learning (LoRA, continual learning)
    AdaptiveLearning,
    /// Human feedback collection
    HumanFeedback,
    /// Knowledge distillation
    KnowledgeDistillation,

    // Explainability Capabilities
    /// Reasoning chains
    ReasoningChains,
    /// Source attribution
    SourceAttribution,
    /// Tamper-evident audit logs
    TamperEvidentAudit,

    // Infrastructure Capabilities
    /// Hardware detection and optimization
    HardwareOptimization,
    /// Local vector database
    LocalVectorDB,
    /// Local LLM inference
    LocalLLMInference,

    // Compliance Capabilities
    /// PII detection and redaction
    PIIDetection,
    /// HIPAA compliance
    HIPAACompliance,
}

/// Registry for tracking available capabilities
#[derive(Debug, Default)]
pub struct CapabilityRegistry {
    capabilities: HashSet<Capability>,
    plugin_capabilities: HashMap<String, Vec<Capability>>,
}

impl CapabilityRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a capability
    pub fn register(&mut self, capability: Capability) {
        self.capabilities.insert(capability);
    }

    /// Register a capability from a specific plugin
    pub fn register_from_plugin(&mut self, plugin_name: &str, capability: Capability) {
        self.capabilities.insert(capability);
        self.plugin_capabilities
            .entry(plugin_name.to_string())
            .or_default()
            .push(capability);
    }

    /// Check if a capability is available
    pub fn has(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Check if all specified capabilities are available
    pub fn has_all(&self, capabilities: &[Capability]) -> bool {
        capabilities.iter().all(|c| self.capabilities.contains(c))
    }

    /// Check if any of the specified capabilities are available
    pub fn has_any(&self, capabilities: &[Capability]) -> bool {
        capabilities.iter().any(|c| self.capabilities.contains(c))
    }

    /// Get all registered capabilities
    pub fn all(&self) -> &HashSet<Capability> {
        &self.capabilities
    }

    /// Get capabilities registered by a specific plugin
    pub fn get_plugin_capabilities(&self, plugin_name: &str) -> Option<&Vec<Capability>> {
        self.plugin_capabilities.get(plugin_name)
    }

    /// Unregister all capabilities from a plugin
    pub fn unregister_plugin(&mut self, plugin_name: &str) {
        if let Some(caps) = self.plugin_capabilities.remove(plugin_name) {
            for cap in caps {
                // Only remove if no other plugin provides this capability
                let still_provided = self.plugin_capabilities.values().any(|v| v.contains(&cap));
                if !still_provided {
                    self.capabilities.remove(&cap);
                }
            }
        }
    }
}

/// Trait for plugins that can provide ML capabilities
#[async_trait]
pub trait MLCapable: Send + Sync {
    /// Run inference with the given input
    async fn infer(
        &self,
        model_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// List available models
    async fn list_models(&self) -> Vec<String>;

    /// Get model status
    async fn model_status(&self, model_name: &str) -> Option<ModelStatus>;
}

/// Trait for plugins that can provide workflow capabilities
#[async_trait]
pub trait WorkflowCapable: Send + Sync {
    /// Create a workflow
    async fn create_workflow(&self, config: WorkflowConfig) -> Result<String, String>;

    /// Execute a workflow by ID
    async fn execute_workflow(&self, workflow_id: &str) -> Result<WorkflowResult, String>;

    /// Get workflow status
    async fn workflow_status(&self, workflow_id: &str) -> Option<WorkflowStatus>;
}

/// Trait for plugins that can provide hardware optimization
#[async_trait]
pub trait HardwareCapable: Send + Sync {
    /// Get current hardware info
    async fn get_hardware_info(&self) -> HardwareInfo;

    /// Get optimization recommendations
    async fn get_recommendations(&self) -> Vec<OptimizationRecommendation>;
}

/// Trait for plugins that can provide explainability
#[async_trait]
pub trait ExplainabilityCapable: Send + Sync {
    /// Generate explanation for a decision
    async fn explain(&self, decision_id: &str) -> Option<Explanation>;

    /// Record a decision for audit
    async fn record_for_audit(&self, context: AuditContext) -> Result<String, String>;
}

/// Trait for plugins that can provide adaptive learning
#[async_trait]
pub trait AdaptiveCapable: Send + Sync {
    /// Add a training example to the replay buffer
    async fn add_example(&self, example: TrainingExample) -> Result<(), String>;

    /// Trigger fine-tuning if buffer is ready
    async fn maybe_finetune(&self) -> Option<FinetuneResult>;
}

/// Model status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatus {
    /// Model name
    pub name: String,
    /// Current state (ready, training, loading, error)
    pub state: String,
    /// Framework (pytorch, tensorflow)
    pub framework: String,
    /// Model version
    pub version: String,
    /// Performance metrics
    pub metrics: Option<serde_json::Value>,
}

/// Workflow configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    /// Workflow name
    pub name: String,
    /// Task definitions
    pub tasks: Vec<TaskConfig>,
}

/// Task configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// Task name
    pub name: String,
    /// Task type
    pub task_type: String,
    /// Dependencies (other task names)
    pub depends_on: Vec<String>,
    /// Task parameters
    pub params: serde_json::Value,
}

/// Workflow execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    /// Workflow ID
    pub workflow_id: String,
    /// Status
    pub status: String,
    /// Task results
    pub task_results: HashMap<String, serde_json::Value>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Workflow status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStatus {
    /// Workflow ID
    pub id: String,
    /// Status
    pub status: String,
    /// Progress (0.0 to 1.0)
    pub progress: f64,
    /// Current task
    pub current_task: Option<String>,
}

/// Hardware information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    /// CPU info
    pub cpu: CpuInfo,
    /// Memory info
    pub memory: MemoryInfo,
    /// GPU info (if available)
    pub gpu: Option<GpuInfo>,
}

/// CPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    /// Number of cores
    pub cores: usize,
    /// Number of threads
    pub threads: usize,
    /// Architecture
    pub architecture: String,
}

/// Memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    /// Total memory in GB
    pub total_gb: f64,
    /// Available memory in GB
    pub available_gb: f64,
}

/// GPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// GPU name
    pub name: String,
    /// VRAM in GB
    pub vram_gb: f64,
    /// Compute capability or backend
    pub compute: String,
}

/// Optimization recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRecommendation {
    /// Recommendation type
    pub recommendation_type: String,
    /// Description
    pub description: String,
    /// Impact level (low, medium, high)
    pub impact: String,
    /// Suggested action
    pub action: String,
}

/// Explanation for a decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    /// Decision ID
    pub decision_id: String,
    /// Reasoning steps
    pub reasoning_steps: Vec<String>,
    /// Confidence score
    pub confidence: f64,
    /// Sources referenced
    pub sources: Vec<String>,
    /// Alternatives considered
    pub alternatives: Vec<AlternativeOption>,
}

/// Alternative option considered
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeOption {
    /// Description
    pub description: String,
    /// Why not chosen
    pub rejection_reason: String,
    /// Hypothetical confidence
    pub hypothetical_confidence: f64,
}

/// Audit context for recording decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditContext {
    /// Decision type
    pub decision_type: String,
    /// Input data hash
    pub input_hash: String,
    /// Output data hash
    pub output_hash: String,
    /// Agent ID
    pub agent_id: String,
    /// Additional metadata
    pub metadata: serde_json::Value,
}

/// Training example for adaptive learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingExample {
    /// Input text
    pub input: String,
    /// Expected output
    pub output: String,
    /// Quality score (0.0 to 1.0)
    pub quality: f64,
    /// Source (user_feedback, system_generated, etc.)
    pub source: String,
}

/// Fine-tuning result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneResult {
    /// Adapter name
    pub adapter_name: String,
    /// Number of examples used
    pub examples_used: usize,
    /// Training loss
    pub final_loss: f64,
    /// Validation accuracy
    pub validation_accuracy: Option<f64>,
}

/// Integration bridge that connects available capabilities
pub struct IntegrationBridge {
    registry: Arc<RwLock<CapabilityRegistry>>,
    ml_providers: Arc<RwLock<Vec<Arc<dyn MLCapable>>>>,
    workflow_providers: Arc<RwLock<Vec<Arc<dyn WorkflowCapable>>>>,
    hardware_providers: Arc<RwLock<Vec<Arc<dyn HardwareCapable>>>>,
    explainability_providers: Arc<RwLock<Vec<Arc<dyn ExplainabilityCapable>>>>,
    adaptive_providers: Arc<RwLock<Vec<Arc<dyn AdaptiveCapable>>>>,
}

impl Default for IntegrationBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl IntegrationBridge {
    /// Create a new integration bridge
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(CapabilityRegistry::new())),
            ml_providers: Arc::new(RwLock::new(Vec::new())),
            workflow_providers: Arc::new(RwLock::new(Vec::new())),
            hardware_providers: Arc::new(RwLock::new(Vec::new())),
            explainability_providers: Arc::new(RwLock::new(Vec::new())),
            adaptive_providers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get the capability registry
    pub fn registry(&self) -> Arc<RwLock<CapabilityRegistry>> {
        self.registry.clone()
    }

    /// Register an ML-capable provider
    pub async fn register_ml_provider(&self, provider: Arc<dyn MLCapable>) {
        self.ml_providers.write().await.push(provider);
        self.registry.write().await.register(Capability::MLCore);
        self.registry
            .write()
            .await
            .register(Capability::MLInference);
    }

    /// Register a workflow-capable provider
    pub async fn register_workflow_provider(&self, provider: Arc<dyn WorkflowCapable>) {
        self.workflow_providers.write().await.push(provider);
        self.registry
            .write()
            .await
            .register(Capability::WorkflowEngine);
    }

    /// Register a hardware-capable provider
    pub async fn register_hardware_provider(&self, provider: Arc<dyn HardwareCapable>) {
        self.hardware_providers.write().await.push(provider);
        self.registry
            .write()
            .await
            .register(Capability::HardwareOptimization);
    }

    /// Register an explainability provider
    pub async fn register_explainability_provider(&self, provider: Arc<dyn ExplainabilityCapable>) {
        self.explainability_providers.write().await.push(provider);
        self.registry
            .write()
            .await
            .register(Capability::ReasoningChains);
        self.registry
            .write()
            .await
            .register(Capability::TamperEvidentAudit);
    }

    /// Register an adaptive learning provider
    pub async fn register_adaptive_provider(&self, provider: Arc<dyn AdaptiveCapable>) {
        self.adaptive_providers.write().await.push(provider);
        self.registry
            .write()
            .await
            .register(Capability::AdaptiveLearning);
    }

    // === Bridged Operations (Gracefully Degrading) ===

    /// Run inference if ML capability is available
    pub async fn try_infer(
        &self,
        model_name: &str,
        input: serde_json::Value,
    ) -> Option<serde_json::Value> {
        let providers = self.ml_providers.read().await;
        if let Some(provider) = providers.first() {
            provider.infer(model_name, input).await.ok()
        } else {
            tracing::debug!("ML inference not available - no ML provider registered");
            None
        }
    }

    /// Create and execute a workflow if capability is available
    pub async fn try_run_workflow(&self, config: WorkflowConfig) -> Option<WorkflowResult> {
        let providers = self.workflow_providers.read().await;
        if let Some(provider) = providers.first() {
            if let Ok(workflow_id) = provider.create_workflow(config).await {
                return provider.execute_workflow(&workflow_id).await.ok();
            }
        } else {
            tracing::debug!("Workflow not available - no workflow provider registered");
        }
        None
    }

    /// Get hardware optimization if capability is available
    pub async fn try_get_hardware_recommendations(&self) -> Vec<OptimizationRecommendation> {
        let providers = self.hardware_providers.read().await;
        if let Some(provider) = providers.first() {
            provider.get_recommendations().await
        } else {
            tracing::debug!("Hardware optimization not available");
            Vec::new()
        }
    }

    /// Record for audit if explainability is available
    pub async fn try_record_audit(&self, context: AuditContext) -> Option<String> {
        let providers = self.explainability_providers.read().await;
        if let Some(provider) = providers.first() {
            provider.record_for_audit(context).await.ok()
        } else {
            tracing::debug!("Audit recording not available - no explainability provider");
            None
        }
    }

    /// Add training example if adaptive learning is available
    pub async fn try_add_training_example(&self, example: TrainingExample) -> bool {
        let providers = self.adaptive_providers.read().await;
        if let Some(provider) = providers.first() {
            provider.add_example(example).await.is_ok()
        } else {
            tracing::debug!("Adaptive learning not available");
            false
        }
    }

    /// Get a summary of available integrations
    pub async fn integration_summary(&self) -> IntegrationSummary {
        let registry = self.registry.read().await;
        let capabilities = registry.all().clone();

        IntegrationSummary {
            ml_available: registry.has(Capability::MLCore),
            workflow_available: registry.has(Capability::WorkflowEngine),
            hardware_optimization: registry.has(Capability::HardwareOptimization),
            explainability_available: registry
                .has_any(&[Capability::ReasoningChains, Capability::TamperEvidentAudit]),
            adaptive_learning_available: registry.has(Capability::AdaptiveLearning),
            total_capabilities: capabilities.len(),
            capabilities: capabilities.into_iter().collect(),
        }
    }
}

/// Summary of available integrations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationSummary {
    /// Whether ML capabilities are available
    pub ml_available: bool,
    /// Whether workflow capabilities are available
    pub workflow_available: bool,
    /// Whether hardware optimization is available
    pub hardware_optimization: bool,
    /// Whether explainability is available
    pub explainability_available: bool,
    /// Whether adaptive learning is available
    pub adaptive_learning_available: bool,
    /// Total number of capabilities
    pub total_capabilities: usize,
    /// List of available capabilities
    pub capabilities: Vec<Capability>,
}

// ============================================================================
// DYNAMIC PLUGIN MANAGER
// ============================================================================

/// Manages dynamic plugin loading based on workflow needs
pub struct DynamicPluginManager {
    /// Plugin lifecycles
    lifecycles: Arc<RwLock<HashMap<String, PluginLifecycle>>>,
    /// Active plugin count by policy
    active_by_policy: Arc<RwLock<HashMap<PluginPolicy, usize>>>,
    /// Maximum memory budget for on-demand plugins (MB)
    memory_budget_mb: f64,
    /// Current memory usage (MB)
    current_memory_mb: Arc<RwLock<f64>>,
    /// Integration bridge reference
    bridge: Arc<IntegrationBridge>,
    /// Idle timeout for suspending plugins
    idle_timeout: Duration,
}

impl DynamicPluginManager {
    /// Create a new dynamic plugin manager
    pub fn new(bridge: Arc<IntegrationBridge>, memory_budget_mb: f64) -> Self {
        Self {
            lifecycles: Arc::new(RwLock::new(HashMap::new())),
            active_by_policy: Arc::new(RwLock::new(HashMap::new())),
            memory_budget_mb,
            current_memory_mb: Arc::new(RwLock::new(0.0)),
            bridge,
            idle_timeout: Duration::from_secs(300), // 5 minutes default
        }
    }

    /// Register an always-on plugin (will be immediately activated)
    pub async fn register_always_on(&self, name: &str, capabilities: Vec<Capability>) {
        let lifecycle = PluginLifecycle::always_on(name, capabilities.clone());

        // Register capabilities in the bridge
        for cap in &capabilities {
            self.bridge
                .registry()
                .write()
                .await
                .register_from_plugin(name, *cap);
        }

        self.lifecycles
            .write()
            .await
            .insert(name.to_string(), lifecycle);

        let mut counts = self.active_by_policy.write().await;
        *counts.entry(PluginPolicy::AlwaysOn).or_insert(0) += 1;

        tracing::info!(
            "Registered always-on plugin: {} with {:?}",
            name,
            capabilities
        );
    }

    /// Register an on-demand plugin (will be loaded when needed)
    pub async fn register_on_demand(
        &self,
        name: &str,
        capabilities: Vec<Capability>,
        memory_mb: f64,
    ) {
        let lifecycle = PluginLifecycle::on_demand(name, capabilities.clone(), memory_mb);
        self.lifecycles
            .write()
            .await
            .insert(name.to_string(), lifecycle);

        tracing::info!(
            "Registered on-demand plugin: {} ({:.1}MB) with {:?}",
            name,
            memory_mb,
            capabilities
        );
    }

    /// Ensure required capabilities are available, loading plugins if needed
    pub async fn ensure_capabilities(
        &self,
        required: &[Capability],
    ) -> Result<Vec<String>, String> {
        let mut loaded_plugins = Vec::new();

        for cap in required {
            // Check if capability is already available
            if self.bridge.registry().read().await.has(*cap) {
                continue;
            }

            // Find a plugin that provides this capability
            let lifecycles = self.lifecycles.read().await;
            let provider = lifecycles
                .values()
                .find(|l| l.capabilities.contains(cap) && l.state != PluginState::Active);

            if let Some(lifecycle) = provider {
                let plugin_name = lifecycle.name.clone();
                drop(lifecycles);

                // Load the plugin
                self.load_plugin(&plugin_name).await?;
                loaded_plugins.push(plugin_name);
            }
        }

        Ok(loaded_plugins)
    }

    /// Load a plugin by name
    pub async fn load_plugin(&self, name: &str) -> Result<(), String> {
        // First pass: check if already loaded or get memory requirement
        let (already_active, memory_needed) = {
            let mut lifecycles = self.lifecycles.write().await;
            let lifecycle = lifecycles
                .get_mut(name)
                .ok_or_else(|| format!("Plugin not registered: {}", name))?;

            if lifecycle.state == PluginState::Active {
                lifecycle.last_used = Some(chrono::Utc::now());
                return Ok(());
            }

            (lifecycle.state == PluginState::Active, lifecycle.memory_mb)
        };

        if already_active {
            return Ok(());
        }

        // Check memory budget and free if needed
        let current = *self.current_memory_mb.read().await;
        if current + memory_needed > self.memory_budget_mb {
            self.free_memory(memory_needed).await?;
        }

        // Now actually load the plugin
        let mut lifecycles = self.lifecycles.write().await;
        let lifecycle = lifecycles.get_mut(name).unwrap();
        lifecycle.state = PluginState::Loading;
        let capabilities = lifecycle.capabilities.clone();
        let memory = lifecycle.memory_mb;
        let policy = lifecycle.policy;

        // Simulate loading (in real implementation, would init the plugin)
        lifecycle.state = PluginState::Active;
        lifecycle.initialized = true;
        lifecycle.last_used = Some(chrono::Utc::now());
        drop(lifecycles);

        // Register capabilities
        for cap in &capabilities {
            self.bridge
                .registry()
                .write()
                .await
                .register_from_plugin(name, *cap);
        }

        // Update memory usage
        *self.current_memory_mb.write().await += memory;

        let mut counts = self.active_by_policy.write().await;
        *counts.entry(policy).or_insert(0) += 1;

        tracing::info!("Loaded plugin: {} ({:.1}MB)", name, memory);
        Ok(())
    }

    /// Unload a plugin by name (fails for always-on plugins)
    pub async fn unload_plugin(&self, name: &str) -> Result<(), String> {
        let mut lifecycles = self.lifecycles.write().await;
        let lifecycle = lifecycles
            .get_mut(name)
            .ok_or_else(|| format!("Plugin not registered: {}", name))?;

        // Cannot unload always-on plugins
        if lifecycle.policy == PluginPolicy::AlwaysOn {
            return Err(format!("Cannot unload always-on plugin: {}", name));
        }

        if lifecycle.state != PluginState::Active {
            return Ok(());
        }

        let memory = lifecycle.memory_mb;
        lifecycle.state = PluginState::Unloaded;

        // Unregister capabilities
        self.bridge.registry().write().await.unregister_plugin(name);

        // Update memory usage
        *self.current_memory_mb.write().await -= memory;

        let mut counts = self.active_by_policy.write().await;
        if let Some(count) = counts.get_mut(&lifecycle.policy) {
            *count = count.saturating_sub(1);
        }

        tracing::info!("Unloaded plugin: {} (freed {:.1}MB)", name, memory);
        Ok(())
    }

    /// Suspend a plugin (keep state, free resources)
    pub async fn suspend_plugin(&self, name: &str) -> Result<(), String> {
        let mut lifecycles = self.lifecycles.write().await;
        let lifecycle = lifecycles
            .get_mut(name)
            .ok_or_else(|| format!("Plugin not registered: {}", name))?;

        if !lifecycle.can_suspend() {
            return Err(format!("Cannot suspend plugin: {}", name));
        }

        let memory = lifecycle.memory_mb;
        lifecycle.state = PluginState::Suspended;

        *self.current_memory_mb.write().await -= memory;

        tracing::info!("Suspended plugin: {} (freed {:.1}MB)", name, memory);
        Ok(())
    }

    /// Resume a suspended plugin
    pub async fn resume_plugin(&self, name: &str) -> Result<(), String> {
        // First, check if plugin exists and get memory requirement
        let memory_needed = {
            let lifecycles = self.lifecycles.read().await;
            let lifecycle = lifecycles
                .get(name)
                .ok_or_else(|| format!("Plugin not registered: {}", name))?;

            if lifecycle.state != PluginState::Suspended {
                return Err(format!("Plugin is not suspended: {}", name));
            }

            lifecycle.memory_mb
        };

        // Check memory budget and free if needed
        let current = *self.current_memory_mb.read().await;
        if current + memory_needed > self.memory_budget_mb {
            self.free_memory(memory_needed).await?;
        }

        // Now actually resume
        let mut lifecycles = self.lifecycles.write().await;
        let lifecycle = lifecycles.get_mut(name).unwrap();
        lifecycle.state = PluginState::Active;
        lifecycle.last_used = Some(chrono::Utc::now());

        *self.current_memory_mb.write().await += lifecycle.memory_mb;

        tracing::info!("Resumed plugin: {}", name);
        Ok(())
    }

    /// Free memory by suspending or unloading idle plugins
    async fn free_memory(&self, needed_mb: f64) -> Result<(), String> {
        let mut freed = 0.0;
        let now = chrono::Utc::now();

        let lifecycles = self.lifecycles.read().await;
        let mut candidates: Vec<_> = lifecycles
            .values()
            .filter(|l| l.can_suspend() || l.can_unload())
            .filter(|l| {
                l.last_used
                    .map(|t| (now - t).num_seconds() > self.idle_timeout.as_secs() as i64)
                    .unwrap_or(true)
            })
            .map(|l| (l.name.clone(), l.memory_mb, l.policy))
            .collect();
        drop(lifecycles);

        // Sort by memory (largest first) to free memory efficiently
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (name, memory, policy) in candidates {
            if freed >= needed_mb {
                break;
            }

            // Prefer suspending over unloading
            if matches!(policy, PluginPolicy::Suspendable) {
                if self.suspend_plugin(&name).await.is_ok() {
                    freed += memory;
                }
            } else if self.unload_plugin(&name).await.is_ok() {
                freed += memory;
            }
        }

        if freed >= needed_mb {
            Ok(())
        } else {
            Err(format!(
                "Could not free enough memory: needed {:.1}MB, freed {:.1}MB",
                needed_mb, freed
            ))
        }
    }

    /// Process a user message and ensure required plugins are loaded
    pub async fn process_intent(&self, message: &str) -> ProcessedIntent {
        let intents = IntentDetector::detect(message);
        let required_caps = IntentDetector::capabilities_for_intents(&intents);
        let suggested_plugins = IntentDetector::plugins_for_intents(&intents);

        // Always ensure compliance capabilities
        let mut all_caps: Vec<_> = required_caps.into_iter().collect();
        for cap in AlwaysOnPlugins::required_capabilities() {
            if !all_caps.contains(&cap) {
                all_caps.push(cap);
            }
        }

        // Try to load required plugins
        let loaded = self
            .ensure_capabilities(&all_caps)
            .await
            .unwrap_or_default();

        ProcessedIntent {
            detected_intents: intents,
            required_capabilities: all_caps,
            suggested_plugins: suggested_plugins.into_iter().map(String::from).collect(),
            plugins_loaded: loaded,
        }
    }

    /// Get current plugin status summary
    pub async fn status(&self) -> PluginManagerStatus {
        let lifecycles = self.lifecycles.read().await;

        let always_on: Vec<_> = lifecycles
            .values()
            .filter(|l| l.policy == PluginPolicy::AlwaysOn)
            .map(|l| l.name.clone())
            .collect();

        let active: Vec<_> = lifecycles
            .values()
            .filter(|l| l.state == PluginState::Active && l.policy != PluginPolicy::AlwaysOn)
            .map(|l| l.name.clone())
            .collect();

        let suspended: Vec<_> = lifecycles
            .values()
            .filter(|l| l.state == PluginState::Suspended)
            .map(|l| l.name.clone())
            .collect();

        let unloaded: Vec<_> = lifecycles
            .values()
            .filter(|l| l.state == PluginState::Unloaded)
            .map(|l| l.name.clone())
            .collect();

        PluginManagerStatus {
            always_on_plugins: always_on,
            active_plugins: active,
            suspended_plugins: suspended,
            unloaded_plugins: unloaded,
            memory_used_mb: *self.current_memory_mb.read().await,
            memory_budget_mb: self.memory_budget_mb,
        }
    }
}

/// Result of processing user intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedIntent {
    /// Detected intent categories
    pub detected_intents: Vec<IntentCategory>,
    /// Required capabilities
    pub required_capabilities: Vec<Capability>,
    /// Suggested plugins
    pub suggested_plugins: Vec<String>,
    /// Plugins that were loaded to satisfy requirements
    pub plugins_loaded: Vec<String>,
}

/// Plugin manager status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManagerStatus {
    /// Always-on plugins (cannot be disabled)
    pub always_on_plugins: Vec<String>,
    /// Currently active on-demand plugins
    pub active_plugins: Vec<String>,
    /// Suspended plugins (quick resume)
    pub suspended_plugins: Vec<String>,
    /// Unloaded plugins
    pub unloaded_plugins: Vec<String>,
    /// Current memory usage
    pub memory_used_mb: f64,
    /// Memory budget
    pub memory_budget_mb: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_registry() {
        let mut registry = CapabilityRegistry::new();

        registry.register(Capability::MLCore);
        registry.register(Capability::MLInference);

        assert!(registry.has(Capability::MLCore));
        assert!(registry.has(Capability::MLInference));
        assert!(!registry.has(Capability::WorkflowEngine));

        assert!(registry.has_all(&[Capability::MLCore, Capability::MLInference]));
        assert!(!registry.has_all(&[Capability::MLCore, Capability::WorkflowEngine]));

        assert!(registry.has_any(&[Capability::MLCore, Capability::WorkflowEngine]));
        assert!(!registry.has_any(&[Capability::WorkflowEngine, Capability::PyTorchBackend]));
    }

    #[test]
    fn test_plugin_capability_tracking() {
        let mut registry = CapabilityRegistry::new();

        registry.register_from_plugin("ml", Capability::MLCore);
        registry.register_from_plugin("ml", Capability::MLInference);
        registry.register_from_plugin("pytorch", Capability::PyTorchBackend);

        assert_eq!(registry.get_plugin_capabilities("ml").unwrap().len(), 2);
        assert_eq!(
            registry.get_plugin_capabilities("pytorch").unwrap().len(),
            1
        );

        registry.unregister_plugin("pytorch");
        assert!(!registry.has(Capability::PyTorchBackend));
        assert!(registry.has(Capability::MLCore)); // Still provided by ml plugin
    }

    #[tokio::test]
    async fn test_integration_bridge() {
        let bridge = IntegrationBridge::new();

        // Without any providers, operations should gracefully return None
        let result = bridge.try_infer("test", serde_json::json!({})).await;
        assert!(result.is_none());

        let summary = bridge.integration_summary().await;
        assert!(!summary.ml_available);
        assert!(!summary.workflow_available);
        assert_eq!(summary.total_capabilities, 0);
    }
}
