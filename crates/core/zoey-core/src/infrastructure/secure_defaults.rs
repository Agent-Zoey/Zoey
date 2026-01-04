//! Secure defaults audit
//!
//! Ensures all configurations use secure defaults:
//! - TLS enabled by default
//! - Authentication required
//! - Rate limiting enabled
//! - Audit logging enabled
//! - PII detection enabled

use crate::{ZoeyError, Result};
use serde::{Deserialize, Serialize};

/// Security audit result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditResult {
    /// Overall security score (0-100)
    pub score: u32,
    /// Security level
    pub level: SecurityLevel,
    /// Passed checks
    pub passed: Vec<SecurityCheck>,
    /// Failed checks
    pub failed: Vec<SecurityCheck>,
    /// Warnings
    pub warnings: Vec<SecurityCheck>,
    /// Recommendations
    pub recommendations: Vec<String>,
    /// Audit timestamp
    pub timestamp: i64,
}

/// Security level classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityLevel {
    /// Critical - immediate action required
    Critical,
    /// Low - significant security gaps
    Low,
    /// Medium - some improvements needed
    Medium,
    /// High - good security posture
    High,
    /// Excellent - all best practices followed
    Excellent,
}

impl SecurityLevel {
    fn from_score(score: u32) -> Self {
        match score {
            0..=20 => SecurityLevel::Critical,
            21..=40 => SecurityLevel::Low,
            41..=60 => SecurityLevel::Medium,
            61..=80 => SecurityLevel::High,
            _ => SecurityLevel::Excellent,
        }
    }
}

/// Individual security check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityCheck {
    /// Check ID
    pub id: String,
    /// Check name
    pub name: String,
    /// Category
    pub category: SecurityCategory,
    /// Severity
    pub severity: SecuritySeverity,
    /// Current value
    pub current_value: String,
    /// Expected value
    pub expected_value: String,
    /// Description
    pub description: String,
    /// Remediation steps
    pub remediation: Option<String>,
}

/// Security check categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityCategory {
    /// Authentication and authorization
    Authentication,
    /// Data protection
    DataProtection,
    /// Network security
    Network,
    /// Logging and monitoring
    Logging,
    /// Input validation
    InputValidation,
    /// Access control
    AccessControl,
    /// Secrets management
    SecretsManagement,
    /// Rate limiting
    RateLimiting,
}

/// Security severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecuritySeverity {
    /// Critical - must fix immediately
    Critical,
    /// High - fix as soon as possible
    High,
    /// Medium - should be addressed
    Medium,
    /// Low - nice to have
    Low,
    /// Info - informational only
    Info,
}

/// Secure defaults configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureDefaults {
    /// TLS/HTTPS enabled
    pub tls_enabled: bool,
    /// Minimum TLS version
    pub min_tls_version: String,
    /// Authentication required
    pub auth_required: bool,
    /// Rate limiting enabled
    pub rate_limiting_enabled: bool,
    /// Audit logging enabled
    pub audit_logging_enabled: bool,
    /// PII detection enabled
    pub pii_detection_enabled: bool,
    /// Input sanitization enabled
    pub input_sanitization_enabled: bool,
    /// CORS restricted
    pub cors_restricted: bool,
    /// Secrets encrypted at rest
    pub secrets_encrypted: bool,
    /// Debug mode disabled
    pub debug_mode_disabled: bool,
    /// Secure headers enabled
    pub secure_headers_enabled: bool,
    /// Session timeout (seconds)
    pub session_timeout_seconds: u32,
    /// Maximum request size (bytes)
    pub max_request_size: usize,
    /// API versioning enabled
    pub api_versioning_enabled: bool,
}

impl Default for SecureDefaults {
    fn default() -> Self {
        Self {
            tls_enabled: true,
            min_tls_version: "1.2".to_string(),
            auth_required: true,
            rate_limiting_enabled: true,
            audit_logging_enabled: true,
            pii_detection_enabled: true,
            input_sanitization_enabled: true,
            cors_restricted: true,
            secrets_encrypted: true,
            debug_mode_disabled: true,
            secure_headers_enabled: true,
            session_timeout_seconds: 3600,
            max_request_size: 10 * 1024 * 1024, // 10MB
            api_versioning_enabled: true,
        }
    }
}

