//! Common test utilities and mock servers for E2E testing

use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Test Configuration
// ============================================================================

/// Test wallet address (fake)
pub const TEST_WALLET_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";

/// Test API key
pub const TEST_API_KEY: &str = "test-api-key-12345";

/// Default x402 facilitator URL for production use
pub const DEFAULT_FACILITATOR_URL: &str = "https://facilitator.payai.network";

/// Create a test configuration for the x402 video plugin
pub fn create_test_config() -> zoey_plugin_x402_video::X402VideoConfig {
    create_test_config_with_facilitator(None)
}

/// Create a test configuration with a custom facilitator URL
/// If None is provided, uses a placeholder mock URL (for tests that will override it)
pub fn create_test_config_with_facilitator(facilitator_url: Option<&str>) -> zoey_plugin_x402_video::X402VideoConfig {
    use zoey_plugin_x402_video::config::*;

    X402VideoConfig {
        x402: X402Config {
            facilitator_url: facilitator_url
                .unwrap_or("http://127.0.0.1:9999/facilitator")
                .to_string(),
            // Use test wallet as facilitator address for tests
            facilitator_pay_to_address: TEST_WALLET_ADDRESS.to_string(),
            wallet_address: TEST_WALLET_ADDRESS.to_string(),
            private_key_env: "TEST_PRIVATE_KEY".to_string(),
            supported_networks: vec!["base".to_string()],
            supported_tokens: vec!["USDC".to_string()],
            default_price_cents: 100,
            payment_timeout_secs: 300,
        },
        video_generation: VideoGenerationConfig {
            provider: VideoProvider::Replicate,
            api_url: "http://127.0.0.1:9998".to_string(),
            api_key_env: "TEST_VIDEO_API_KEY".to_string(),
            default_duration_secs: 4,
            default_resolution: VideoResolution::HD720p,
            max_duration_secs: 16,
            webhook_url: None,
        },
        platforms: PlatformConfigs::default(),
    }
}

// ============================================================================
// Mock X402 Facilitator Server
// ============================================================================

/// State for mock facilitator
#[derive(Default)]
pub struct MockFacilitatorState {
    /// Valid payment proofs
    pub valid_proofs: RwLock<HashMap<String, MockPayment>>,
    /// Request log
    pub requests: RwLock<Vec<String>>,
}

/// Mock payment record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockPayment {
    pub from: String,
    pub to: String,
    pub amount: String,
    pub network: String,
    pub tx_hash: String,
}

/// Start a mock x402 facilitator server
pub async fn start_mock_facilitator(port: u16) -> (SocketAddr, Arc<MockFacilitatorState>) {
    let state = Arc::new(MockFacilitatorState::default());
    let state_clone = state.clone();

    let app = Router::new()
        .route("/facilitator/verify", post(mock_verify_payment))
        .with_state(state_clone);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    (actual_addr, state)
}

/// Mock payment verification endpoint
async fn mock_verify_payment(
    State(state): State<Arc<MockFacilitatorState>>,
    Json(request): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Log request
    state
        .requests
        .write()
        .await
        .push(request.to_string());

    let x402_header = request["x402Header"].as_str().unwrap_or("");

    // Decode the base64 payload to check the signature field
    let payment_type = if x402_header.starts_with("x402 ") {
        let encoded = &x402_header[5..];
        if let Ok(decoded) = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            encoded,
        ) {
            if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&decoded) {
                payload["signature"]
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        x402_header.to_string()
    };

    // Check payment type based on signature field
    if payment_type.contains("valid-payment-proof") {
        Json(serde_json::json!({
            "valid": true,
            "txHash": "0xabc123def456",
            "message": "Payment verified successfully"
        }))
    } else if payment_type.contains("invalid-payment") {
        Json(serde_json::json!({
            "valid": false,
            "txHash": null,
            "message": "Invalid payment signature"
        }))
    } else if payment_type.contains("expired-payment") {
        Json(serde_json::json!({
            "valid": false,
            "txHash": null,
            "message": "Payment authorization expired"
        }))
    } else {
        // Default: reject unknown payments
        Json(serde_json::json!({
            "valid": false,
            "txHash": null,
            "message": "Unknown payment type"
        }))
    }
}

// ============================================================================
// Mock Video Generation Server
// ============================================================================

