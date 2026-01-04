//! # Extension Points for Enterprise Features
//!
//! This module defines traits that enterprise plugins can implement to extend
//! ZoeyOS functionality. The consumer codebase provides default implementations
//! that work standalone, while enterprise can provide enhanced versions.
//!
//! ## Design Principles
//!
//! 1. **Consumer Complete**: Consumer features work without enterprise
//! 2. **Drop-in Replacement**: Enterprise implementations can replace consumer
//! 3. **Graceful Degradation**: Missing enterprise features don't break consumer
//! 4. **Type Safety**: Shared types ensure compatibility
//!
//! ## Usage
//!
//! ```rust
//! use zoey_core::extensions::{LearningProvider, ComplianceProvider};
//!
//! // Consumer provides basic implementations
//! let basic_compliance = BasicComplianceProvider::new();
//!
//! // Enterprise provides enhanced implementations
//! // let enterprise_compliance = HipaaComplianceProvider::new(); // Enterprise
//! ```

use crate::types::UUID;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// LEARNING PROVIDER - For ML/Training Features
// ============================================================================

/// Feedback from user or system about agent response quality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningFeedback {
    /// ID of the response being rated
    pub response_id: UUID,
    /// Score from -1.0 (bad) to 1.0 (good)
    pub score: f32,
    /// Optional text feedback
    pub text: Option<String>,
    /// Feedback source (user, evaluator, system)
    pub source: FeedbackSource,
    /// Timestamp
    pub timestamp: i64,
}

/// Source of feedback
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackSource {
    /// End user feedback
    User,
    /// Automated evaluator
    Evaluator,
    /// System-generated (quality heuristics)
    System,
    /// Expert annotation
    Expert,
}

/// Result of a training operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingResult {
    /// Whether training succeeded
    pub success: bool,
    /// Training loss (if applicable)
    pub loss: Option<f32>,
    /// Number of examples trained on
    pub examples_count: usize,
    /// Training duration in seconds
    pub duration_secs: f64,
    /// Error message if failed
    pub error: Option<String>,
    /// Additional metrics
    pub metrics: HashMap<String, f64>,
}

/// Learning provider trait - allows enterprise to add training capabilities
///
/// Consumer implementation: Basic feedback collection, no training
/// Enterprise implementation: LoRA fine-tuning, continual learning, RLHF
#[async_trait]
pub trait LearningProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Collect feedback for a response (Consumer: stores locally)
    async fn collect_feedback(&self, feedback: LearningFeedback) -> Result<()>;

    /// Get collected feedback for analysis
    async fn get_feedback(&self, limit: usize) -> Result<Vec<LearningFeedback>>;

    /// Check if training is available (Consumer: returns false)
    fn supports_training(&self) -> bool {
        false
    }

    /// Train on collected feedback (Enterprise only)
    /// Consumer implementation returns Ok with success=false
    async fn train(&self) -> Result<TrainingResult> {
        Ok(TrainingResult {
            success: false,
            loss: None,
            examples_count: 0,
            duration_secs: 0.0,
            error: Some("Training requires enterprise license".to_string()),
            metrics: HashMap::new(),
        })
    }

    /// Enable continual learning mode (Enterprise only)
    async fn enable_continual_learning(&self) -> Result<bool> {
        Ok(false)
    }

    /// Load a trained adapter (Enterprise only)
    async fn load_adapter(&self, _adapter_name: &str) -> Result<bool> {
        Ok(false)
    }

    /// List available adapters (Enterprise only)
    async fn list_adapters(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }
}

// ============================================================================
// COMPLIANCE PROVIDER - For Regulatory Features
// ============================================================================

/// PII finding from compliance scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiFinding {
    /// Type of PII found
    pub pii_type: PiiType,
    /// Start position in text
    pub start: usize,
    /// End position in text
    pub end: usize,
    /// The matched text (may be redacted in logs)
    pub matched_text: String,
    /// Severity level
    pub severity: Severity,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

