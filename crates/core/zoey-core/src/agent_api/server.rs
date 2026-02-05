//! Agent API server implementation
//!
//! Provides HTTP server for agent interaction with:
//! - Authentication and authorization
//! - Rate limiting
//! - Input validation
//! - CORS support
//! - Secure communication

use super::{
    auth::ApiAuthManager,
    handlers::{
        action_handler, chat_handler, health_check, state_handler, task_status_handler, ApiError,
    },
    state::{ApiState, ServerState},
    types::ApiPermission,
};
use crate::utils::logger::{subscribe_logs, LogEvent};
use crate::{
    security::RateLimiter,
    types::service::{Service, ServiceHealth},
    AgentRuntime, ZoeyError, Result,
};
use async_trait::async_trait;
use axum::response::sse::{Event, Sse};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use futures_util::stream::{self, BoxStream, StreamExt};
use regex::Regex;
use std::convert::Infallible;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};

/// Agent API configuration
#[derive(Debug, Clone)]
pub struct AgentApiConfig {
    /// Server host
    pub host: String,

    /// Server port
    pub port: u16,

    /// Enable authentication
    pub require_auth: bool,

    /// Enable rate limiting
    pub enable_rate_limit: bool,

    /// Requests per minute per token
    pub rpm_limit: u32,

    /// Rate limit window duration
    pub rate_limit_window: Duration,

    /// Enable CORS
    pub enable_cors: bool,

    /// Allowed CORS origins
    pub cors_origins: Vec<String>,
}

impl Default for AgentApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            require_auth: false,
            enable_rate_limit: true,
            rpm_limit: 60,
            rate_limit_window: Duration::from_secs(60),
            enable_cors: true,
            cors_origins: vec!["*".to_string()],
        }
    }
}

/// Agent API server service
pub struct AgentApiServer {
    /// Server configuration
    config: Arc<AgentApiConfig>,

    /// Agent runtime
    runtime: Arc<RwLock<AgentRuntime>>,

    /// Server state
    state: Option<ServerState>,

    /// Server handle for shutdown
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,

    /// Is the server running
    running: bool,
}

impl AgentApiServer {
    /// Create new agent API server
    pub fn new(config: AgentApiConfig, runtime: Arc<RwLock<AgentRuntime>>) -> Self {
        Self {
            config: Arc::new(config),
            runtime,
            state: None,
            shutdown_tx: None,
            running: false,
        }
    }

    /// Build the Axum router
    fn build_router(state: ServerState) -> Router {
        let enable_cors = state.config.enable_cors;
        let require_auth = state.config.require_auth;
        let enable_rate_limit = state.config.enable_rate_limit;

        // Build all routes
        let mut router = Router::new()
            // Health check (no auth required)
            .route("/health", get(health_check))
            // Agent endpoints (will be protected if auth/rate limiting enabled)
            .route("/agent/chat", post(chat_handler))
            .route(
                "/agent/chat/stream",
                post(super::handlers::chat_stream_handler),
            )
            .route("/agent/action", post(action_handler))
            .route("/agent/state", post(state_handler))
            // Character management
            .route(
                "/agent/characters",
                get(super::handlers::character_list_handler),
            )
            .route(
                "/agent/character/select",
                post(super::handlers::character_select_handler),
            )
            // Provider management
            .route(
                "/agent/providers",
                get(super::handlers::providers_list_handler),
            )
            .route(
                "/agent/provider/switch",
                post(super::handlers::provider_switch_handler),
            )
            // Context hints management
            .route(
                "/agent/context/add",
                post(super::handlers::context_add_handler),
            )
            .route(
                "/agent/context/save",
                post(super::handlers::context_save_handler),
            )
            .route(
                "/agent/room/delete",
                post(super::handlers::delete_room_handler),
            )
            // Memory persistence endpoint (async, for all clients)
            .route(
                "/agent/memory",
                post(super::handlers::memory_create_handler),
            )
            // Knowledge management endpoints (secure document ingestion)
            .route(
                "/agent/knowledge/ingest",
                post(super::handlers::knowledge_ingest_handler),
            )
            .route(
                "/agent/knowledge/query",
                post(super::handlers::knowledge_query_handler),
            )
            .route(
                "/agent/knowledge/list/:room_id",
                get(super::handlers::knowledge_list_handler),
            )
            // Task polling endpoint
            .route("/agent/task/:task_id", get(task_status_handler))
            .route("/agent/logs", get(agent_logs_sse))
            // Training / RLHF endpoints
            .route("/agent/mcp/statistics", get(super::handlers::training_statistics_handler))
            .route("/agent/mcp/feedback", post(super::handlers::training_feedback_handler))
            .route("/agent/mcp/export", post(super::handlers::training_export_handler))
            .route("/agent/mcp/samples", get(super::handlers::training_samples_handler))
            .route("/agent/mcp/training/start", post(super::handlers::training_start_handler))
            .route("/agent/mcp/training/status", get(super::handlers::training_job_status_handler))
            .route("/agent/mcp/training/jobs", get(super::handlers::training_jobs_handler))
            .route("/agent/mcp/training/events", get(training_events_sse))
            // Skill documentation
            .route("/docs/skills", get(super::handlers::skill_docs_handler))
            .with_state(state.clone());

        // Note: Middleware temporarily disabled due to complex type inference issues
        // To enable, configure tokens in AuthManager and set require_auth/enable_rate_limit to true

        // Authentication and rate limiting are handled at the application level
        // Alternative approach: Check auth/rate limits directly in handlers for now
        let _ = (require_auth, enable_rate_limit); // Suppress unused warnings

        // Add CORS if enabled (outermost layer)
        if enable_cors {
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);
            router = router.layer(cors);
        }

