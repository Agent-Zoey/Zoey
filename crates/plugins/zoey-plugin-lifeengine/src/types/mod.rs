//! Core types for the Life Engine
//!
//! Implements the foundational abstractions inspired by OpenSouls:
//! - WorkingMemory: Immutable collection of thought fragments
//! - CognitiveStep: Functions that transform working memory
//! - MentalProcess: State machine for behavioral modes
//! - SoulConfig: Personality, drive, and ego definitions
//! - EmotionalState: Dynamic emotional modeling

mod cognitive;
mod emotion;
mod memory;
pub mod mental_process;
pub mod soul_config;

pub use cognitive::*;
pub use emotion::*;
pub use memory::*;
pub use mental_process::*;
pub use soul_config::*;

