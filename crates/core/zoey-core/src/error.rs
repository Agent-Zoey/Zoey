//! Error types for ZoeyOS core

use thiserror::Error;

/// Main error type for ZoeyOS operations
#[derive(Debug, Error)]
pub enum ZoeyError {
    /// Database operation error (from sqlx)
    #[error("Database error: {0}")]
    DatabaseSqlx(#[from] sqlx::Error),

    /// Database operation error (custom message)
    #[error("Database error: {0}")]
    Database(String),

    /// Plugin-related error
    #[error("Plugin error: {0}")]
    Plugin(String),

    /// Runtime error
    #[error("Runtime error: {0}")]
    Runtime(String),

    /// Model/LLM error
    #[error("Model error: {0}")]
    Model(String),

    /// Memory operation error
    #[error("Memory error: {0}")]
    Memory(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Network/HTTP error
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Template rendering error
    #[error("Template error: {0}")]
    Template(String),

    /// Service error
    #[error("Service error: {0}")]
    Service(String),

    /// Event handling error
    #[error("Event error: {0}")]
    Event(String),

    /// Action execution error
    #[error("Action error: {0}")]
    Action(String),

    /// Provider error
    #[error("Provider error: {0}")]
    Provider(String),

    /// Evaluator error
    #[error("Evaluator error: {0}")]
    Evaluator(String),

    /// Not found error (generic)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Authentication/authorization error
    #[error("Auth error: {0}")]
    Auth(String),

    /// Rate limit error
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Timeout error
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Generic error with context
    #[error("{0}")]
    Other(String),

    /// Database constraint violation with details
    #[error(
        "Database constraint violation in table '{table}': {constraint} = '{value}'. {suggestion}"
    )]
    DatabaseConstraintViolation {
        /// Database table name
        table: String,
        /// Constraint name
        constraint: String,
        /// Attempted value
        value: String,
        /// Suggestion for fixing
        suggestion: String,
    },

    /// Vector search error
    #[error(
        "Vector search error: {message}. Dimension: {dimension}, Expected: {expected_dimension}"
    )]
    VectorSearch {
        /// Error message
        message: String,
        /// Actual embedding dimension
        dimension: usize,
        /// Expected embedding dimension
        expected_dimension: usize,
    },

    /// Missing required field
    #[error("Missing required field '{field}' in {context}. {suggestion}")]
    MissingField {
        /// Field name
        field: String,
        /// Context where field is missing
        context: String,
        /// Suggestion for fixing
        suggestion: String,
    },

    /// Resource exhausted
    #[error("Resource '{resource}' exhausted: {message}. Current: {current}, Limit: {limit}")]
    ResourceExhausted {
        /// Resource name
        resource: String,
        /// Error message
        message: String,
        /// Current usage
        current: usize,
        /// Maximum limit
        limit: usize,
    },
}

/// Convenient Result type using ZoeyError
pub type Result<T> = std::result::Result<T, ZoeyError>;

impl ZoeyError {
    /// Create a database error
    pub fn database(msg: impl Into<String>) -> Self {
        ZoeyError::Database(msg.into())
    }

    /// Create a plugin error
    pub fn plugin(msg: impl Into<String>) -> Self {
        ZoeyError::Plugin(msg.into())
    }

    /// Create a runtime error
    pub fn runtime(msg: impl Into<String>) -> Self {
        ZoeyError::Runtime(msg.into())
    }

    /// Create a model error
    pub fn model(msg: impl Into<String>) -> Self {
        ZoeyError::Model(msg.into())
    }

    /// Create a memory error
    pub fn memory(msg: impl Into<String>) -> Self {
        ZoeyError::Memory(msg.into())
    }

    /// Create a config error
    pub fn config(msg: impl Into<String>) -> Self {
        ZoeyError::Config(msg.into())
    }

    /// Create a validation error
    pub fn validation(msg: impl Into<String>) -> Self {
        ZoeyError::Validation(msg.into())
    }

    /// Create a service error
    pub fn service(msg: impl Into<String>) -> Self {
        ZoeyError::Service(msg.into())
    }

    /// Create an event error
    pub fn event(msg: impl Into<String>) -> Self {
        ZoeyError::Event(msg.into())
    }

    /// Create an action error
    pub fn action(msg: impl Into<String>) -> Self {
        ZoeyError::Action(msg.into())
    }

    /// Create a provider error
    pub fn provider(msg: impl Into<String>) -> Self {
        ZoeyError::Provider(msg.into())
    }

    /// Create an evaluator error
    pub fn evaluator(msg: impl Into<String>) -> Self {
        ZoeyError::Evaluator(msg.into())
    }

    /// Create a not found error
    pub fn not_found(msg: impl Into<String>) -> Self {
        ZoeyError::NotFound(msg.into())
    }

    /// Create an auth error
    pub fn auth(msg: impl Into<String>) -> Self {
        ZoeyError::Auth(msg.into())
    }

    /// Create a rate limit error
    pub fn rate_limit(msg: impl Into<String>) -> Self {
        ZoeyError::RateLimit(msg.into())
    }

    /// Create a timeout error
    pub fn timeout(msg: impl Into<String>) -> Self {
        ZoeyError::Timeout(msg.into())
    }

    /// Create a generic error
    pub fn other(msg: impl Into<String>) -> Self {
        ZoeyError::Other(msg.into())
    }

    /// Create a template error
    pub fn template(msg: impl Into<String>) -> Self {
        ZoeyError::Template(msg.into())
    }

    /// Create a constraint violation error
    pub fn constraint_violation(
        table: impl Into<String>,
        constraint: impl Into<String>,
        value: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        ZoeyError::DatabaseConstraintViolation {
            table: table.into(),
            constraint: constraint.into(),
            value: value.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Create a vector search error
    pub fn vector_search(
        message: impl Into<String>,
        dimension: usize,
        expected_dimension: usize,
    ) -> Self {
        ZoeyError::VectorSearch {
            message: message.into(),
            dimension,
            expected_dimension,
        }
    }

    /// Create a missing field error
    pub fn missing_field(
        field: impl Into<String>,
        context: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        ZoeyError::MissingField {
            field: field.into(),
            context: context.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Create a resource exhausted error
    pub fn resource_exhausted(
        resource: impl Into<String>,
        message: impl Into<String>,
        current: usize,
        limit: usize,
    ) -> Self {
        ZoeyError::ResourceExhausted {
            resource: resource.into(),
            message: message.into(),
            current,
            limit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = ZoeyError::plugin("test plugin error");
        assert_eq!(err.to_string(), "Plugin error: test plugin error");

        let err = ZoeyError::runtime("test runtime error");
        assert_eq!(err.to_string(), "Runtime error: test runtime error");
    }

    #[test]
    fn test_result_type() {
        fn returns_result() -> Result<i32> {
            Ok(42)
        }

        assert_eq!(returns_result().unwrap(), 42);
    }
}
