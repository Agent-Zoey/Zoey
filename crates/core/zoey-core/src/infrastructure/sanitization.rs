//! Input sanitization framework
//!
//! Provides comprehensive input validation and sanitization:
//! - XSS prevention
//! - SQL injection prevention
//! - Command injection prevention
//! - Content length limits
//! - Character encoding validation

use crate::{ZoeyError, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Sanitization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizationConfig {
    /// Maximum allowed message length
    pub max_message_length: usize,
    /// Maximum field length
    pub max_field_length: usize,
    /// Allow HTML content
    pub allow_html: bool,
    /// Allow markdown
    pub allow_markdown: bool,
    /// Blocked patterns (regex)
    pub blocked_patterns: Vec<String>,
    /// Content types allowed
    pub allowed_content_types: Vec<String>,
    /// Strip null bytes
    pub strip_null_bytes: bool,
    /// Normalize unicode
    pub normalize_unicode: bool,
}

impl Default for SanitizationConfig {
    fn default() -> Self {
        Self {
            max_message_length: 32000,
            max_field_length: 1000,
            allow_html: false,
            allow_markdown: true,
            blocked_patterns: vec![],
            allowed_content_types: vec!["text/plain".to_string(), "application/json".to_string()],
            strip_null_bytes: true,
            normalize_unicode: true,
        }
    }
}

/// Sanitization result
#[derive(Debug, Clone)]
pub struct SanitizationResult {
    /// Sanitized content
    pub content: String,
    /// Whether content was modified
    pub was_modified: bool,
    /// Modifications made
    pub modifications: Vec<SanitizationModification>,
    /// Warnings generated
    pub warnings: Vec<String>,
}

/// Type of sanitization modification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SanitizationModification {
    /// Content was truncated
    Truncated {
        original_length: usize,
        new_length: usize,
    },
    /// Characters were removed
    CharactersRemoved { count: usize, reason: String },
    /// Pattern was removed
    PatternRemoved { pattern: String },
    /// HTML was escaped
    HtmlEscaped,
    /// Null bytes removed
    NullBytesRemoved { count: usize },
    /// Unicode normalized
    UnicodeNormalized,
}

/// Input sanitizer
pub struct InputSanitizer {
    config: SanitizationConfig,
    blocked_pattern_regexes: Vec<Regex>,
}

