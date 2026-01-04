//! HIPAA Compliance Features for Database
//!
//! Provides database-level protection for PII including:
//! - Pre-storage PII detection and redaction
//! - Dual storage (encrypted original + redacted version)
//! - Access control with attorney-client privilege support
//! - Comprehensive audit logging
//! - HIPAA-compliant encryption at rest

use zoey_core::{
    security::{decrypt_secret, encrypt_secret},
    ZoeyError, Result,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;

/// HIPAA compliance configuration
#[derive(Debug, Clone)]
pub struct HIPAAConfig {
    /// Enable HIPAA features (master switch)
    pub enabled: bool,

    /// Enable audit logging
    pub audit_logging: bool,

    /// Enable encryption at rest
    pub encryption_at_rest: bool,

    /// Enable access control
    pub access_control: bool,

    /// Retention policy in days
    pub retention_days: usize,

    /// Enable automatic de-identification
    pub auto_deidentify: bool,

    /// Encryption key for PHI data (MUST be set for encryption_at_rest)
    /// This should be loaded from a secure key management system, NOT hardcoded
    encryption_key: Option<String>,
}

impl Default for HIPAAConfig {
    fn default() -> Self {
        Self {
            enabled: true, // Can be disabled
            audit_logging: true,
            encryption_at_rest: true,
            access_control: true,
            retention_days: 2555, // 7 years as per HIPAA
            auto_deidentify: true,
            encryption_key: None, // MUST be set via with_encryption_key() for production
        }
    }
}

impl HIPAAConfig {
    /// Create config with HIPAA disabled (for non-healthcare use)
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            audit_logging: false,
            encryption_at_rest: false,
            access_control: false,
            retention_days: 365, // 1 year default
            auto_deidentify: false,
            encryption_key: None,
        }
    }

    /// Create config with minimal features (audit only)
    pub fn minimal() -> Self {
        Self {
            enabled: true,
            audit_logging: true, // Keep audit trail
            encryption_at_rest: false,
            access_control: false,
            retention_days: 365,
            auto_deidentify: false,
            encryption_key: None,
        }
    }

    /// Create config with maximum compliance
    pub fn maximum() -> Self {
        Self::default()
    }

    /// Set the encryption key for PHI data
    ///
    /// SECURITY: The key should be:
    /// - At least 32 characters long
    /// - Loaded from a secure key management system (e.g., AWS KMS, HashiCorp Vault)
    /// - NEVER hardcoded in source code
    /// - Rotated periodically
    pub fn with_encryption_key(mut self, key: impl Into<String>) -> Self {
        let key = key.into();
        if key.len() < 32 {
            tracing::warn!(
                "HIPAA encryption key is less than 32 characters - this may be insecure"
            );
        }
        self.encryption_key = Some(key);
        self
    }

    /// Check if encryption is properly configured
    pub fn is_encryption_ready(&self) -> bool {
        self.encryption_at_rest && self.encryption_key.is_some()
    }
}

/// HIPAA audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// Unique identifier
    pub id: uuid::Uuid,

    /// Timestamp
    pub timestamp: i64,

    /// User/entity who performed the action
    pub actor_id: uuid::Uuid,

    /// Action performed
    pub action: String,

    /// Resource accessed
    pub resource_type: String,

    /// Resource ID
    pub resource_id: uuid::Uuid,

    /// IP address
    pub ip_address: Option<String>,

    /// Result (success/failure)
    pub result: String,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// HIPAA compliance manager
pub struct HIPAACompliance {
    pool: PgPool,
    config: HIPAAConfig,
}

impl HIPAACompliance {
    /// Create a new HIPAA compliance manager
    pub fn new(pool: PgPool, config: HIPAAConfig) -> Self {
        Self { pool, config }
    }

