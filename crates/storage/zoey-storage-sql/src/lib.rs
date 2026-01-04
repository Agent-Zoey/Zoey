//! ZoeyOS SQL Plugin
//!
//! Database adapters for PostgreSQL and SQLite with HIPAA-ready patterns (NOT certified).

#![warn(missing_docs)]
#![warn(clippy::all)]

// Re-exports
pub use zoey_core;

pub mod hipaa;
pub mod postgres;
pub mod sqlite;
pub mod vector_search;

// Re-export adapters
pub use hipaa::{AuditLogEntry, HIPAACompliance, HIPAAConfig};
pub use postgres::PostgresAdapter;
pub use sqlite::SqliteAdapter;
pub use vector_search::VectorSearch;
