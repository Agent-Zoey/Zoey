//! Infrastructure module for production-ready features
//!
//! This module provides:
//! - Graceful shutdown with state persistence
//! - Enhanced health checks
//! - Request tracing and correlation
//! - Webhook integration
//! - Batch processing API
//! - Rate limiting tiers
//! - Input sanitization
//! - API key rotation
//! - Secure defaults

pub mod batch;
pub mod key_rotation;
#[cfg(feature = "otel")]
pub mod otel;
pub mod rate_limiting;
pub mod sanitization;
pub mod secure_defaults;
pub mod shutdown;
pub mod tracing;
pub mod webhooks;

pub use batch::*;
pub use key_rotation::*;
#[cfg(feature = "otel")]
pub use otel::*;
pub use rate_limiting::*;
pub use sanitization::*;
pub use secure_defaults::*;
pub use shutdown::*;
pub use tracing::*;
pub use webhooks::*;