    /// Initialize HIPAA compliance tables
    pub async fn initialize(&self) -> Result<()> {
        if !self.config.enabled {
            tracing::info!("HIPAA compliance is DISABLED - skipping initialization");
            return Ok(());
        }

        tracing::info!("Initializing HIPAA compliance features");

        // Create audit log table
        if self.config.audit_logging {
            sqlx::query(
                r#"
            CREATE TABLE IF NOT EXISTS audit_log (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                timestamp BIGINT NOT NULL,
                actor_id UUID NOT NULL,
                action TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id UUID NOT NULL,
                ip_address TEXT,
                result TEXT NOT NULL,
                metadata JSONB DEFAULT '{}'
            )
        "#,
            )
            .execute(&self.pool)
            .await?;

            // Create index for audit log queries
            sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_audit_log_timestamp ON audit_log(timestamp)",
            )
            .execute(&self.pool)
            .await?;

            sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_log_actor ON audit_log(actor_id)")
                .execute(&self.pool)
                .await?;
        }

        // Enable Row Level Security (RLS)
        if self.config.access_control {
            sqlx::query("ALTER TABLE memories ENABLE ROW LEVEL SECURITY")
                .execute(&self.pool)
                .await
                .ok(); // May fail if already enabled

            // Create policies for HIPAA compliance
            sqlx::query(
                r#"
                CREATE POLICY IF NOT EXISTS hipaa_agent_isolation ON memories
                FOR ALL
                USING (agent_id = current_setting('app.current_agent_id')::uuid)
            "#,
            )
            .execute(&self.pool)
            .await
            .ok();
        }

        // Create encrypted column for sensitive data
        if self.config.encryption_at_rest {
            // Would use pgcrypto extension
            sqlx::query("CREATE EXTENSION IF NOT EXISTS pgcrypto")
                .execute(&self.pool)
                .await
                .ok();
        }

