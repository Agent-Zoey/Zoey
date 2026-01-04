//! End-to-End Tests for HTTP API Endpoints
//!
//! Tests the REST API endpoints exposed by the plugin:
//! - GET /x402-video/pricing
//! - POST /x402-video/generate (with/without x402 payment)
//! - GET /x402-video/status/:job_id
//! - POST /x402-video/post/:job_id
//! - GET /x402-video/health

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::*;
use zoey_plugin_x402_video::{
    config::PlatformTarget,
    routes::{
        GenerateVideoRequest, GenerateVideoResponse, JobStatusResponse,
        PaymentRequiredResponse, PricingResponse, X402VideoRouteState,
    },
    X402VideoPlugin,
};
use std::sync::Arc;
use tower::util::ServiceExt;

// ============================================================================
// Test Setup
// ============================================================================

async fn setup_test_plugin() -> (X402VideoPlugin, Arc<X402VideoRouteState>) {
    let config = create_test_config();
    let plugin = X402VideoPlugin::new(config);
    let state = plugin.create_route_state();
    (plugin, state)
}

async fn setup_with_mock_servers() -> (
    X402VideoPlugin,
    Arc<X402VideoRouteState>,
    std::net::SocketAddr,
    std::net::SocketAddr,
) {
    use zoey_core::types::Service;

    // Start mock servers
    let (facilitator_addr, _) = start_mock_facilitator(0).await;
    let (video_addr, _) = start_mock_video_server(0).await;

    // Set up environment with unique var name
    let env_var_name = format!("TEST_VIDEO_API_KEY_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
    std::env::set_var(&env_var_name, TEST_API_KEY);

    // Create config with mock server addresses
    let mut config = create_test_config();
    config.x402.facilitator_url = format!("http://{}/facilitator", facilitator_addr);
    config.video_generation.api_url = format!("http://{}", video_addr);
    config.video_generation.api_key_env = env_var_name;

    // Create service and initialize it
    let mut video_service = zoey_plugin_x402_video::services::VideoGenerationService::new(
        config.video_generation.clone(),
    );
    video_service.initialize(Arc::new(())).await.unwrap();

    let payment_service = Arc::new(zoey_plugin_x402_video::services::X402PaymentService::new(
        config.x402.clone(),
    ));

    let poster = Arc::new(zoey_plugin_x402_video::services::MultiPlatformPoster::new(
        config.platforms.instagram.clone(),
        config.platforms.tiktok.clone(),
        config.platforms.snapchat.clone(),
    ));

    let state = Arc::new(X402VideoRouteState {
        video_service: Arc::new(video_service),
        payment_service,
        poster,
        config: config.clone(),
        pending_jobs: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    });

    let plugin = X402VideoPlugin::new(config);

    (plugin, state, facilitator_addr, video_addr)
}

// ============================================================================
// Pricing Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_get_pricing() {
    let (plugin, state) = setup_test_plugin().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let request = Request::builder()
        .method("GET")
        .uri("/x402-video/pricing")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let pricing: PricingResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(pricing.base_price_cents, 100);
    assert!(pricing.networks.contains(&"base".to_string()));
    assert!(pricing.tokens.contains(&"USDC".to_string()));
    assert_eq!(pricing.wallet_address, TEST_WALLET_ADDRESS);
}

#[tokio::test]
async fn test_pricing_shows_enabled_platforms() {
    let (plugin, state) = setup_test_plugin().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let request = Request::builder()
        .method("GET")
        .uri("/x402-video/pricing")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let pricing: PricingResponse = serde_json::from_slice(&body).unwrap();

    // Default config has no platforms enabled
    assert!(pricing.enabled_platforms.is_empty());
}

// ============================================================================
// Generate Endpoint Tests - Payment Required
// ============================================================================

#[tokio::test]
async fn test_generate_without_payment_returns_402() {
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let gen_request = GenerateVideoRequest {
        prompt: "A beautiful sunset".to_string(),
        image_url: None,
        caption: None,
        hashtags: vec![],
        platforms: vec![],
        options: Default::default(),
        price_cents: None,
        callback_url: None,
    };

    let request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&gen_request).unwrap()))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);

    // Check for WWW-Authenticate header
    assert!(
        response.headers().contains_key("www-authenticate"),
        "Should have WWW-Authenticate header"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payment_response: PaymentRequiredResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(payment_response.error, "Payment required");
    assert_eq!(payment_response.payment.scheme, "x402");
    assert_eq!(payment_response.payment.network, "base");
    assert!(!payment_response.payment.resource_id.is_empty());

    // Cleanup
    std::env::remove_var("TEST_VIDEO_API_KEY");
}

#[tokio::test]
async fn test_generate_payment_requirement_includes_price() {
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let gen_request = GenerateVideoRequest {
        prompt: "Test video".to_string(),
        image_url: None,
        caption: None,
        hashtags: vec![],
        platforms: vec![],
        options: Default::default(),
        price_cents: None,
        callback_url: None,
    };

    let request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&gen_request).unwrap()))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payment_response: PaymentRequiredResponse = serde_json::from_slice(&body).unwrap();

    // Default price is 100 cents = 1,000,000 USDC units
    assert_eq!(payment_response.payment.amount, "1000000");

    std::env::remove_var("TEST_VIDEO_API_KEY");
}

