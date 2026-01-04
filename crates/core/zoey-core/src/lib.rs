//! ZoeyAI Core Runtime
//!
//! This crate provides the core runtime, types, and interfaces for building
//! AI agents optimized for local model deployment. It includes:
//!
//! - Agent runtime with resource-efficient execution
//! - Plugin system with dependency resolution
//! - Memory management optimized for edge devices
//! - Local model integration (Ollama, llama.cpp, LocalAI)
//! - Event system for pub/sub messaging
//! - Planning and cost management system
//! - Privacy-first, offline-capable architecture
//!
//! # Example: Local Model Agent
//!
//! ```no_run
//! use zoey_core::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Initialize runtime for local Ollama model
//!     let runtime = AgentRuntime::new(RuntimeOpts::default()).await?;
//!     // Agent runs 100% locally, no cloud dependencies
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::too_many_arguments)]

// Re-export commonly used types
pub use uuid::Uuid;

// Core modules
pub mod actions;
pub mod agent_api;
pub mod character_loader;
pub mod config;
pub mod distributed;
pub mod dynamic_prompts;
pub mod zoeyos;
pub mod entities;
pub mod error;
pub mod function_calling;
pub mod infrastructure;
pub mod ipo;
pub mod message;
pub mod ml_bridge;
pub mod multi_agent;
pub mod observability;
pub mod planner;
pub mod plugin;
pub mod plugin_system;
pub mod preprocessor;
pub mod resilience;
pub mod roles;
pub mod runtime;
pub mod runtime_ref;
pub mod secrets;
pub mod security;
pub mod streaming;
pub mod templates;
pub mod testing;
pub mod training;
pub mod types;
pub mod utils;
#[cfg(feature = "otel")]
pub use infrastructure::otel;
pub mod detectors;
pub mod extensions;
pub mod integration;
pub mod nlp;
pub mod pipeline;
pub mod workers;