impl InputSanitizer {
    /// Create a new input sanitizer
    pub fn new(config: SanitizationConfig) -> Result<Self> {
        let mut blocked_pattern_regexes = Vec::new();

        // Compile blocked patterns
        for pattern in &config.blocked_patterns {
            let regex = Regex::new(pattern).map_err(|e| {
                ZoeyError::validation(format!("Invalid regex pattern '{}': {}", pattern, e))
            })?;
            blocked_pattern_regexes.push(regex);
        }

        // Add default security patterns
        let default_patterns = vec![
            // SQL injection patterns
            r"(?i)(union\s+select|select\s+\*|drop\s+table|insert\s+into|delete\s+from|update\s+set)",
            // Command injection patterns
            r"[;&|`$]|\$\(|\)\s*[;&|]",
            // Path traversal
            r"\.\./|\.\.\\",
        ];

        for pattern in default_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                blocked_pattern_regexes.push(regex);
            }
        }

        Ok(Self {
            config,
            blocked_pattern_regexes,
        })
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(SanitizationConfig::default()).unwrap()
    }

    /// Sanitize input text
    pub fn sanitize(&self, input: &str) -> SanitizationResult {
        let mut content = input.to_string();
        let mut modifications = Vec::new();
        let mut warnings = Vec::new();

        // Strip null bytes
        if self.config.strip_null_bytes {
            let null_count = content.matches('\0').count();
            if null_count > 0 {
                content = content.replace('\0', "");
                modifications
                    .push(SanitizationModification::NullBytesRemoved { count: null_count });
            }
        }

        // Normalize unicode (basic normalization)
        if self.config.normalize_unicode {
            let original = content.clone();
            // Remove zero-width characters and other problematic unicode
            content = content
                .chars()
                .filter(|c| !is_problematic_unicode(*c))
                .collect();

            if content != original {
                modifications.push(SanitizationModification::UnicodeNormalized);
            }
        }

        // Check length
        if content.len() > self.config.max_message_length {
            let original_length = content.len();
            content.truncate(self.config.max_message_length);
            modifications.push(SanitizationModification::Truncated {
                original_length,
                new_length: self.config.max_message_length,
            });
            warnings.push(format!(
                "Content truncated from {} to {} characters",
                original_length, self.config.max_message_length
            ));
        }

        // Remove blocked patterns
        for regex in &self.blocked_pattern_regexes {
            if regex.is_match(&content) {
                let pattern = regex.as_str().to_string();
                content = regex.replace_all(&content, "[REDACTED]").to_string();
                modifications.push(SanitizationModification::PatternRemoved {
                    pattern: pattern.clone(),
                });
                warnings.push(format!("Blocked pattern detected and removed: {}", pattern));
            }
        }

        // Escape HTML if not allowed
        if !self.config.allow_html {
            let original = content.clone();
            content = escape_html(&content);
            if content != original {
                modifications.push(SanitizationModification::HtmlEscaped);
            }
        }

        SanitizationResult {
            was_modified: !modifications.is_empty(),
            content,
            modifications,
            warnings,
        }
    }

    /// Validate input without modifying
    pub fn validate(&self, input: &str) -> Result<()> {
        // Check length
        if input.len() > self.config.max_message_length {
            return Err(ZoeyError::validation(format!(
                "Input exceeds maximum length of {} characters",
                self.config.max_message_length
            )));
        }

        // Check for null bytes
        if self.config.strip_null_bytes && input.contains('\0') {
            return Err(ZoeyError::validation("Input contains null bytes"));
        }

        // Check for blocked patterns
        for regex in &self.blocked_pattern_regexes {
            if regex.is_match(input) {
                return Err(ZoeyError::validation(format!(
                    "Input contains blocked pattern: {}",
                    regex.as_str()
                )));
            }
        }

        // Check for HTML if not allowed
        if !self.config.allow_html && contains_html(input) {
            return Err(ZoeyError::validation("HTML content not allowed"));
        }

        Ok(())
    }

    /// Sanitize a field (shorter max length)
    pub fn sanitize_field(&self, input: &str, field_name: &str) -> SanitizationResult {
        let mut result = self.sanitize(input);

        // Apply field-specific length limit
        if result.content.len() > self.config.max_field_length {
            let original_length = result.content.len();
            result.content.truncate(self.config.max_field_length);
            result
                .modifications
                .push(SanitizationModification::Truncated {
                    original_length,
                    new_length: self.config.max_field_length,
                });
            result.warnings.push(format!(
                "Field '{}' truncated from {} to {} characters",
                field_name, original_length, self.config.max_field_length
            ));
            result.was_modified = true;
        }

        result
    }
}

impl Default for InputSanitizer {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Escape HTML special characters
fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Check if string contains HTML tags
fn contains_html(input: &str) -> bool {
    let html_pattern = Regex::new(r"<[a-zA-Z][^>]*>").unwrap();
    html_pattern.is_match(input)
}

/// Check if character is problematic unicode
fn is_problematic_unicode(c: char) -> bool {
    matches!(
        c,
        // Zero-width characters
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' |
        // Directional overrides (can be used for text spoofing)
        '\u{202A}' | '\u{202B}' | '\u{202C}' | '\u{202D}' | '\u{202E}' |
        // Other problematic characters
        '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'
    )
}

/// Sanitized string newtype for type safety
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedString(String);

impl SanitizedString {
    /// Create from already sanitized content
    pub fn new(content: String) -> Self {
        Self(content)
    }

