//! Life Engine Providers
//!
//! Providers that supply soul state context to LLM prompts.

mod soul_state;
mod emotion;
mod drive;

pub use soul_state::SoulStateProvider;
pub use emotion::EmotionProvider;
pub use drive::DriveProvider;