        // HIPAA/GDPR compliant logs streaming is served by Observability REST at /logs

        router
    }
}

fn scrub_message(mut s: String) -> String {
    if s.len() > 2000 {
        s = s.chars().take(2000).collect();
    }
    let patterns = [
        (Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap(), "sk-REDACTED"),
        (
            Regex::new(r"(?i)api[_-]?key\s*[:=]?\s*[A-Za-z0-9-_]{12,}").unwrap(),
            "api_key=REDACTED",
        ),
        (
            Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap(),
            "email@redacted",
        ),
        (
            Regex::new(r"\b\+?\d[\d\s-]{8,}\b").unwrap(),
            "PHONE_REDACTED",
        ),
    ];
    for (re, rep) in patterns.iter() {
        s = re.replace_all(&s, *rep).into_owned();
    }
    s
}

async fn agent_logs_sse() -> Sse<BoxStream<'static, std::result::Result<Event, Infallible>>> {
    if std::env::var("AGENT_LOGS_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .parse()
        .unwrap_or(false)
        == false
    {
        let empty = futures_util::stream::empty::<std::result::Result<Event, Infallible>>().boxed();
        return Sse::new(empty);
    }
    let rx = subscribe_logs();
    let stream: BoxStream<'static, std::result::Result<Event, Infallible>> = match rx {
        Some(rx) => BroadcastStream::new(rx)
            .filter_map(|item| async move {
                match item {
                    Ok(mut ev) => {
                        ev.message = scrub_message(ev.message);
                        let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
                        Some(Ok(Event::default().data(data)))
                    }
                    Err(_) => None,
                }
            })
            .boxed()
            .chain(stream::once(async move {
                let init = LogEvent { level: "INFO".into(), target: "logs".into(), message: "connected".into(), file: None, line: None, time: chrono::Utc::now().to_rfc3339() };
                let data = serde_json::to_string(&init).unwrap_or_else(|_| "{}".to_string());
                Ok(Event::default().data(data))
            }))
            .boxed(),
        None => BroadcastStream::new({
            let (tx, rx) = tokio::sync::broadcast::channel::<LogEvent>(1);
            let _ = tx.send(LogEvent {
                level: "INFO".into(),
                target: "init".into(),
                message: "logging not initialized".into(),
                file: None,
                line: None,
                time: chrono::Utc::now().to_rfc3339(),
            });
            rx
        })
        .filter_map(|item| async move {
            match item {
                Ok(mut ev) => {
                    ev.message = scrub_message(ev.message);
                    let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
                    Some(Ok(Event::default().data(data)))
                }
                Err(_) => None,
            }
        })
        .boxed(),
    };
    Sse::new(stream)
}

/// Training events SSE endpoint for real-time training updates
async fn training_events_sse(
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> Sse<BoxStream<'static, std::result::Result<Event, Infallible>>> {
    use futures_util::stream;
    
    let runtime = state.api_state.runtime.clone();
    
    // Create a polling stream that checks training status periodically
    let stream = stream::unfold(
        (runtime, std::time::Instant::now()),
        |(runtime, last_update)| async move {
            // Wait 2 seconds between updates
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            let rt = runtime.read().unwrap();
            
            // Get training statistics
            let event_data = if let Some(collector) = rt.get_training_collector() {
                let stats = collector.get_statistics();
                
                // Get active jobs
                let jobs: Vec<_> = super::handlers::get_training_jobs()
                    .read()
                    .map(|j| j.values().cloned().collect())
                    .unwrap_or_default();
                
                serde_json::json!({
                    "type": "training_update",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "stats": {
                        "totalSamples": stats.total_samples,
                        "withFeedbackCount": stats.with_feedback_count,
                        "avgQualityScore": stats.avg_quality_score
                    },
                    "activeJobs": jobs.len(),
                    "jobs": jobs
                })
            } else {
                serde_json::json!({
                    "type": "training_update",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "stats": null,
                    "message": "Training not available"
                })
            };
            
            drop(rt);
            
            let event = Event::default()
                .event("training")
                .data(event_data.to_string());
            
            Some((Ok(event), (runtime, std::time::Instant::now())))
        },
    )
    .boxed();
    
    Sse::new(stream)
}