/// State for mock video server
#[derive(Default)]
pub struct MockVideoServerState {
    /// Jobs and their status
    pub jobs: RwLock<HashMap<String, MockVideoJob>>,
    /// Request counter
    pub request_count: RwLock<u32>,
}

/// Mock video job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockVideoJob {
    pub id: String,
    pub prompt: String,
    pub status: String,
    pub progress: u8,
    pub video_url: Option<String>,
    pub created_at: i64,
}

/// Start a mock video generation server (Replicate-like)
pub async fn start_mock_video_server(port: u16) -> (SocketAddr, Arc<MockVideoServerState>) {
    let state = Arc::new(MockVideoServerState::default());
    let state_clone = state.clone();

    let app = Router::new()
        .route("/predictions", post(mock_create_prediction))
        .route("/predictions/:id", get(mock_get_prediction))
        .with_state(state_clone);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    (actual_addr, state)
}

/// Mock create prediction endpoint
async fn mock_create_prediction(
    State(state): State<Arc<MockVideoServerState>>,
    Json(request): Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut count = state.request_count.write().await;
    *count += 1;
    let job_id = format!("pred-{}", *count);

    let prompt = request["input"]["prompt"]
        .as_str()
        .unwrap_or("test prompt")
        .to_string();

    // Check for special test prompts
    let (status, video_url) = if prompt.contains("instant-complete") {
        (
            "succeeded".to_string(),
            Some("https://mock-video.test/video.mp4".to_string()),
        )
    } else if prompt.contains("fail-generation") {
        ("failed".to_string(), None)
    } else {
        ("processing".to_string(), None)
    };

    let job = MockVideoJob {
        id: job_id.clone(),
        prompt,
        status: status.clone(),
        progress: if status == "succeeded" { 100 } else { 0 },
        video_url: video_url.clone(),
        created_at: chrono::Utc::now().timestamp(),
    };

    state.jobs.write().await.insert(job_id.clone(), job);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": job_id,
            "status": status,
            "output": video_url,
            "error": null
        })),
    )
}

/// Mock get prediction status endpoint
async fn mock_get_prediction(
    Path(job_id): Path<String>,
    State(state): State<Arc<MockVideoServerState>>,
) -> impl IntoResponse {
    let mut jobs = state.jobs.write().await;

    if let Some(job) = jobs.get_mut(&job_id) {
        // Simulate progress
        if job.status == "processing" {
            job.progress = (job.progress + 25).min(100);
            if job.progress >= 100 {
                job.status = "succeeded".to_string();
                job.video_url = Some(format!("https://mock-video.test/{}.mp4", job_id));
            }
        }

        (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": job.id,
                "status": job.status,
                "output": job.video_url,
                "progress": job.progress,
                "error": if job.status == "failed" { Some("Generation failed") } else { None::<&str> }
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Prediction not found"
            })),
        )
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a valid x402 payment header for testing
pub fn create_test_x402_header(payment_type: &str) -> String {
    let payload = serde_json::json!({
        "version": 1,
        "network": "base",
        "from": "0xpayer123",
        "to": TEST_WALLET_ADDRESS,
        "asset": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
        "amount": "1000000",
        "validUntil": chrono::Utc::now().timestamp() + 3600,
        "nonce": "test-nonce-123",
        "signature": format!("{}-signature", payment_type)
    });

    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        payload.to_string(),
    );

    format!("x402 {}", encoded)
}

/// Create a mock HTTP request with x402 header
pub fn create_request_with_x402<T: Serialize>(
    method: &str,
    uri: &str,
    body: T,
    payment_type: Option<&str>,
) -> Request<Body> {
    let body_bytes = serde_json::to_vec(&body).unwrap();

    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json");

    if let Some(pt) = payment_type {
        builder = builder.header("X-402", create_test_x402_header(pt));
    }

    builder.body(Body::from(body_bytes)).unwrap()
}

/// Assert that a response has the expected status code
pub async fn assert_status(response: axum::response::Response, expected: StatusCode) {
    assert_eq!(
        response.status(),
        expected,
        "Expected status {}, got {}",
        expected,
        response.status()
    );
}

/// Extract JSON body from response
pub async fn extract_json<T: for<'de> Deserialize<'de>>(
    response: axum::response::Response,
) -> T {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