// ============================================================================
// Generate Endpoint Tests - With Payment
// ============================================================================

#[tokio::test]
async fn test_generate_with_valid_payment() {
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let gen_request = GenerateVideoRequest {
        prompt: "instant-complete: A test video".to_string(),
        image_url: None,
        caption: Some("Test caption".to_string()),
        hashtags: vec!["test".to_string()],
        platforms: vec![],
        options: Default::default(),
        price_cents: None,
        callback_url: None,
    };

    let x402_header = create_test_x402_header("valid-payment-proof");

    let request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .header("X-402", x402_header)
        .body(Body::from(serde_json::to_vec(&gen_request).unwrap()))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let gen_response: GenerateVideoResponse = serde_json::from_slice(&body).unwrap();

    assert!(!gen_response.job_id.is_empty());
    assert!(gen_response.payment_receipt.is_some());
    assert!(gen_response.status_url.contains(&gen_response.job_id));

    std::env::remove_var("TEST_VIDEO_API_KEY");
}

#[tokio::test]
async fn test_generate_with_invalid_payment() {
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let gen_request = GenerateVideoRequest {
        prompt: "Test video".to_string(),
        image_url: None,
        caption: None,
        hashtags: vec![],
        platforms: vec![],
        options: Default::default(),
        price_cents: None,
        callback_url: None,
    };

    let x402_header = create_test_x402_header("invalid-payment");

    let request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .header("X-402", x402_header)
        .body(Body::from(serde_json::to_vec(&gen_request).unwrap()))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);

    std::env::remove_var("TEST_VIDEO_API_KEY");
}

#[tokio::test]
async fn test_generate_with_authorization_header() {
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let gen_request = GenerateVideoRequest {
        prompt: "instant-complete: Test".to_string(),
        image_url: None,
        caption: None,
        hashtags: vec![],
        platforms: vec![],
        options: Default::default(),
        price_cents: None,
        callback_url: None,
    };

    // Using Authorization header instead of X-402
    let x402_header = create_test_x402_header("valid-payment-proof");

    let request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .header("Authorization", x402_header)
        .body(Body::from(serde_json::to_vec(&gen_request).unwrap()))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    std::env::remove_var("TEST_VIDEO_API_KEY");
}

// ============================================================================
// Status Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_get_status_not_found() {
    // Use full setup - the route should return 404 for jobs not in our pending_jobs cache
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let request = Request::builder()
        .method("GET")
        .uri("/x402-video/status/nonexistent-job-id")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    // Should be NOT_FOUND since job was never created through our API
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_status_for_existing_job() {
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state.clone());

    // First, create a job
    let gen_request = GenerateVideoRequest {
        prompt: "instant-complete: Status test".to_string(),
        image_url: None,
        caption: None,
        hashtags: vec![],
        platforms: vec![],
        options: Default::default(),
        price_cents: None,
        callback_url: None,
    };

    let x402_header = create_test_x402_header("valid-payment-proof");

    let create_request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .header("X-402", &x402_header)
        .body(Body::from(serde_json::to_vec(&gen_request).unwrap()))
        .unwrap();

    let create_response = router.clone().oneshot(create_request).await.unwrap();
    let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let gen_response: GenerateVideoResponse = serde_json::from_slice(&body).unwrap();

    // Now check status
    let status_request = Request::builder()
        .method("GET")
        .uri(format!("/x402-video/status/{}", gen_response.job_id))
        .body(Body::empty())
        .unwrap();

    let status_response = router.oneshot(status_request).await.unwrap();

    assert_eq!(status_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(status_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let job_status: JobStatusResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(job_status.job_id, gen_response.job_id);
    // Since we used "instant-complete" prompt, it should be completed
    assert!(job_status.video_url.is_some() || job_status.status.contains("Completed"));

    std::env::remove_var("TEST_VIDEO_API_KEY");
}

// ============================================================================
// Health Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_health_check() {
    let (plugin, state) = setup_test_plugin().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let request = Request::builder()
        .method("GET")
        .uri("/x402-video/health")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let health: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(health["status"], "healthy");
    assert_eq!(health["plugin"], "x402-video");
    assert!(health["wallet_configured"].as_bool().unwrap());
}

#[tokio::test]
async fn test_health_shows_pending_jobs() {
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state.clone());

    // Create a job first
    let gen_request = GenerateVideoRequest {
        prompt: "A test video".to_string(),
        image_url: None,
        caption: None,
        hashtags: vec![],
        platforms: vec![],
        options: Default::default(),
        price_cents: None,
        callback_url: None,
    };

    let x402_header = create_test_x402_header("valid-payment-proof");

    let create_request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .header("X-402", x402_header)
        .body(Body::from(serde_json::to_vec(&gen_request).unwrap()))
        .unwrap();

    let _ = router.clone().oneshot(create_request).await.unwrap();

    // Check health
    let health_request = Request::builder()
        .method("GET")
        .uri("/x402-video/health")
        .body(Body::empty())
        .unwrap();

    let health_response = router.oneshot(health_request).await.unwrap();
    let body = axum::body::to_bytes(health_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let health: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(health["pending_jobs"].as_u64().unwrap() >= 1);

    std::env::remove_var("TEST_VIDEO_API_KEY");
}