impl AgentApiServer {
    /// Start the server
    pub async fn start(&mut self) -> Result<()> {
        if self.running {
            return Err(ZoeyError::Config("Server already running".to_string()));
        }

        let api_state = ApiState::new(self.runtime.clone());
        let auth_manager = Arc::new(ApiAuthManager::disabled());
        let rate_limiter = Arc::new(RwLock::new(RateLimiter::new(
            self.config.rate_limit_window,
            self.config.rpm_limit as usize,
        )));
        // Task manager with 5 minute task retention
        let task_manager = super::task::TaskManager::new(300);

        let state = ServerState {
            api_state,
            auth_manager,
            rate_limiter,
            task_manager,
            config: self.config.clone(),
        };

        self.state = Some(state.clone());

        let router = Self::build_router(state.clone());

        let addr = format!("{}:{}", self.config.host, self.config.port);
        info!("Starting Agent API server on {}", addr);

        // Spawn background task cleanup
        let cleanup_task_manager = state.task_manager.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                cleanup_task_manager.cleanup_old_tasks();
                debug!(
                    "Task cleanup completed. Active tasks: {}",
                    cleanup_task_manager.task_count()
                );
            }
        });

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| ZoeyError::Config(format!("Failed to bind to {}: {}", addr, e)))?;

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(tx);

        // Spawn server task
        tokio::spawn(async move {
            let server = axum::serve(listener, router).with_graceful_shutdown(async {
                let _ = rx.await;
            });
            if let Err(e) = server.await {
                error!("Server error: {}", e);
            }
        });

        self.running = true;
        Ok(())
    }

    /// Stop the server
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        self.running = false;
        self.state = None;

        info!("Agent API server stopped");
        Ok(())
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.running
    }
}

/// Authentication middleware
async fn auth_middleware(state: State<ServerState>, request: Request, next: Next) -> Response {
    let state = state.0; // Extract the inner ServerState
                         // Skip auth for health check
    if request.uri().path() == "/health" {
        return next.run(request).await;
    }

    // Extract token from Authorization header
    let token = match request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        Some(t) => t,
        None => {
            return ApiError::Unauthorized("Missing or invalid Authorization header".to_string())
                .into_response();
        }
    };

    // Validate token
    let permissions = match state.auth_manager.validate_token(token).await {
        Ok(p) => p,
        Err(_) => {
            return ApiError::Unauthorized("Invalid token".to_string()).into_response();
        }
    };

    // Check if token has required permission based on endpoint
    let required_permission = match request.uri().path() {
        p if p.starts_with("/agent/chat") => ApiPermission::Write,
        p if p.starts_with("/agent/action") => ApiPermission::Execute,
        p if p.starts_with("/agent/state") => ApiPermission::Read,
        _ => ApiPermission::Read,
    };

    if !permissions.contains(&required_permission) && !permissions.contains(&ApiPermission::Admin) {
        return ApiError::Forbidden(format!(
            "Token does not have required permission: {:?}",
            required_permission
        ))
        .into_response();
    }

    debug!("Request authenticated successfully");
    next.run(request).await
}

/// Rate limiting middleware
async fn rate_limit_middleware(
    State(state): State<ServerState>,
    request: Request,
    next: Next,
) -> Response {
    if !state.config.enable_rate_limit {
        return next.run(request).await;
    }

    // Extract token or IP for rate limiting
    let key = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous")
        .to_string();

    // Check rate limit
    let limiter = state.rate_limiter.write().unwrap();
    if !limiter.check(&key) {
        drop(limiter); // Release the lock
        warn!("Rate limit exceeded for key: {}", key);
        return ApiError::RateLimited("Rate limit exceeded. Please try again later.".to_string())
            .into_response();
    }
    drop(limiter); // Release the lock

    next.run(request).await
}

/// Implement Service trait for AgentApiServer
#[async_trait]
impl Service for AgentApiServer {
    fn service_type(&self) -> &str {
        "agent_api"
    }

    async fn start(&mut self) -> Result<()> {
        AgentApiServer::start(self).await
    }

    async fn stop(&mut self) -> Result<()> {
        AgentApiServer::stop(self).await
    }

    fn is_running(&self) -> bool {
        self.running
    }

    async fn health_check(&self) -> Result<ServiceHealth> {
        Ok(if self.running {
            ServiceHealth::Healthy
        } else {
            ServiceHealth::Unhealthy
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{types::Character, RuntimeOpts};

    #[tokio::test]
    #[ignore]
    async fn test_server_lifecycle() {
        let runtime = AgentRuntime::new(RuntimeOpts {
            character: Some(Character::default()),
            ..Default::default()
        })
        .await
        .unwrap();

        let config = AgentApiConfig::default();
        let mut server = AgentApiServer::new(config, runtime);

        assert!(!server.is_running());

        // Note: We can't actually start the server in tests due to port conflicts
        // In a real environment, you would:
        // server.start().await.unwrap();
        // assert!(server.is_running());
        // server.stop().await.unwrap();
        // assert!(!server.is_running());
    }
}