/// Types of PII that can be detected
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiiType {
    /// Social Security Number
    Ssn,
    /// Email address
    Email,
    /// Phone number
    Phone,
    /// Credit card number
    CreditCard,
    /// IP address
    IpAddress,
    /// API key or token
    ApiKey,
    /// Person name
    Name,
    /// Physical address
    Address,
    /// Date of birth
    DateOfBirth,
    /// Medical record number
    MedicalId,
    /// Custom pattern
    Custom(String),
}

/// Severity level for findings
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

/// Compliance framework
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplianceFramework {
    /// HIPAA (healthcare)
    Hipaa,
    /// GDPR (EU data protection)
    Gdpr,
    /// FDA (medical devices)
    Fda,
    /// PCI-DSS (payment cards)
    PciDss,
    /// SOC2 (service organizations)
    Soc2,
}

/// Compliance check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceCheckResult {
    /// Framework checked
    pub framework: ComplianceFramework,
    /// Whether compliant
    pub compliant: bool,
    /// Findings/violations
    pub findings: Vec<ComplianceFinding>,
    /// Recommendations
    pub recommendations: Vec<String>,
    /// Timestamp
    pub checked_at: i64,
}

/// Individual compliance finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceFinding {
    /// Finding code/ID
    pub code: String,
    /// Description
    pub description: String,
    /// Severity
    pub severity: Severity,
    /// Remediation steps
    pub remediation: Option<String>,
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique ID
    pub id: UUID,
    /// Timestamp
    pub timestamp: i64,
    /// Actor (user/agent/system)
    pub actor: String,
    /// Action performed
    pub action: String,
    /// Resource affected
    pub resource: String,
    /// Outcome (success/failure)
    pub outcome: AuditOutcome,
    /// Additional context
    pub context: HashMap<String, serde_json::Value>,
}

/// Audit outcome
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditOutcome {
    /// Action succeeded
    Success,
    /// Action failed
    Failure,
    /// Action was denied
    Denied,
}

/// Compliance provider trait - allows enterprise to add compliance features
///
/// Consumer implementation: Basic PII detection and redaction
/// Enterprise implementation: Full HIPAA/GDPR/FDA signals, encrypted audit logs
#[async_trait]
pub trait ComplianceProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    // === Consumer Features (always available) ===

    /// Scan text for PII (Consumer: basic patterns)
    async fn scan_pii(&self, text: &str) -> Result<Vec<PiiFinding>>;

    /// Redact PII from text (Consumer: basic redaction)
    fn redact(&self, text: &str) -> String;

    // === Enterprise Features (return None/empty in consumer) ===

    /// Check whether framework compliance is supported
    fn supports_framework(&self, _framework: ComplianceFramework) -> bool {
        false
    }

    /// Check compliance against a framework (Enterprise only)
    async fn check_compliance(
        &self,
        _framework: ComplianceFramework,
        _context: &str,
    ) -> Result<Option<ComplianceCheckResult>> {
        Ok(None)
    }

    /// Log an audit entry (Enterprise: encrypted, Consumer: no-op)
    async fn audit_log(&self, _entry: AuditEntry) -> Result<()> {
        Ok(())
    }

    /// Get audit logs (Enterprise only)
    async fn get_audit_logs(
        &self,
        _start: i64,
        _end: i64,
        _limit: usize,
    ) -> Result<Vec<AuditEntry>> {
        Ok(vec![])
    }

    /// Run full compliance audit (Enterprise only)
    async fn run_audit(&self, _auditor: &str) -> Result<Option<ComplianceAuditReport>> {
        Ok(None)
    }
}

/// Full compliance audit report (Enterprise)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAuditReport {
    /// Audit ID
    pub id: UUID,
    /// Auditor name
    pub auditor: String,
    /// Frameworks audited
    pub frameworks: Vec<ComplianceFramework>,
    /// Overall compliance percentage
    pub compliance_percentage: f32,
    /// Findings by framework
    pub findings: HashMap<String, Vec<ComplianceFinding>>,
    /// Generated at
    pub generated_at: i64,
}