        tracing::info!("HIPAA compliance initialized successfully");
        Ok(())
    }

    /// Log an audit entry
    pub async fn log_audit(&self, entry: AuditLogEntry) -> Result<()> {
        if !self.config.enabled || !self.config.audit_logging {
            return Ok(());
        }

        sqlx::query(
            "INSERT INTO audit_log (id, timestamp, actor_id, action, resource_type, resource_id, ip_address, result, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(entry.id)
        .bind(entry.timestamp)
        .bind(entry.actor_id)
        .bind(&entry.action)
        .bind(&entry.resource_type)
        .bind(entry.resource_id)
        .bind(&entry.ip_address)
        .bind(&entry.result)
        .bind(serde_json::to_value(&entry.metadata)?)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Purge old records per retention policy
    pub async fn enforce_retention_policy(&self) -> Result<usize> {
        let cutoff_timestamp =
            chrono::Utc::now().timestamp() - (self.config.retention_days as i64 * 86400);

        let result = sqlx::query(
            "DELETE FROM memories WHERE created_at < $1 AND metadata->>'retention' != 'permanent'",
        )
        .bind(cutoff_timestamp)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            "Retention policy enforced: {} records purged",
            result.rows_affected()
        );
        Ok(result.rows_affected() as usize)
    }

    /// Get audit log entries
    pub async fn get_audit_log(
        &self,
        actor_id: Option<uuid::Uuid>,
        limit: usize,
    ) -> Result<Vec<AuditLogEntry>> {
        let query = if let Some(aid) = actor_id {
            sqlx::query_as::<_, (uuid::Uuid, i64, uuid::Uuid, String, String, uuid::Uuid, Option<String>, String, serde_json::Value)>(
                "SELECT id, timestamp, actor_id, action, resource_type, resource_id, ip_address, result, metadata 
                 FROM audit_log WHERE actor_id = $1 ORDER BY timestamp DESC LIMIT $2"
            )
            .bind(aid)
            .bind(limit as i64)
        } else {
            sqlx::query_as::<_, (uuid::Uuid, i64, uuid::Uuid, String, String, uuid::Uuid, Option<String>, String, serde_json::Value)>(
                "SELECT id, timestamp, actor_id, action, resource_type, resource_id, ip_address, result, metadata 
                 FROM audit_log ORDER BY timestamp DESC LIMIT $1"
            )
            .bind(limit as i64)
        };

        let rows = query.fetch_all(&self.pool).await?;

        let entries = rows
            .into_iter()
            .map(
                |(
                    id,
                    timestamp,
                    actor_id,
                    action,
                    resource_type,
                    resource_id,
                    ip_address,
                    result,
                    metadata,
                )| {
                    AuditLogEntry {
                        id,
                        timestamp,
                        actor_id,
                        action,
                        resource_type,
                        resource_id,
                        ip_address,
                        result,
                        metadata: serde_json::from_value(metadata).unwrap_or_default(),
                    }
                },
            )
            .collect();

        Ok(entries)
    }

    /// Encrypt Protected Health Information (PHI) using AES-256-GCM
    ///
    /// This provides HIPAA-compliant encryption at rest for sensitive medical data.
    /// The encryption uses:
    /// - AES-256-GCM for authenticated encryption
    /// - Argon2 for key derivation
    /// - Random salt and nonce per encryption
    ///
    /// # Errors
    /// Returns an error if encryption is not properly configured (missing key)
    pub fn encrypt_phi(&self, data: &str) -> Result<String> {
        if !self.config.encryption_at_rest {
            // Encryption disabled - return plaintext (for non-HIPAA deployments)
            tracing::debug!("PHI encryption disabled, storing plaintext");
            return Ok(data.to_string());
        }

        let key = self.config.encryption_key.as_ref().ok_or_else(|| {
            tracing::error!("CRITICAL: PHI encryption requested but no encryption key configured!");
            ZoeyError::other(
                "HIPAA encryption key not configured. Set encryption key via HIPAAConfig::with_encryption_key() \
                 before encrypting PHI data. This is a HIPAA compliance violation."
            )
        })?;

        encrypt_secret(data, key)
    }

    /// Decrypt Protected Health Information (PHI)
    ///
    /// Decrypts data that was encrypted with `encrypt_phi`.
    ///
    /// # Errors
    /// - Returns error if decryption key is not configured
    /// - Returns error if data was tampered with (authentication failure)
    /// - Returns error if wrong key is used
    pub fn decrypt_phi(&self, encrypted: &str) -> Result<String> {
        if !self.config.encryption_at_rest {
            // Encryption disabled - data is plaintext
            return Ok(encrypted.to_string());
        }

        // Handle legacy "ENCRYPTED:" prefix format (migration path)
        if encrypted.starts_with("ENCRYPTED:") {
            tracing::warn!(
                "Found legacy placeholder encryption format - this data was NOT actually encrypted! \
                 Re-encrypt this data immediately for HIPAA compliance."
            );
            // Return the "encrypted" data for migration purposes, but log a warning
            return Ok(encrypted
                .strip_prefix("ENCRYPTED:")
                .unwrap_or(encrypted)
                .to_string());
        }

        let key = self.config.encryption_key.as_ref().ok_or_else(|| {
            ZoeyError::other("HIPAA encryption key not configured for decryption")
        })?;

        decrypt_secret(encrypted, key)
    }

    /// Get the configuration
    pub fn config(&self) -> &HIPAAConfig {
        &self.config
    }
}

// ============================================================================
// PII Protection Hooks
// ============================================================================

/// Access level for protected data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessLevel {
    /// Full access - can see original PII
    Full,
    /// Redacted access - can only see redacted version
    Redacted,
    /// Metadata only - can see existence but not content
    MetadataOnly,
    /// No access
    None,
}

/// Document privilege status for access control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentPrivilege {
    /// Attorney-client privileged
    AttorneyClient,
    /// Work product doctrine
    WorkProduct,
    /// Both attorney-client and work product
    Dual,
    /// Not privileged
    None,
}

impl std::fmt::Display for DocumentPrivilege {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocumentPrivilege::AttorneyClient => write!(f, "Attorney-Client Privileged"),
            DocumentPrivilege::WorkProduct => write!(f, "Work Product"),
            DocumentPrivilege::Dual => write!(f, "Privileged (A/C + WP)"),
            DocumentPrivilege::None => write!(f, "Not Privileged"),
        }
    }
}