/// Security auditor
pub struct SecurityAuditor {
    defaults: SecureDefaults,
}

impl SecurityAuditor {
    /// Create a new security auditor
    pub fn new() -> Self {
        Self {
            defaults: SecureDefaults::default(),
        }
    }

    /// Create with custom defaults
    pub fn with_defaults(defaults: SecureDefaults) -> Self {
        Self { defaults }
    }

    /// Run security audit on configuration
    pub fn audit(&self, config: &RuntimeSecurityConfig) -> SecurityAuditResult {
        let mut passed = Vec::new();
        let mut failed = Vec::new();
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        // Check TLS
        let tls_check = SecurityCheck {
            id: "SEC-001".to_string(),
            name: "TLS Enabled".to_string(),
            category: SecurityCategory::Network,
            severity: SecuritySeverity::Critical,
            current_value: config.tls_enabled.to_string(),
            expected_value: "true".to_string(),
            description: "TLS/HTTPS should be enabled for all connections".to_string(),
            remediation: Some("Enable TLS in configuration: tls_enabled = true".to_string()),
        };
        if config.tls_enabled {
            passed.push(tls_check);
        } else {
            failed.push(tls_check);
            recommendations.push("Enable TLS/HTTPS for all network communications".to_string());
        }

        // Check authentication
        let auth_check = SecurityCheck {
            id: "SEC-002".to_string(),
            name: "Authentication Required".to_string(),
            category: SecurityCategory::Authentication,
            severity: SecuritySeverity::Critical,
            current_value: config.auth_required.to_string(),
            expected_value: "true".to_string(),
            description: "Authentication should be required for API access".to_string(),
            remediation: Some("Enable authentication: auth_required = true".to_string()),
        };
        if config.auth_required {
            passed.push(auth_check);
        } else {
            failed.push(auth_check);
            recommendations.push("Enable authentication for all API endpoints".to_string());
        }

        // Check rate limiting
        let rate_limit_check = SecurityCheck {
            id: "SEC-003".to_string(),
            name: "Rate Limiting Enabled".to_string(),
            category: SecurityCategory::RateLimiting,
            severity: SecuritySeverity::High,
            current_value: config.rate_limiting_enabled.to_string(),
            expected_value: "true".to_string(),
            description: "Rate limiting should be enabled to prevent abuse".to_string(),
            remediation: Some("Enable rate limiting: rate_limiting_enabled = true".to_string()),
        };
        if config.rate_limiting_enabled {
            passed.push(rate_limit_check);
        } else {
            failed.push(rate_limit_check);
            recommendations.push("Enable rate limiting to prevent API abuse".to_string());
        }

        // Check audit logging
        let audit_log_check = SecurityCheck {
            id: "SEC-004".to_string(),
            name: "Audit Logging Enabled".to_string(),
            category: SecurityCategory::Logging,
            severity: SecuritySeverity::High,
            current_value: config.audit_logging_enabled.to_string(),
            expected_value: "true".to_string(),
            description: "Audit logging should be enabled for compliance".to_string(),
            remediation: Some("Enable audit logging: audit_logging_enabled = true".to_string()),
        };
        if config.audit_logging_enabled {
            passed.push(audit_log_check);
        } else {
            failed.push(audit_log_check);
            recommendations.push("Enable audit logging for security monitoring".to_string());
        }

        // Check PII detection
        let pii_check = SecurityCheck {
            id: "SEC-005".to_string(),
            name: "PII Detection Enabled".to_string(),
            category: SecurityCategory::DataProtection,
            severity: SecuritySeverity::High,
            current_value: config.pii_detection_enabled.to_string(),
            expected_value: "true".to_string(),
            description: "PII detection should be enabled for data protection".to_string(),
            remediation: Some("Enable PII detection: pii_detection_enabled = true".to_string()),
        };
        if config.pii_detection_enabled {
            passed.push(pii_check);
        } else {
            failed.push(pii_check);
            recommendations.push("Enable PII detection to protect sensitive data".to_string());
        }

        // Check input sanitization
        let input_check = SecurityCheck {
            id: "SEC-006".to_string(),
            name: "Input Sanitization Enabled".to_string(),
            category: SecurityCategory::InputValidation,
            severity: SecuritySeverity::High,
            current_value: config.input_sanitization_enabled.to_string(),
            expected_value: "true".to_string(),
            description: "Input sanitization should be enabled to prevent injection attacks"
                .to_string(),
            remediation: Some(
                "Enable input sanitization: input_sanitization_enabled = true".to_string(),
            ),
        };
        if config.input_sanitization_enabled {
            passed.push(input_check);
        } else {
            failed.push(input_check);
            recommendations
                .push("Enable input sanitization to prevent injection attacks".to_string());
        }

        // Check debug mode
        let debug_check = SecurityCheck {
            id: "SEC-007".to_string(),
            name: "Debug Mode Disabled".to_string(),
            category: SecurityCategory::AccessControl,
            severity: SecuritySeverity::Critical,
            current_value: (!config.debug_mode).to_string(),
            expected_value: "true".to_string(),
            description: "Debug mode should be disabled in production".to_string(),
            remediation: Some("Disable debug mode: debug_mode = false".to_string()),
        };
        if !config.debug_mode {
            passed.push(debug_check);
        } else {
            failed.push(debug_check);
            recommendations.push("Disable debug mode in production environments".to_string());
        }

        // Check secrets encryption
        let secrets_check = SecurityCheck {
            id: "SEC-008".to_string(),
            name: "Secrets Encrypted".to_string(),
            category: SecurityCategory::SecretsManagement,
            severity: SecuritySeverity::Critical,
            current_value: config.secrets_encrypted.to_string(),
            expected_value: "true".to_string(),
            description: "Secrets should be encrypted at rest".to_string(),
            remediation: Some("Enable secrets encryption: secrets_encrypted = true".to_string()),
        };
        if config.secrets_encrypted {
            passed.push(secrets_check);
        } else {
            failed.push(secrets_check);
            recommendations.push("Enable encryption for secrets at rest".to_string());
        }

        // Check CORS
        let cors_check = SecurityCheck {
            id: "SEC-009".to_string(),
            name: "CORS Restricted".to_string(),
            category: SecurityCategory::Network,
            severity: SecuritySeverity::Medium,
            current_value: config.cors_restricted.to_string(),
            expected_value: "true".to_string(),
            description: "CORS should be restricted to allowed origins".to_string(),
            remediation: Some("Configure CORS to only allow trusted origins".to_string()),
        };
        if config.cors_restricted {
            passed.push(cors_check);
        } else {
            warnings.push(cors_check);
            recommendations.push("Restrict CORS to only allow trusted origins".to_string());
        }

        // Check session timeout
        let session_check = SecurityCheck {
            id: "SEC-010".to_string(),
            name: "Session Timeout Configured".to_string(),
            category: SecurityCategory::Authentication,
            severity: SecuritySeverity::Medium,
            current_value: format!("{} seconds", config.session_timeout_seconds),
            expected_value: format!("<= {} seconds", self.defaults.session_timeout_seconds),
            description: "Session timeout should be reasonably short".to_string(),
            remediation: Some(format!(
                "Set session timeout <= {} seconds",
                self.defaults.session_timeout_seconds
            )),
        };
        if config.session_timeout_seconds <= self.defaults.session_timeout_seconds {
            passed.push(session_check);
        } else {
            warnings.push(session_check);
            recommendations
                .push("Consider reducing session timeout for better security".to_string());
        }

        // Calculate score
        let total_checks = passed.len() + failed.len() + warnings.len();
        let score = if total_checks > 0 {
            let passed_score: u32 = passed.len() as u32 * 10;
            let warning_score: u32 = warnings.len() as u32 * 5;
            ((passed_score + warning_score) * 100 / (total_checks as u32 * 10)).min(100)
        } else {
            100
        };

        let level = SecurityLevel::from_score(score);

        SecurityAuditResult {
            score,
            level,
            passed,
            failed,
            warnings,
            recommendations,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Get recommended configuration
    pub fn get_recommended_config(&self) -> SecureDefaults {
        self.defaults.clone()
    }

    /// Validate configuration meets minimum security requirements
    pub fn validate_minimum_security(&self, config: &RuntimeSecurityConfig) -> Result<()> {
        let mut errors = Vec::new();

        if !config.auth_required {
            errors.push("Authentication must be required");
        }

        if config.debug_mode {
            errors.push("Debug mode must be disabled in production");
        }

        if !config.secrets_encrypted {
            errors.push("Secrets must be encrypted");
        }

        if !errors.is_empty() {
            return Err(ZoeyError::validation(format!(
                "Security requirements not met: {}",
                errors.join(", ")
            )));
        }

        Ok(())
    }
}

impl Default for SecurityAuditor {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime security configuration to audit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSecurityConfig {
    pub tls_enabled: bool,
    pub auth_required: bool,
    pub rate_limiting_enabled: bool,
    pub audit_logging_enabled: bool,
    pub pii_detection_enabled: bool,
    pub input_sanitization_enabled: bool,
    pub debug_mode: bool,
    pub secrets_encrypted: bool,
    pub cors_restricted: bool,
    pub session_timeout_seconds: u32,
}

impl Default for RuntimeSecurityConfig {
    fn default() -> Self {
        Self {
            tls_enabled: true,
            auth_required: true,
            rate_limiting_enabled: true,
            audit_logging_enabled: true,
            pii_detection_enabled: true,
            input_sanitization_enabled: true,
            debug_mode: false,
            secrets_encrypted: true,
            cors_restricted: true,
            session_timeout_seconds: 3600,
        }
    }
}

/// Helper function to generate secure configuration from environment
pub fn generate_secure_config_from_env() -> RuntimeSecurityConfig {
    RuntimeSecurityConfig {
        tls_enabled: std::env::var("TLS_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true),
        auth_required: std::env::var("AUTH_REQUIRED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true),
        rate_limiting_enabled: std::env::var("RATE_LIMITING_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true),
        audit_logging_enabled: std::env::var("AUDIT_LOGGING_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true),
        pii_detection_enabled: std::env::var("PII_DETECTION_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true),
        input_sanitization_enabled: std::env::var("INPUT_SANITIZATION_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true),
        debug_mode: std::env::var("DEBUG_MODE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false),
        secrets_encrypted: std::env::var("SECRETS_ENCRYPTED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true),
        cors_restricted: std::env::var("CORS_RESTRICTED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true),
        session_timeout_seconds: std::env::var("SESSION_TIMEOUT_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_secure_defaults() {
        let defaults = SecureDefaults::default();

        assert!(defaults.tls_enabled);
        assert!(defaults.auth_required);
        assert!(defaults.rate_limiting_enabled);
        assert!(defaults.debug_mode_disabled);
    }

    #[test]
    fn test_security_audit_passing() {
        let auditor = SecurityAuditor::new();
        let config = RuntimeSecurityConfig::default();

        let result = auditor.audit(&config);

        assert_eq!(result.level, SecurityLevel::Excellent);
        assert!(result.failed.is_empty());
    }

    #[test]
    fn test_security_audit_failing() {
        let auditor = SecurityAuditor::new();
        let config = RuntimeSecurityConfig {
            tls_enabled: false,
            auth_required: false,
            debug_mode: true,
            ..Default::default()
        };

        let result = auditor.audit(&config);

        assert!(!result.failed.is_empty());
        assert!(result.score < 100);
    }

    #[test]
    fn test_minimum_security_validation() {
        let auditor = SecurityAuditor::new();

        // Should pass with secure config
        let secure_config = RuntimeSecurityConfig::default();
        assert!(auditor.validate_minimum_security(&secure_config).is_ok());

        // Should fail with insecure config
        let insecure_config = RuntimeSecurityConfig {
            auth_required: false,
            debug_mode: true,
            secrets_encrypted: false,
            ..Default::default()
        };
        assert!(auditor.validate_minimum_security(&insecure_config).is_err());
    }

    #[test]
    fn test_security_level_from_score() {
        assert_eq!(SecurityLevel::from_score(10), SecurityLevel::Critical);
        assert_eq!(SecurityLevel::from_score(30), SecurityLevel::Low);
        assert_eq!(SecurityLevel::from_score(50), SecurityLevel::Medium);
        assert_eq!(SecurityLevel::from_score(70), SecurityLevel::High);
        assert_eq!(SecurityLevel::from_score(90), SecurityLevel::Excellent);
    }
}
