//! Agent API module
//!
//! Provides a secure REST API for agent interaction, enabling frontend
//! applications to communicate with agents through authenticated endpoints.
//!
//! # Features
//!
//! - **Authentication**: Token-based authentication with SHA-256 hashing
//! - **Authorization**: Permission-based access control (Read, Write, Execute, Admin)
//! - **Rate Limiting**: Configurable rate limits per token/user
//! - **Input Validation**: Automatic validation and sanitization
//! - **Streaming**: Server-Sent Events (SSE) for real-time responses
//! - **CORS**: Configurable CORS for frontend integration
//!
//! # Endpoints
//!
//! - `GET /health` - Health check (no auth required)
//! - `POST /agent/chat` - Send messages to agent (requires Write permission)
//! - `POST /agent/action` - Execute agent actions (requires Execute permission)
//! - `POST /agent/state` - Get agent state (requires Read permission)
//!
//! # Example
//!
//! ```no_run
//! use zoey_core::{AgentRuntime, RuntimeOpts, agent_api::{AgentApiServer, AgentApiConfig}};
//!
//! #[tokio::main]
//! async fn main() -> zoey_core::Result<()> {
//!     // Create agent runtime
//!     let runtime = AgentRuntime::new(RuntimeOpts::default()).await?;
//!
//!     // Configure API server
//!     let config = AgentApiConfig {
//!         host: "127.0.0.1".to_string(),
//!         port: 3000,
//!         require_auth: false,  // Disable for testing
//!         ..Default::default()
//!     };
//!
//!     // Start server
//!     let mut server = AgentApiServer::new(config, runtime);
//!     server.start().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Security Best Practices
//!
//! 1. **Always enable authentication in production** (`require_auth: true`)
//! 2. **Use HTTPS/TLS** for production deployments
//! 3. **Configure rate limiting** to prevent abuse
//! 4. **Validate CORS origins** - avoid using "*" in production
//! 5. **Rotate tokens regularly** and set expiration times
//! 6. **Monitor API usage** and set up alerts for suspicious activity
//!
//! # Authentication
//!
//! Requests to protected endpoints must include an Authorization header:
//!
//! ```text
//! Authorization: Bearer <your-token>
//! ```
//!
//! Tokens are SHA-256 hashed and validated against configured permissions.

pub mod auth;
pub mod handlers;
pub mod server;
pub mod state;
pub mod task;
pub mod types;

#[cfg(test)]
mod tests;

// Re-export main types
pub use auth::ApiAuthManager;
pub use handlers::ApiError;
pub use server::{AgentApiConfig, AgentApiServer};
pub use state::{ApiState, ServerState};
pub use task::{Task, TaskManager, TaskResult, TaskStatus};
pub use types::{
    ActionRequest, ActionResponse, ApiPermission, ApiResponse, ApiToken, ChatRequest, ChatResponse,
    HealthResponse, StateRequest, StateResponse, StreamEvent,
};