// ============================================================================
// Post Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_post_video_not_found() {
    let (plugin, state) = setup_test_plugin().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let post_request = zoey_plugin_x402_video::routes::PostVideoRequest {
        platforms: vec![PlatformTarget::Instagram],
        caption: Some("Test".to_string()),
        hashtags: vec![],
    };

    let request = Request::builder()
        .method("POST")
        .uri("/x402-video/post/nonexistent-job")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&post_request).unwrap()))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_post_video_not_complete() {
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state.clone());

    // Create a processing job (not instant-complete)
    let gen_request = GenerateVideoRequest {
        prompt: "A processing video".to_string(),
        image_url: None,
        caption: None,
        hashtags: vec![],
        platforms: vec![],
        options: Default::default(),
        price_cents: None,
        callback_url: None,
    };

    let x402_header = create_test_x402_header("valid-payment-proof");

    let create_request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .header("X-402", x402_header)
        .body(Body::from(serde_json::to_vec(&gen_request).unwrap()))
        .unwrap();

    let create_response = router.clone().oneshot(create_request).await.unwrap();
    let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let gen_response: GenerateVideoResponse = serde_json::from_slice(&body).unwrap();

    // Try to post immediately (video not complete)
    let post_request = zoey_plugin_x402_video::routes::PostVideoRequest {
        platforms: vec![PlatformTarget::Instagram],
        caption: Some("Test".to_string()),
        hashtags: vec![],
    };

    let post_http_request = Request::builder()
        .method("POST")
        .uri(format!("/x402-video/post/{}", gen_response.job_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&post_request).unwrap()))
        .unwrap();

    let post_response = router.oneshot(post_http_request).await.unwrap();

    // Should fail because video is not complete
    assert_eq!(post_response.status(), StatusCode::BAD_REQUEST);

    std::env::remove_var("TEST_VIDEO_API_KEY");
}

// ============================================================================
// Route Registration Tests
// ============================================================================

#[tokio::test]
async fn test_all_routes_registered() {
    let routes = zoey_plugin_x402_video::routes::get_routes();

    let paths: Vec<&str> = routes.iter().map(|r| r.path.as_str()).collect();

    assert!(paths.contains(&"/x402-video/pricing"));
    assert!(paths.contains(&"/x402-video/generate"));
    assert!(paths.contains(&"/x402-video/status/:job_id"));
    assert!(paths.contains(&"/x402-video/post/:job_id"));
    assert!(paths.contains(&"/x402-video/health"));

    assert_eq!(routes.len(), 5);
}

#[tokio::test]
async fn test_routes_are_public() {
    let routes = zoey_plugin_x402_video::routes::get_routes();

    for route in routes {
        assert!(route.public, "Route {} should be public", route.path);
    }
}

// ============================================================================
// Plugin Integration Tests
// ============================================================================

#[tokio::test]
async fn test_plugin_creates_valid_router() {
    let config = create_test_config();
    let plugin = X402VideoPlugin::new(config);

    let router = plugin.build_router();

    // Test that router responds to health check
    let request = Request::builder()
        .method("GET")
        .uri("/x402-video/health")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_plugin_from_env() {
    // Set up environment
    std::env::set_var("X402_WALLET_ADDRESS", TEST_WALLET_ADDRESS);
    std::env::set_var("X402_PRICE_CENTS", "200");
    std::env::set_var("VIDEO_PROVIDER", "sora");

    let plugin = X402VideoPlugin::from_env();
    let state = plugin.create_route_state();

    assert_eq!(state.config.x402.wallet_address, TEST_WALLET_ADDRESS);
    assert_eq!(state.config.x402.default_price_cents, 200);
    assert_eq!(
        state.config.video_generation.provider,
        zoey_plugin_x402_video::config::VideoProvider::Sora
    );

    // Cleanup
    std::env::remove_var("X402_WALLET_ADDRESS");
    std::env::remove_var("X402_PRICE_CENTS");
    std::env::remove_var("VIDEO_PROVIDER");
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_generate_with_malformed_json() {
    let (plugin, state) = setup_test_plugin().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .body(Body::from("{ invalid json }"))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    // Should return client error for bad JSON
    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn test_generate_with_empty_prompt() {
    let (plugin, state, _, _) = setup_with_mock_servers().await;
    let router = zoey_plugin_x402_video::routes::build_router(state);

    let gen_request = GenerateVideoRequest {
        prompt: "".to_string(),
        image_url: None,
        caption: None,
        hashtags: vec![],
        platforms: vec![],
        options: Default::default(),
        price_cents: None,
        callback_url: None,
    };

    let request = Request::builder()
        .method("POST")
        .uri("/x402-video/generate")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&gen_request).unwrap()))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();

    // Still returns 402 (payment required) even with empty prompt
    // The prompt validation happens after payment
    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);

    std::env::remove_var("TEST_VIDEO_API_KEY");
}