// ============================================================================
// DISTRIBUTED EXECUTOR - For Distributed Workloads
// ============================================================================

/// Node information for distributed execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node ID
    pub id: String,
    /// Node address
    pub address: String,
    /// Node status
    pub status: NodeStatus,
    /// Available resources
    pub resources: NodeResources,
    /// Last heartbeat
    pub last_heartbeat: i64,
}

/// Node status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is healthy and available
    Healthy,
    /// Node is overloaded
    Overloaded,
    /// Node is unhealthy/unreachable
    Unhealthy,
    /// Node is draining (no new work)
    Draining,
}

/// Node resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResources {
    /// CPU cores available
    pub cpu_cores: f32,
    /// Memory available (MB)
    pub memory_mb: u64,
    /// GPU devices available
    pub gpu_devices: u32,
}

/// Distributed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedTask {
    /// Task ID
    pub id: UUID,
    /// Task type
    pub task_type: String,
    /// Task payload
    pub payload: serde_json::Value,
    /// Resource requirements
    pub requirements: NodeResources,
    /// Priority (higher = more important)
    pub priority: i32,
}

/// Distributed task result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedTaskResult {
    /// Task ID
    pub task_id: UUID,
    /// Node that executed
    pub executed_by: String,
    /// Success status
    pub success: bool,
    /// Result data
    pub result: Option<serde_json::Value>,
    /// Error if failed
    pub error: Option<String>,
    /// Execution time (ms)
    pub duration_ms: u64,
}

/// Distributed executor trait - allows enterprise to add distributed execution
///
/// Consumer implementation: Single-node execution only
/// Enterprise implementation: Multi-node coordination, load balancing
#[async_trait]
pub trait DistributedExecutor: Send + Sync {
    /// Executor name
    fn name(&self) -> &str;

    /// Check if distributed execution is available
    fn is_distributed(&self) -> bool {
        false
    }

    /// Get cluster nodes (Consumer: returns self only)
    async fn get_nodes(&self) -> Result<Vec<NodeInfo>> {
        Ok(vec![])
    }

    /// Submit task for distributed execution
    /// Consumer: executes locally, Enterprise: distributes to cluster
    async fn submit_task(&self, task: DistributedTask) -> Result<UUID>;

    /// Get task result
    async fn get_task_result(&self, task_id: UUID) -> Result<Option<DistributedTaskResult>>;

    /// Wait for task completion
    async fn wait_for_task(
        &self,
        task_id: UUID,
        timeout_ms: u64,
    ) -> Result<Option<DistributedTaskResult>>;

    // === Enterprise Features ===

    /// Join a cluster (Enterprise only)
    async fn join_cluster(&self, _coordinator: &str) -> Result<bool> {
        Ok(false)
    }

    /// Leave cluster (Enterprise only)
    async fn leave_cluster(&self) -> Result<bool> {
        Ok(false)
    }

    /// Get cluster status (Enterprise only)
    async fn cluster_status(&self) -> Result<Option<ClusterStatus>> {
        Ok(None)
    }
}

/// Cluster status (Enterprise)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    /// Cluster name
    pub name: String,
    /// Number of nodes
    pub node_count: usize,
    /// Healthy node count
    pub healthy_nodes: usize,
    /// Total available resources
    pub total_resources: NodeResources,
    /// Tasks in queue
    pub queued_tasks: usize,
    /// Tasks running
    pub running_tasks: usize,
}

// ============================================================================
// POLICY PROVIDER - For Governance Features
// ============================================================================

/// Policy rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Rule ID
    pub id: String,
    /// Rule name
    pub name: String,
    /// Rule type
    pub rule_type: PolicyRuleType,
    /// Configuration
    pub config: serde_json::Value,
    /// Whether rule is enabled
    pub enabled: bool,
}

/// Policy rule types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyRuleType {
    /// Rate limiting
    RateLimit,
    /// Cost cap
    CostCap,
    /// Content filtering
    ContentFilter,
    /// Tool permission
    ToolPermission,
    /// Custom rule
    Custom,
}

