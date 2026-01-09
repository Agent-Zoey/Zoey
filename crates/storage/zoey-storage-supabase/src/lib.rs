//! ZoeyOS Supabase Plugin
//!
//! Database adapter for Supabase with pgvector support for embeddings.
//! Uses Supabase REST API (PostgREST) for database operations.

#![warn(missing_docs)]
#![warn(clippy::all)]

// Re-exports
pub use zoey_core;

pub mod supabase;
pub mod vector_search;

// Re-export adapters
pub use supabase::{SupabaseAdapter, SupabaseConfig};
pub use vector_search::SupabaseVectorSearch;