/// PII detection result for storage decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PIIDetectionResult {
    /// Whether PII was detected
    pub contains_pii: bool,
    /// Number of PII instances found
    pub pii_count: usize,
    /// Types of PII found
    pub pii_types: Vec<String>,
    /// Whether critical PII (SSN, credit card, etc.) was found
    pub has_critical: bool,
    /// Suggested access level
    pub suggested_access: AccessLevel,
    /// Detected document privilege (for legal documents)
    pub privilege_status: DocumentPrivilege,
}

impl Default for PIIDetectionResult {
    fn default() -> Self {
        Self {
            contains_pii: false,
            pii_count: 0,
            pii_types: Vec::new(),
            has_critical: false,
            suggested_access: AccessLevel::Full,
            privilege_status: DocumentPrivilege::None,
        }
    }
}

/// Protected data record for dual storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedDataRecord {
    /// Unique identifier
    pub id: uuid::Uuid,
    /// Original data (encrypted)
    pub encrypted_original: String,
    /// Redacted version (safe to display)
    pub redacted_version: String,
    /// PII detection metadata
    pub pii_metadata: PIIDetectionResult,
    /// Entity ID (owner)
    pub entity_id: uuid::Uuid,
    /// Agent ID
    pub agent_id: uuid::Uuid,
    /// Room/conversation ID
    pub room_id: uuid::Uuid,
    /// Creation timestamp
    pub created_at: i64,
    /// Last accessed timestamp
    pub last_accessed: Option<i64>,
    /// Access count
    pub access_count: usize,
    /// Document type (for legal classification)
    pub document_type: Option<String>,
    /// Case reference (for legal documents)
    pub case_reference: Option<String>,
}

/// PII Storage Hook for pre-storage processing
pub struct PIIStorageHook {
    config: PIIStorageConfig,
}

/// Configuration for PII storage hooks
#[derive(Debug, Clone)]
pub struct PIIStorageConfig {
    /// Block storage if critical PII is detected (fail-safe mode)
    pub block_on_critical: bool,
    /// Always store both encrypted and redacted versions
    pub dual_storage: bool,
    /// Automatically detect attorney-client privilege
    pub detect_privilege: bool,
    /// Log all PII detections for audit
    pub audit_detections: bool,
    /// Redaction placeholder format
    pub redaction_format: String,
}

impl Default for PIIStorageConfig {
    fn default() -> Self {
        Self {
            block_on_critical: false, // Don't block by default
            dual_storage: true,
            detect_privilege: true,
            audit_detections: true,
            redaction_format: "[REDACTED-{}]".to_string(),
        }
    }
}

impl PIIStorageConfig {
    /// Create config for law office use
    pub fn for_legal_office() -> Self {
        Self {
            block_on_critical: false, // Law offices need to store client data
            dual_storage: true,
            detect_privilege: true, // Important for legal
            audit_detections: true,
            redaction_format: "[REDACTED-{}]".to_string(),
        }
    }

    /// Create config for healthcare/HIPAA use
    pub fn for_hipaa() -> Self {
        Self {
            block_on_critical: true, // Block unintended PHI storage
            dual_storage: true,
            detect_privilege: false,
            audit_detections: true,
            redaction_format: "[PHI-REDACTED]".to_string(),
        }
    }

    /// Create a strict config that blocks on any PII
    pub fn strict() -> Self {
        Self {
            block_on_critical: true,
            dual_storage: true,
            detect_privilege: true,
            audit_detections: true,
            redaction_format: "[BLOCKED-PII]".to_string(),
        }
    }
}

impl PIIStorageHook {
    /// Create a new PII storage hook
    pub fn new(config: PIIStorageConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(PIIStorageConfig::default())
    }

    /// Create for law office use
    pub fn for_legal_office() -> Self {
        Self::new(PIIStorageConfig::for_legal_office())
    }

    /// Create for HIPAA compliance
    pub fn for_hipaa() -> Self {
        Self::new(PIIStorageConfig::for_hipaa())
    }