/// Policy decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    /// Whether action is allowed
    pub allowed: bool,
    /// Reason for decision
    pub reason: String,
    /// Rules that matched
    pub matched_rules: Vec<String>,
}

/// Policy provider trait - allows enterprise to add governance features
///
/// Consumer implementation: No policy enforcement
/// Enterprise implementation: Rate limits, cost caps, content rules
#[async_trait]
pub trait PolicyProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Check if policy engine is enabled
    fn is_enabled(&self) -> bool {
        false
    }

    /// Check policy for an action (Enterprise only)
    async fn check_policy(
        &self,
        _action: &str,
        _context: &HashMap<String, serde_json::Value>,
    ) -> Result<PolicyDecision> {
        Ok(PolicyDecision {
            allowed: true,
            reason: "Policy engine not enabled".to_string(),
            matched_rules: vec![],
        })
    }

    /// Get active rules (Enterprise only)
    async fn get_rules(&self) -> Result<Vec<PolicyRule>> {
        Ok(vec![])
    }

    /// Set a rule (Enterprise only)
    async fn set_rule(&self, _rule: PolicyRule) -> Result<bool> {
        Ok(false)
    }

    /// Delete a rule (Enterprise only)
    async fn delete_rule(&self, _rule_id: &str) -> Result<bool> {
        Ok(false)
    }
}

// ============================================================================
// IDENTITY PROVIDER - For User Management Features
// ============================================================================

/// User identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// Identity ID
    pub id: UUID,
    /// User handle/name
    pub handle: String,
    /// Consent scopes granted
    pub consents: Vec<ConsentScope>,
    /// Data retention days
    pub retention_days: i32,
    /// Created at
    pub created_at: i64,
}

/// Consent scope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentScope {
    /// Scope name
    pub scope: String,
    /// Whether granted
    pub granted: bool,
    /// Granted at
    pub granted_at: Option<i64>,
}

/// Data export request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataExportRequest {
    /// Request ID
    pub id: UUID,
    /// Identity ID
    pub identity_id: UUID,
    /// Export format
    pub format: String,
    /// Status
    pub status: DataRequestStatus,
    /// Download URL (when ready)
    pub download_url: Option<String>,
}

/// Data deletion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataDeletionRequest {
    /// Request ID
    pub id: UUID,
    /// Identity ID
    pub identity_id: UUID,
    /// Status
    pub status: DataRequestStatus,
    /// Completed at
    pub completed_at: Option<i64>,
}

/// Status of data request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataRequestStatus {
    /// Request pending
    Pending,
    /// Request in progress
    InProgress,
    /// Request completed
    Completed,
    /// Request failed
    Failed,
}

/// Identity provider trait - allows enterprise to add identity management
///
/// Consumer implementation: No identity management
/// Enterprise implementation: GDPR data rights, consent management
#[async_trait]
pub trait IdentityProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Check if identity management is enabled
    fn is_enabled(&self) -> bool {
        false
    }

    /// Get identity by ID (Enterprise only)
    async fn get_identity(&self, _id: UUID) -> Result<Option<Identity>> {
        Ok(None)
    }

    /// Update consent (Enterprise only)
    async fn update_consent(
        &self,
        _identity_id: UUID,
        _scope: &str,
        _granted: bool,
    ) -> Result<bool> {
        Ok(false)
    }

    /// Request data export (Enterprise only)
    async fn request_export(&self, _identity_id: UUID, _format: &str) -> Result<Option<UUID>> {
        Ok(None)
    }

    /// Request data deletion (Enterprise only)
    async fn request_deletion(&self, _identity_id: UUID) -> Result<Option<UUID>> {
        Ok(None)
    }

    /// Set retention policy (Enterprise only)
    async fn set_retention(&self, _identity_id: UUID, _days: i32) -> Result<bool> {
        Ok(false)
    }
}

