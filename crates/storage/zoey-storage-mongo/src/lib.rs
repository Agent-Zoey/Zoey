//! ZoeyOS MongoDB Plugin
//!
//! Database adapter for MongoDB with optional HIPAA-ready patterns (NOT certified).

#![warn(missing_docs)]
#![warn(clippy::all)]

// Re-exports
pub use zoey_core;

pub mod mongo;
pub mod vector_search;

// Re-export adapters
pub use mongo::MongoAdapter;
pub use vector_search::MongoVectorSearch;