    /// Get the sanitized content
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for SanitizedString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SanitizedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Validation rules for specific input types
pub struct ValidationRules;

impl ValidationRules {
    /// Validate email format
    pub fn validate_email(email: &str) -> Result<()> {
        let email_regex = Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap();
        if !email_regex.is_match(email) {
            return Err(ZoeyError::validation("Invalid email format"));
        }
        Ok(())
    }

    /// Validate UUID format
    pub fn validate_uuid(uuid: &str) -> Result<()> {
        if uuid::Uuid::parse_str(uuid).is_err() {
            return Err(ZoeyError::validation("Invalid UUID format"));
        }
        Ok(())
    }

    /// Validate URL format
    pub fn validate_url(url: &str) -> Result<()> {
        if url::Url::parse(url).is_err() {
            return Err(ZoeyError::validation("Invalid URL format"));
        }
        Ok(())
    }

    /// Validate alphanumeric (with optional characters)
    pub fn validate_alphanumeric(input: &str, allow_chars: &str) -> Result<()> {
        let allowed: HashSet<char> = allow_chars.chars().collect();
        for c in input.chars() {
            if !c.is_alphanumeric() && !allowed.contains(&c) {
                return Err(ZoeyError::validation(format!(
                    "Invalid character '{}' in input",
                    c
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitizer_creation() {
        let sanitizer = InputSanitizer::with_defaults();
        assert!(sanitizer.config.max_message_length > 0);
    }

    #[test]
    fn test_html_escaping() {
        let sanitizer = InputSanitizer::with_defaults();
        let result = sanitizer.sanitize("<script>alert('xss')</script>");

        assert!(result.was_modified);
        assert!(!result.content.contains('<'));
        assert!(!result.content.contains('>'));
    }

    #[test]
    fn test_sql_injection_prevention() {
        let sanitizer = InputSanitizer::with_defaults();
        let result = sanitizer.sanitize("'; DROP TABLE users; --");

        assert!(result.was_modified);
        assert!(result.content.contains("[REDACTED]") || !result.content.contains("DROP TABLE"));
    }

    #[test]
    fn test_length_truncation() {
        let config = SanitizationConfig {
            max_message_length: 10,
            ..Default::default()
        };
        let sanitizer = InputSanitizer::new(config).unwrap();
        let result = sanitizer.sanitize("This is a very long message");

        assert!(result.was_modified);
        assert_eq!(result.content.len(), 10);
    }

    #[test]
    fn test_null_byte_removal() {
        let sanitizer = InputSanitizer::with_defaults();
        let result = sanitizer.sanitize("Hello\0World");

        assert!(result.was_modified);
        assert!(!result.content.contains('\0'));
        assert_eq!(result.content, "HelloWorld");
    }

    #[test]
    fn test_validation() {
        let sanitizer = InputSanitizer::with_defaults();

        // Valid input
        assert!(sanitizer.validate("Hello, world!").is_ok());

        // Invalid: too long
        let config = SanitizationConfig {
            max_message_length: 5,
            ..Default::default()
        };
        let sanitizer = InputSanitizer::new(config).unwrap();
        assert!(sanitizer.validate("Too long").is_err());
    }

    #[test]
    fn test_email_validation() {
        assert!(ValidationRules::validate_email("test@example.com").is_ok());
        assert!(ValidationRules::validate_email("invalid").is_err());
    }

    #[test]
    fn test_uuid_validation() {
        assert!(ValidationRules::validate_uuid("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(ValidationRules::validate_uuid("not-a-uuid").is_err());
    }

    #[test]
    fn test_problematic_unicode_removal() {
        let sanitizer = InputSanitizer::with_defaults();
        let result = sanitizer.sanitize("Hello\u{200B}World"); // Zero-width space

        assert!(result.was_modified);
        assert_eq!(result.content, "HelloWorld");
    }
}