// ============================================================================
// DEFAULT IMPLEMENTATIONS (Consumer)
// ============================================================================

/// Basic learning provider (Consumer) - collects feedback but doesn't train
pub struct BasicLearningProvider {
    feedback: std::sync::RwLock<Vec<LearningFeedback>>,
}

impl BasicLearningProvider {
    /// Create new basic learning provider
    pub fn new() -> Self {
        Self {
            feedback: std::sync::RwLock::new(Vec::new()),
        }
    }
}

impl Default for BasicLearningProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LearningProvider for BasicLearningProvider {
    fn name(&self) -> &str {
        "basic-learning"
    }

    async fn collect_feedback(&self, feedback: LearningFeedback) -> Result<()> {
        let mut store = self.feedback.write().unwrap();
        store.push(feedback);
        // Keep only last 1000 feedback items
        if store.len() > 1000 {
            store.remove(0);
        }
        Ok(())
    }

    async fn get_feedback(&self, limit: usize) -> Result<Vec<LearningFeedback>> {
        let store = self.feedback.read().unwrap();
        Ok(store.iter().rev().take(limit).cloned().collect())
    }
}

/// Basic compliance provider (Consumer) - simple PII detection
pub struct BasicComplianceProvider {
    patterns: Vec<(PiiType, regex::Regex)>,
}

impl BasicComplianceProvider {
    /// Create new basic compliance provider
    pub fn new() -> Self {
        let patterns = vec![
            (
                PiiType::Ssn,
                regex::Regex::new(r"\b\d{3}[-\s]?\d{2}[-\s]?\d{4}\b").unwrap(),
            ),
            (
                PiiType::Email,
                regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap(),
            ),
            (
                PiiType::Phone,
                regex::Regex::new(r"\b(\+?1[-.\s]?)?(\(?\d{3}\)?[-.\s]?)?\d{3}[-.\s]?\d{4}\b")
                    .unwrap(),
            ),
            (
                PiiType::CreditCard,
                regex::Regex::new(r"\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b").unwrap(),
            ),
            (
                PiiType::ApiKey,
                regex::Regex::new(r"\b(sk-[a-zA-Z0-9]{32,}|api[_-]?key[_-]?[a-zA-Z0-9]{16,})\b")
                    .unwrap(),
            ),
        ];
        Self { patterns }
    }
}

impl Default for BasicComplianceProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ComplianceProvider for BasicComplianceProvider {
    fn name(&self) -> &str {
        "basic-compliance"
    }

    async fn scan_pii(&self, text: &str) -> Result<Vec<PiiFinding>> {
        let mut findings = Vec::new();

        for (pii_type, pattern) in &self.patterns {
            for m in pattern.find_iter(text) {
                findings.push(PiiFinding {
                    pii_type: pii_type.clone(),
                    start: m.start(),
                    end: m.end(),
                    matched_text: m.as_str().to_string(),
                    severity: match pii_type {
                        PiiType::Ssn | PiiType::CreditCard | PiiType::ApiKey => Severity::Critical,
                        PiiType::Email | PiiType::Phone => Severity::Medium,
                        _ => Severity::Low,
                    },
                    confidence: 0.9,
                });
            }
        }

        Ok(findings)
    }

    fn redact(&self, text: &str) -> String {
        let mut result = text.to_string();

        for (pii_type, pattern) in &self.patterns {
            let replacement = match pii_type {
                PiiType::Ssn => "[SSN]",
                PiiType::Email => "[EMAIL]",
                PiiType::Phone => "[PHONE]",
                PiiType::CreditCard => "[CREDIT_CARD]",
                PiiType::ApiKey => "[API_KEY]",
                _ => "[REDACTED]",
            };
            result = pattern.replace_all(&result, replacement).to_string();
        }

        result
    }
}

// ============================================================================
// EXTENSION REGISTRY
// ============================================================================

