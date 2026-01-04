//! Utility functions and helpers

pub mod delayed_reassessment;
pub mod logger;
pub mod rhythm;
pub mod search;
pub mod uuid;

// Re-export commonly used utilities
pub use self::delayed_reassessment::DelayedReassessment;
pub use self::logger::Logger;
pub use self::rhythm::ConversationRhythm;
pub use self::search::BM25;
pub use self::uuid::{create_unique_uuid, string_to_uuid};

// Re-export from dynamic_prompts for convenience
pub use crate::dynamic_prompts::{compose_random_user, upgrade_double_to_triple};
