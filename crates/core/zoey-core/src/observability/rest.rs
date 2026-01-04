use super::config::RestApiConfig;
use super::cost_tracker::CostTracker;
use super::types::*;
use crate::utils::logger::{subscribe_logs, LogEvent};
use axum::response::sse::{Event, Sse};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use futures_util::stream::BoxStream;
use futures_util::stream::StreamExt;
use regex::Regex;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::{BroadcastStream, IntervalStream};
use uuid::Uuid as UUID;

/// Shared state for REST API
#[derive(Clone)]
struct ApiState {
    cost_tracker: Option<Arc<CostTracker>>,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
}

/// Cost query parameters
#[derive(Debug, Deserialize)]
struct CostQuery {
    agent_id: Option<UUID>,
    #[serde(default)]
    period: String, // "hourly" or "daily"
}

/// Start the REST API server
pub async fn start_rest_api(
    config: RestApiConfig,
    cost_tracker: Option<Arc<CostTracker>>,
) -> Result<(), std::io::Error> {
    let state = ApiState { cost_tracker };

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/costs", get(costs_handler))
        .route("/costs/summary", get(costs_summary_handler))
        .route("/logs", get(logs_sse_handler))
        .with_state(state);

    let host = config.host;
    let mut port = config.port;
    let mut bound = None;
    for _ in 0..20 {
        let addr = format!("{}:{}", host, port);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                std::env::set_var("OBSERVABILITY_REST_API_PORT", port.to_string());
                tracing::info!("Starting REST API on {}", addr);
                bound = Some(listener);
                break;
            }
            Err(_) => {
                port = port.saturating_add(1);
                continue;
            }
        }
    }
    let listener = bound.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            "Failed to bind REST API",
        )
    })?;
    axum::serve(listener, app).await
}

/// Health check endpoint
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// Get costs for an agent
async fn costs_handler(
    State(state): State<ApiState>,
    Query(params): Query<CostQuery>,
) -> Result<Json<CostResponse>, ApiError> {
    let cost_tracker = state
        .cost_tracker
        .as_ref()
        .ok_or(ApiError::NotEnabled("Cost tracking not enabled"))?;

    let agent_id = params
        .agent_id
        .ok_or(ApiError::MissingParameter("agent_id"))?;

    let cost = match params.period.as_str() {
        "hourly" => cost_tracker.get_hourly_cost(agent_id).await,
        "daily" | "" => cost_tracker.get_daily_cost(agent_id).await,
        _ => {
            return Err(ApiError::InvalidParameter(
                "period must be 'hourly' or 'daily'",
            ))
        }
    };

    Ok(Json(CostResponse {
        agent_id,
        period: if params.period.is_empty() {
            "daily".to_string()
        } else {
            params.period
        },
        total_cost_usd: cost,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get cost summary
async fn costs_summary_handler(
    State(state): State<ApiState>,
) -> Result<Json<CostSummary>, ApiError> {
    let cost_tracker = state
        .cost_tracker
        .as_ref()
        .ok_or(ApiError::NotEnabled("Cost tracking not enabled"))?;

    let summary = cost_tracker.get_cost_summary().await;

    Ok(Json(summary))
}

/// Stream runtime logs via Server-Sent Events
/// Compliance: messages are length-capped and contain only structured metadata.
fn scrub_message(mut s: String) -> String {
    if s.len() > 2000 {
        s = s.chars().take(2000).collect();
    }
    // redact obvious secrets and identifiers
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

async fn logs_sse_handler() -> Sse<BoxStream<'static, Result<Event, Infallible>>> {
    let rx = subscribe_logs();
    let stream: BoxStream<Result<Event, Infallible>> = match rx {
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
            .boxed(),
        None => BroadcastStream::new(crate::utils::logger::subscribe_logs().unwrap_or_else(|| {
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
        }))
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
    let init = futures_util::stream::once(async {
        Ok(Event::default()
            .event("ready")
            .data("{\"message\":\"logs stream ready\"}"))
    });
    let keepalive = IntervalStream::new(tokio::time::interval(Duration::from_secs(15)))
        .map(|_| Ok(Event::default().event("ping").data("{}")))
        .boxed();
    let stream = futures_util::stream::select(init.boxed(), stream);
    let stream = futures_util::stream::select(stream, keepalive).boxed();
    Sse::new(stream)
}

/// Cost response
#[derive(Debug, Serialize)]
struct CostResponse {
    agent_id: UUID,
    period: String,
    total_cost_usd: f64,
    timestamp: String,
}

/// API error types
#[derive(Debug)]
enum ApiError {
    NotEnabled(&'static str),
    MissingParameter(&'static str),
    InvalidParameter(&'static str),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotEnabled(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            ApiError::MissingParameter(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::InvalidParameter(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint() {
        let response = health_handler().await;
        assert_eq!(response.0.status, "ok");
    }
}
#[inline]
pub fn extract_rate_limit_from_headers(headers: &HeaderMap) -> ProviderRateLimit {
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok());
    let reset_epoch_s = headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    ProviderRateLimit {
        remaining,
        reset_epoch_s,
    }
}