/// Registry for extension providers
pub struct ExtensionRegistry {
    /// Learning provider
    pub learning: Option<Arc<dyn LearningProvider>>,
    /// Compliance provider
    pub compliance: Option<Arc<dyn ComplianceProvider>>,
    /// Distributed executor
    pub distributed: Option<Arc<dyn DistributedExecutor>>,
    /// Policy provider
    pub policy: Option<Arc<dyn PolicyProvider>>,
    /// Identity provider
    pub identity: Option<Arc<dyn IdentityProvider>>,
}

impl ExtensionRegistry {
    /// Create new registry with default (consumer) providers
    pub fn new() -> Self {
        Self {
            learning: Some(Arc::new(BasicLearningProvider::new())),
            compliance: Some(Arc::new(BasicComplianceProvider::new())),
            distributed: None,
            policy: None,
            identity: None,
        }
    }

    /// Create empty registry (no providers)
    pub fn empty() -> Self {
        Self {
            learning: None,
            compliance: None,
            distributed: None,
            policy: None,
            identity: None,
        }
    }

    /// Set learning provider
    pub fn with_learning(mut self, provider: Arc<dyn LearningProvider>) -> Self {
        self.learning = Some(provider);
        self
    }

    /// Set compliance provider
    pub fn with_compliance(mut self, provider: Arc<dyn ComplianceProvider>) -> Self {
        self.compliance = Some(provider);
        self
    }

    /// Set distributed executor
    pub fn with_distributed(mut self, executor: Arc<dyn DistributedExecutor>) -> Self {
        self.distributed = Some(executor);
        self
    }

    /// Set policy provider
    pub fn with_policy(mut self, provider: Arc<dyn PolicyProvider>) -> Self {
        self.policy = Some(provider);
        self
    }

    /// Set identity provider
    pub fn with_identity(mut self, provider: Arc<dyn IdentityProvider>) -> Self {
        self.identity = Some(provider);
        self
    }

    /// Check if enterprise features are available
    pub fn has_enterprise_features(&self) -> bool {
        self.learning
            .as_ref()
            .map(|l| l.supports_training())
            .unwrap_or(false)
            || self
                .compliance
                .as_ref()
                .map(|c| c.supports_framework(ComplianceFramework::Hipaa))
                .unwrap_or(false)
            || self
                .distributed
                .as_ref()
                .map(|d| d.is_distributed())
                .unwrap_or(false)
            || self.policy.as_ref().map(|p| p.is_enabled()).unwrap_or(false)
            || self
                .identity
                .as_ref()
                .map(|i| i.is_enabled())
                .unwrap_or(false)
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_learning_provider() {
        let provider = BasicLearningProvider::new();

        let feedback = LearningFeedback {
            response_id: uuid::Uuid::new_v4(),
            score: 0.8,
            text: Some("Good response".to_string()),
            source: FeedbackSource::User,
            timestamp: chrono::Utc::now().timestamp(),
        };

        provider.collect_feedback(feedback.clone()).await.unwrap();

        let collected = provider.get_feedback(10).await.unwrap();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].score, 0.8);
    }

    #[tokio::test]
    async fn test_basic_compliance_provider() {
        let provider = BasicComplianceProvider::new();

        // Test PII detection
        let text = "My SSN is 123-45-6789 and email is test@example.com";
        let findings = provider.scan_pii(text).await.unwrap();

        assert!(findings.iter().any(|f| f.pii_type == PiiType::Ssn));
        assert!(findings.iter().any(|f| f.pii_type == PiiType::Email));

        // Test redaction
        let redacted = provider.redact(text);
        assert!(redacted.contains("[SSN]"));
        assert!(redacted.contains("[EMAIL]"));
        assert!(!redacted.contains("123-45-6789"));
    }

    #[tokio::test]
    async fn test_training_not_available_in_consumer() {
        let provider = BasicLearningProvider::new();

        assert!(!provider.supports_training());

        let result = provider.train().await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_extension_registry() {
        let registry = ExtensionRegistry::new();

        assert!(registry.learning.is_some());
        assert!(registry.compliance.is_some());
        assert!(!registry.has_enterprise_features());
    }
}