// Re-export main types
pub use actions::{compose_action_examples, format_action_names, format_actions};
pub use character_loader::{load_character_from_xml, parse_character_xml};
pub use config::{
    get_env_bool, get_env_float, get_env_int, get_env_or, get_required_env, load_env,
    load_env_from_path, validate_env,
};
pub use distributed::{
    ClusterConfig, DistributedMessage, DistributedRuntime, NodeInfo, NodeStatus,
};
pub use dynamic_prompts::{
    compose_random_user, upgrade_double_to_triple, DynamicPromptExecutor, DynamicPromptOptions,
    MetricsSummary, ModelMetrics, ResponseFormat, SchemaMetrics, SchemaRow, SchemaType,
    ValidationLevel,
};
pub use zoeyos::{
    ZoeyOS, ZoeyOSMetrics, HealthStatus as ZoeyOSHealthStatus, SendMessageOptions,
    SendMessageResult,
};
pub use entities::{
    create_unique_uuid_for_entity, find_entity_by_name, find_entity_by_name_with_config,
    format_entities, get_entity_details, get_recent_interactions, EntityResolutionConfig,
};
pub use error::{ZoeyError, Result};
pub use function_calling::{
    create_function_definition, FunctionCall, FunctionDefinition, FunctionHandler,
    FunctionRegistry, FunctionResult,
};
pub use integration::{
    AdaptiveCapable,
    AlternativeOption,
    AlwaysOnPlugins,
    AuditContext,
    // Core integration
    Capability,
    CapabilityRegistry,
    CpuInfo,
    // Dynamic plugin management
    DynamicPluginManager,
    ExplainabilityCapable,
    Explanation,
    FinetuneResult,
    GpuInfo,
    HardwareCapable,
    HardwareInfo,
    IntegrationBridge,
    IntegrationSummary,
    // Intent detection
    IntentCategory,
    IntentDetector,
    // Capability traits
    MLCapable,
    MemoryInfo,
    // Data types
    ModelStatus,
    OptimizationRecommendation,
    PluginLifecycle,
    PluginManagerStatus,
    // Plugin lifecycle
    PluginPolicy,
    PluginState,
    ProcessedIntent,
    TaskConfig,
    TrainingExample,
    WorkflowCapable,
    WorkflowConfig,
    WorkflowResult,
    WorkflowStatus,
};
pub use ipo::{create_government_pipeline, IPOPipeline, Input, Output, Process, ProcessDecision};
pub use message::MessageProcessor;
pub use ml_bridge::{
    MLBridge, MLFramework, ModelInterface, PythonEnvironment, SecurityConfig, TrainedModel,
};
pub use multi_agent::{
    AgentCapability, AgentStatus as MultiAgentStatus, CoordinationMessage, CoordinationMessageType,
    MultiAgentCoordinator, MultiAgentService,
};
pub use observability::{
    CostSummary, CostTracker, CostTrackingConfig, LLMCallContext, LLMCostRecord, Observability,
    ObservabilityConfig, PromptStorageConfig, ProviderPricing, RestApiConfig,
};
pub use planner::{
    AgentBudget, BudgetAction, BudgetCheckResult, BudgetManager, ComplexityAnalyzer,
    ComplexityAssessment, ComplexityLevel, CostCalculator, CostEstimate, EmojiPlanner,
    EmojiStrategy, EmojiTone, EmojiType, ExecutionPlan, ExecutionRecord, KnowledgeAnalyzer,
    KnowledgeGap, KnowledgeState, MetricsTracker, ModelPricing, Optimization, PlanOptimizer,
    Planner, PlannerConfig, PlannerMetrics, Priority, ResponseStrategy, ResponseTone, ResponseType,
    TokenBudget, TokenCounter, TokenEstimate, TokenTracker,
};
pub use plugin::{
    get_plugin_actions, get_plugin_evaluators, get_plugin_providers, get_plugin_services,
    initialize_plugins, load_plugins, resolve_plugin_dependencies, validate_plugin,
};
pub use resilience::{
    retry_with_backoff, CircuitBreaker, CircuitState, HealthCheck, HealthChecker, HealthStatus,
    RetryConfig,
};
pub use roles::{
    find_worlds_for_owner, get_user_world_role, is_admin_or_owner, is_moderator_or_higher, Role,
};
pub use runtime::{AgentRuntime, RuntimeOpts};
pub use runtime_ref::{downcast_runtime_ref, RuntimeRef};
pub use secrets::{
    get_secret, has_character_secrets, load_secret_from_env, remove_secret,
    set_default_secrets_from_env, set_secret,
};
pub use security::{
    decrypt_secret, encrypt_secret, hash_password, sanitize_input, validate_input, verify_password,
    RateLimiter,
};
pub use streaming::{
    collect_stream, create_text_stream, StreamHandler, TextChunk, TextStream, TextStreamSender,
};
pub use templates::{
    compose_prompt_from_state, TemplateEngine, MESSAGE_HANDLER_TEMPLATE, POST_CREATION_TEMPLATE,
};
pub use testing::{create_mock_runtime, create_test_memory, create_test_room, run_test_suite};
pub use training::{
    create_training_collector, DatasetBuilder, DatasetStatistics, RLHFManager, TrainingCollector,
    TrainingConfig, TrainingFormat, TrainingSample,
};
pub use types::*;
pub use utils::{create_unique_uuid, string_to_uuid, Logger, BM25};

// Extension traits for enterprise
pub use extensions::{
    // Learning
    LearningProvider, LearningFeedback, FeedbackSource, TrainingResult,
    BasicLearningProvider,
    // Compliance
    ComplianceProvider, PiiFinding, PiiType, Severity, ComplianceFramework,
    ComplianceCheckResult, ComplianceFinding, AuditEntry, AuditOutcome,
    ComplianceAuditReport, BasicComplianceProvider,
    // Distributed (renamed to avoid conflict with distributed module)
    DistributedExecutor,
    NodeInfo as ExtNodeInfo,
    NodeStatus as ExtNodeStatus,
    NodeResources as ExtNodeResources,
    DistributedTask, DistributedTaskResult, ClusterStatus,
    // Policy
    PolicyProvider, PolicyRule, PolicyRuleType, PolicyDecision,
    // Identity
    IdentityProvider, Identity, ConsentScope, DataExportRequest,
    DataDeletionRequest, DataRequestStatus,
    // Registry
    ExtensionRegistry,
};
