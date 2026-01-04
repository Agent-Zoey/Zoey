//! Example: Agent API Server
//!
//! This example demonstrates how to set up an Agent API server that provides
//! HTTP endpoints for frontend applications to interact with the agent.
//!
//! # Features Demonstrated
//!
//! - Setting up the agent runtime
//! - Configuring the API server
//! - Starting the HTTP server
//! - Authentication (optional)
//! - Rate limiting
//! - CORS configuration
//!
//! # Running the Example
//!
//! ```bash
//! cargo run --example agent_api_server
//! ```
//!
//! # Testing the Endpoints
//!
//! Once running, you can test the endpoints:
//!
//! ```bash
//! # Health check
//! curl http://localhost:3000/health
//!
//! # Send a chat message
//! curl -X POST http://localhost:3000/agent/chat \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "text": "Hello, how are you?",
//!     "roomId": "00000000-0000-0000-0000-000000000001",
//!     "source": "api"
//!   }'
//!
//! # Get agent state
//! curl -X POST http://localhost:3000/agent/state \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "roomId": "00000000-0000-0000-0000-000000000001"
//!   }'
//! ```

use zoey_core::{
    agent_api::{AgentApiConfig, AgentApiServer},
    types::Character,
    AgentRuntime, Result, RuntimeOpts,
};
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("Starting Agent API Server Example");

    // Create a simple character
    let character = Character {
        name: "Assistant".to_string(),
        bio: Some("A helpful AI assistant".to_string()),
        ..Default::default()
    };

    // Create agent runtime
    info!("Initializing agent runtime...");
    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(character),
        ..Default::default()
    })
    .await?;

    info!("Agent runtime created with ID: {}", runtime.read().unwrap().agent_id);

    // Configure API server
    let config = AgentApiConfig {
        host: "127.0.0.1".to_string(),
        port: 3000,
        require_auth: false, // Disable auth for this example
        enable_rate_limit: true,
        rpm_limit: 60,
        enable_cors: true,
        cors_origins: vec!["*".to_string()],
        ..Default::default()
    };

    info!("Creating API server...");
    let mut server = AgentApiServer::new(config, runtime);

    // Start server
    info!("Starting API server on http://127.0.0.1:3000");
    server.start().await?;

    info!("Server is running! Press Ctrl+C to stop.");
    info!("");
    info!("Available endpoints:");
    info!("  GET  http://localhost:3000/health       - Health check");
    info!("  POST http://localhost:3000/agent/chat   - Send message to agent");
    info!("  POST http://localhost:3000/agent/action - Execute agent action");
    info!("  POST http://localhost:3000/agent/state  - Get agent state");
    info!("");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    info!("Shutting down server...");
    server.stop().await?;

    info!("Server stopped. Goodbye!");

    Ok(())
}