    /// Pre-storage hook: Analyze text before storage
    ///
    /// This performs PII detection and returns metadata about the content.
    /// The caller can use this to decide how to store the data.
    pub fn analyze_for_storage(&self, text: &str) -> PIIDetectionResult {
        let mut result = PIIDetectionResult::default();

        // Basic PII pattern detection (simplified - in production would use HybridPIIDetector)
        let patterns = [
            ("SSN", r"\b\d{3}-\d{2}-\d{4}\b"),
            ("EMAIL", r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"),
            ("PHONE", r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b"),
            ("CREDITCARD", r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b"),
            ("APIKEY", r"\b(sk-|pk-|api[_-]?key)[A-Za-z0-9_-]+"),
        ];

        let critical_types = ["SSN", "CREDITCARD", "APIKEY"];

        for (pii_type, pattern) in &patterns {
            if let Ok(regex) = regex::Regex::new(pattern) {
                let matches: Vec<_> = regex.find_iter(text).collect();
                if !matches.is_empty() {
                    result.contains_pii = true;
                    result.pii_count += matches.len();
                    result.pii_types.push(pii_type.to_string());

                    if critical_types.contains(pii_type) {
                        result.has_critical = true;
                    }
                }
            }
        }

        // Detect privilege status if enabled
        if self.config.detect_privilege {
            result.privilege_status = self.detect_privilege(text);
        }

        // Determine suggested access level
        result.suggested_access = if result.has_critical {
            AccessLevel::Redacted
        } else if result.contains_pii {
            AccessLevel::Redacted
        } else if result.privilege_status != DocumentPrivilege::None {
            AccessLevel::Redacted // Privileged documents should be protected
        } else {
            AccessLevel::Full
        };

        result
    }

    /// Detect attorney-client privilege status
    fn detect_privilege(&self, text: &str) -> DocumentPrivilege {
        let text_lower = text.to_lowercase();

        let ac_keywords = [
            "privileged",
            "attorney-client",
            "attorney client",
            "confidential communication",
            "legal advice",
            "privileged and confidential",
        ];

        let wp_keywords = [
            "work product",
            "attorney work product",
            "prepared in anticipation of litigation",
            "trial preparation",
            "litigation strategy",
        ];

        let has_ac = ac_keywords.iter().any(|kw| text_lower.contains(kw));
        let has_wp = wp_keywords.iter().any(|kw| text_lower.contains(kw));

        match (has_ac, has_wp) {
            (true, true) => DocumentPrivilege::Dual,
            (true, false) => DocumentPrivilege::AttorneyClient,
            (false, true) => DocumentPrivilege::WorkProduct,
            (false, false) => DocumentPrivilege::None,
        }
    }

    /// Redact PII from text using configured format
    pub fn redact(&self, text: &str) -> String {
        let mut redacted = text.to_string();

        let patterns = [
            ("SSN", r"\b\d{3}-\d{2}-\d{4}\b"),
            ("EMAIL", r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"),
            ("PHONE", r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b"),
            ("CREDITCARD", r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b"),
            ("APIKEY", r"\b(sk-|pk-|api[_-]?key)[A-Za-z0-9_-]+"),
        ];

        for (pii_type, pattern) in &patterns {
            if let Ok(regex) = regex::Regex::new(pattern) {
                let replacement = self.config.redaction_format.replace("{}", pii_type);
                redacted = regex.replace_all(&redacted, replacement.as_str()).to_string();
            }
        }

        redacted
    }

    /// Prepare data for dual storage
    ///
    /// Returns (encrypted_original, redacted_version)
    pub fn prepare_for_storage(
        &self,
        text: &str,
        encryption_key: Option<&str>,
    ) -> Result<(String, String, PIIDetectionResult)> {
        let detection = self.analyze_for_storage(text);

        // Check if we should block storage
        if self.config.block_on_critical && detection.has_critical {
            return Err(ZoeyError::validation(
                "Storage blocked: Critical PII detected. Please review and redact before storing.",
            ));
        }

        // Prepare encrypted original
        let encrypted = if let Some(key) = encryption_key {
            encrypt_secret(text, key)?
        } else {
            // No encryption key - store plaintext (not recommended for production)
            text.to_string()
        };

        // Prepare redacted version
        let redacted = self.redact(text);

        Ok((encrypted, redacted, detection))
    }

    /// Check if storage should be allowed based on detection result
    pub fn should_allow_storage(&self, detection: &PIIDetectionResult) -> bool {
        if self.config.block_on_critical && detection.has_critical {
            return false;
        }
        true
    }

    /// Get the access level for a user based on their role
    pub fn get_access_level(&self, user_role: &str, detection: &PIIDetectionResult) -> AccessLevel {
        match user_role.to_lowercase().as_str() {
            "admin" | "attorney" | "partner" => {
                // Full access for attorneys and partners
                AccessLevel::Full
            }
            "paralegal" | "legal_assistant" => {
                // Paralegals can see redacted for critical PII
                if detection.has_critical {
                    AccessLevel::Redacted
                } else {
                    AccessLevel::Full
                }
            }
            "billing" | "accounting" => {
                // Billing can see metadata and non-critical PII
                if detection.has_critical {
                    AccessLevel::MetadataOnly
                } else {
                    AccessLevel::Redacted
                }
            }
            "staff" | "receptionist" => {
                // General staff gets redacted view
                AccessLevel::Redacted
            }
            "external" | "client" => {
                // External users get metadata only for PII documents
                if detection.contains_pii {
                    AccessLevel::MetadataOnly
                } else {
                    AccessLevel::Full
                }
            }
            _ => AccessLevel::Redacted, // Default to redacted
        }
    }
}

impl Default for PIIStorageHook {
    fn default() -> Self {
        Self::default_config()
    }
}

/// Extension trait for HIPAACompliance to add PII storage hooks
impl HIPAACompliance {
    /// Create a protected data record with PII detection
    pub fn create_protected_record(
        &self,
        text: &str,
        entity_id: uuid::Uuid,
        agent_id: uuid::Uuid,
        room_id: uuid::Uuid,
        storage_hook: &PIIStorageHook,
    ) -> Result<ProtectedDataRecord> {
        let (encrypted, redacted, detection) = storage_hook.prepare_for_storage(
            text,
            self.config.encryption_key.as_deref(),
        )?;

        Ok(ProtectedDataRecord {
            id: uuid::Uuid::new_v4(),
            encrypted_original: encrypted,
            redacted_version: redacted,
            pii_metadata: detection,
            entity_id,
            agent_id,
            room_id,
            created_at: chrono::Utc::now().timestamp(),
            last_accessed: None,
            access_count: 0,
            document_type: None,
            case_reference: None,
        })
    }

    /// Initialize protected data storage table
    pub async fn initialize_protected_storage(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS protected_data (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                encrypted_original TEXT NOT NULL,
                redacted_version TEXT NOT NULL,
                pii_metadata JSONB DEFAULT '{}',
                entity_id UUID NOT NULL,
                agent_id UUID NOT NULL,
                room_id UUID NOT NULL,
                created_at BIGINT NOT NULL,
                last_accessed BIGINT,
                access_count INTEGER DEFAULT 0,
                document_type TEXT,
                case_reference TEXT,
                privilege_status TEXT DEFAULT 'none'
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_protected_data_entity ON protected_data(entity_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_protected_data_agent ON protected_data(agent_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_protected_data_case ON protected_data(case_reference)")
            .execute(&self.pool)
            .await?;

        // Enable RLS for protected data
        if self.config.access_control {
            sqlx::query("ALTER TABLE protected_data ENABLE ROW LEVEL SECURITY")
                .execute(&self.pool)
                .await
                .ok();
        }

        tracing::info!("Protected data storage initialized");
        Ok(())
    }

    /// Store a protected data record
    pub async fn store_protected_data(&self, record: &ProtectedDataRecord) -> Result<uuid::Uuid> {
        let pii_metadata_json = serde_json::to_value(&record.pii_metadata)?;
        let privilege_str = format!("{:?}", record.pii_metadata.privilege_status);

        sqlx::query(
            r#"
            INSERT INTO protected_data 
            (id, encrypted_original, redacted_version, pii_metadata, entity_id, agent_id, room_id, 
             created_at, last_accessed, access_count, document_type, case_reference, privilege_status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(record.id)
        .bind(&record.encrypted_original)
        .bind(&record.redacted_version)
        .bind(pii_metadata_json)
        .bind(record.entity_id)
        .bind(record.agent_id)
        .bind(record.room_id)
        .bind(record.created_at)
        .bind(record.last_accessed)
        .bind(record.access_count as i32)
        .bind(&record.document_type)
        .bind(&record.case_reference)
        .bind(privilege_str)
        .execute(&self.pool)
        .await?;

        // Log audit entry
        if self.config.audit_logging {
            let mut metadata = HashMap::new();
            metadata.insert("contains_pii".to_string(), record.pii_metadata.contains_pii.to_string());
            metadata.insert("pii_count".to_string(), record.pii_metadata.pii_count.to_string());
            if !record.pii_metadata.pii_types.is_empty() {
                metadata.insert("pii_types".to_string(), record.pii_metadata.pii_types.join(","));
            }

            self.log_audit(AuditLogEntry {
                id: uuid::Uuid::new_v4(),
                timestamp: chrono::Utc::now().timestamp(),
                actor_id: record.agent_id,
                action: "STORE_PROTECTED_DATA".to_string(),
                resource_type: "ProtectedData".to_string(),
                resource_id: record.id,
                ip_address: None,
                result: "SUCCESS".to_string(),
                metadata,
            })
            .await?;
        }

        Ok(record.id)
    }

    /// Retrieve protected data with access control
    pub async fn get_protected_data(
        &self,
        id: uuid::Uuid,
        accessor_id: uuid::Uuid,
        access_level: AccessLevel,
    ) -> Result<Option<(String, PIIDetectionResult)>> {
        let row = sqlx::query_as::<_, (String, String, serde_json::Value)>(
            "SELECT encrypted_original, redacted_version, pii_metadata FROM protected_data WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((encrypted, redacted, pii_json)) = row {
            // Update access tracking
            sqlx::query(
                "UPDATE protected_data SET last_accessed = $1, access_count = access_count + 1 WHERE id = $2",
            )
            .bind(chrono::Utc::now().timestamp())
            .bind(id)
            .execute(&self.pool)
            .await?;

            // Log audit
            if self.config.audit_logging {
                let mut metadata = HashMap::new();
                metadata.insert("access_level".to_string(), format!("{:?}", access_level));

                self.log_audit(AuditLogEntry {
                    id: uuid::Uuid::new_v4(),
                    timestamp: chrono::Utc::now().timestamp(),
                    actor_id: accessor_id,
                    action: "ACCESS_PROTECTED_DATA".to_string(),
                    resource_type: "ProtectedData".to_string(),
                    resource_id: id,
                    ip_address: None,
                    result: "SUCCESS".to_string(),
                    metadata,
                })
                .await?;
            }

            let pii_metadata: PIIDetectionResult = serde_json::from_value(pii_json)?;

            // Return data based on access level
            let content = match access_level {
                AccessLevel::Full => {
                    // Decrypt and return original
                    self.decrypt_phi(&encrypted)?
                }
                AccessLevel::Redacted => redacted,
                AccessLevel::MetadataOnly => {
                    format!("[Content protected - {} PII instances detected]", pii_metadata.pii_count)
                }
                AccessLevel::None => {
                    return Err(ZoeyError::auth("Access denied to protected data"));
                }
            };

            Ok(Some((content, pii_metadata)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hipaa_config() {
        let config = HIPAAConfig::default();
        assert!(config.audit_logging);
        assert!(config.encryption_at_rest);
        assert_eq!(config.retention_days, 2555); // 7 years
    }

    #[test]
    fn test_audit_log_entry() {
        let entry = AuditLogEntry {
            id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now().timestamp(),
            actor_id: uuid::Uuid::new_v4(),
            action: "READ".to_string(),
            resource_type: "Memory".to_string(),
            resource_id: uuid::Uuid::new_v4(),
            ip_address: Some("127.0.0.1".to_string()),
            result: "SUCCESS".to_string(),
            metadata: HashMap::new(),
        };

        assert_eq!(entry.action, "READ");
    }
}
