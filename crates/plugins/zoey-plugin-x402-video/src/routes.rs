//! HTTP Routes for X402 Video Plugin
//!
//! Provides REST API endpoints for external agents to request video generation
//! using x402 payment protocol.

use crate::config::{
    PaymentReceipt, PlatformPostResult, PlatformTarget, VideoOptions, VideoPostRequest,
    VideoPostResult, X402VideoConfig,
};
use crate::services::{
    MultiPlatformPoster, VideoGenRequest, VideoGenStatus, VideoGenerationService,
    X402PaymentService,
};
use axum::{
    extract::{Path, State as AxumState},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use zoey_core::types::Route;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Shared state for route handlers
pub struct X402VideoRouteState {
    pub video_service: Arc<VideoGenerationService>,
    pub payment_service: Arc<X402PaymentService>,
    pub poster: Arc<MultiPlatformPoster>,
    pub config: X402VideoConfig,
    /// Track pending jobs
    pub pending_jobs: Arc<RwLock<std::collections::HashMap<String, JobInfo>>>,
}

/// Information about a pending video generation job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub job_id: String,
    pub status: String,
    pub prompt: String,
    pub created_at: i64,
    pub payment_receipt: Option<PaymentReceipt>,
    pub video_url: Option<String>,
    pub platforms: Vec<PlatformTarget>,
    pub post_results: Option<Vec<PlatformPostResult>>,
    /// Callback URL to notify when job completes (for x402scan integration)
    pub callback_url: Option<String>,
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to generate a video
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateVideoRequest {
    /// Text prompt describing the video
    pub prompt: String,

    /// Optional starting image URL for img2vid
    #[serde(default)]
    pub image_url: Option<String>,

    /// Caption for social media posts
    #[serde(default)]
    pub caption: Option<String>,

    /// Hashtags for social media
    #[serde(default)]
    pub hashtags: Vec<String>,

    /// Target platforms to post to (optional)
    #[serde(default)]
    pub platforms: Vec<PlatformTarget>,

    /// Video generation options
    #[serde(default)]
    pub options: VideoOptions,

    /// Custom price override (in cents, must be >= configured minimum)
    #[serde(default)]
    pub price_cents: Option<u64>,

    /// Callback URL to notify when generation completes (for x402scan integration)
    /// The callback will receive a POST with the JobStatusResponse
    #[serde(default)]
    pub callback_url: Option<String>,
}

/// Response for video generation request (when payment required)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequiredResponse {
    /// Error message
    pub error: String,

    /// Payment requirement details
    pub payment: PaymentDetails,
}

/// Payment requirement details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDetails {
    /// Payment scheme
    pub scheme: String,

    /// Network to pay on
    pub network: String,

    /// Token/asset address
    pub asset: String,

    /// Amount required (in smallest unit)
    pub amount: String,

    /// Recipient address
    pub pay_to: String,

    /// Expiration timestamp
    pub expires_at: i64,

    /// Resource ID for this request
    pub resource_id: String,

    /// Human-readable description
    pub description: String,
}

/// Response for successful video generation start
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateVideoResponse {
    /// Job ID for tracking
    pub job_id: String,

    /// Current status
    pub status: String,

    /// Estimated time to completion (seconds)
    pub estimated_time_secs: Option<u32>,

    /// URL to poll for status
    pub status_url: String,

    /// Video URL when complete (null while processing)
    pub video_url: Option<String>,

    /// Payment receipt
    pub payment_receipt: Option<PaymentReceipt>,
}

/// Response for job status query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatusResponse {
    /// Job ID
    pub job_id: String,

    /// Current status
    pub status: String,

    /// Progress percentage (0-100)
    pub progress: u8,

    /// Video URL when complete
    pub video_url: Option<String>,

    /// Thumbnail URL
    pub thumbnail_url: Option<String>,

    /// Error message if failed
    pub error: Option<String>,

    /// Platform post results (if platforms were specified)
    pub platform_results: Option<Vec<PlatformPostResult>>,

    /// Whether the job is complete (success or failure)
    pub complete: bool,
}

/// Response for pricing info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingResponse {
    /// Base price in USD cents
    pub base_price_cents: u64,

    /// Supported networks
    pub networks: Vec<String>,

    /// Supported tokens
    pub tokens: Vec<String>,

    /// Wallet address for payments
    pub wallet_address: String,

    /// Available video providers
    pub video_provider: String,

    /// Enabled platforms
    pub enabled_platforms: Vec<String>,
}

// ============================================================================
// X402 Scan Compatible Types
// ============================================================================

/// X402 Response for x402scan compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X402Response {
    /// X402 protocol version
    pub x402_version: u32,

    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Accepted payment methods
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepts: Option<Vec<X402Accepts>>,

    /// Payer address if authenticated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payer: Option<String>,
}

/// X402 Accepts definition for x402scan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X402Accepts {
    /// Payment scheme (always "exact" for now)
    pub scheme: String,

    /// Network (e.g., "base")
    pub network: String,

    /// Maximum amount required in smallest unit
    pub max_amount_required: String,

    /// Resource URL being purchased
    pub resource: String,

    /// Human-readable description
    pub description: String,

    /// MIME type of the response
    pub mime_type: String,

    /// Wallet address to pay to
    pub pay_to: String,

    /// Maximum timeout in seconds
    pub max_timeout_seconds: u64,

    /// Token/asset address
    pub asset: String,

    /// Schema describing input/output expectations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<X402OutputSchema>,

    /// Additional custom data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// Output schema for x402scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X402OutputSchema {
    /// Input definition
    pub input: X402InputDef,

    /// Output definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
}

/// Input definition for x402scan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X402InputDef {
    /// Type (always "http")
    #[serde(rename = "type")]
    pub input_type: String,

    /// HTTP method
    pub method: String,

    /// Body type for POST requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_type: Option<String>,

    /// Query parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_params: Option<serde_json::Value>,

    /// Body fields for POST requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_fields: Option<serde_json::Value>,

    /// Header fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_fields: Option<serde_json::Value>,

    /// Whether the endpoint is discoverable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discoverable: Option<bool>,
}

// ============================================================================
// Callback Helper Functions (for x402scan integration)
// ============================================================================

/// Extract base URL from request headers to construct full URLs
/// This ensures status_url uses the same host/proxy as the original request
fn get_base_url_from_headers(_headers: &HeaderMap) -> String {
    // Always use the configured base URL for status endpoints
    // This ensures clients poll the actual service, not the proxy
    std::env::var("X402_VIDEO_BASE_URL")
        .unwrap_or_else(|_| "http://x402.getzoey.ai".to_string())
}

/// Send a callback notification to x402scan or other clients when job status changes
async fn send_status_callback(callback_url: &str, status_response: &JobStatusResponse) {
    let client = reqwest::Client::new();
    
    info!(
        "Sending status callback to {} for job {} (status: {}, video_url: {:?})",
        callback_url, status_response.job_id, status_response.status, status_response.video_url
    );
    
    match client
        .post(callback_url)
        .header("Content-Type", "application/json")
        .json(status_response)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                info!(
                    "Successfully sent callback to {} for job {}",
                    callback_url, status_response.job_id
                );
            } else {
                warn!(
                    "Callback to {} returned status {}: {:?}",
                    callback_url,
                    response.status(),
                    response.text().await.ok()
                );
            }
        }
        Err(e) => {
            error!("Failed to send callback to {}: {}", callback_url, e);
        }
    }
}

/// Background task that polls video status and processes completion automatically
/// This ensures videos are downloaded and uploaded to catbox without requiring client polling
async fn background_video_processor(
    job_id: String,
    video_service: Arc<VideoGenerationService>,
    pending_jobs: Arc<RwLock<std::collections::HashMap<String, JobInfo>>>,
    poster: Arc<MultiPlatformPoster>,
) {
    info!("Starting background processor for job {}", job_id);
    
    let poll_interval = std::time::Duration::from_secs(10);
    let max_polls = 30; // ~5 minutes max
    let mut polls = 0;
    
    loop {
        polls += 1;
        if polls > max_polls {
            warn!("Background processor for job {} exceeded max polls, giving up", job_id);
            break;
        }
        
        // Wait before polling
        tokio::time::sleep(poll_interval).await;
        
        // Check if job still exists in pending jobs
        let job_info = {
            let jobs = pending_jobs.read().await;
            jobs.get(&job_id).cloned()
        };
        
        let job_info = match job_info {
            Some(info) => info,
            None => {
                debug!("Job {} no longer in pending jobs, stopping background processor", job_id);
                break;
            }
        };
        
        // If already completed with video URL, stop polling
        if job_info.video_url.is_some() {
            info!("Job {} already has video URL, background processor complete", job_id);
            break;
        }
        
        // Poll the video service
        debug!("Background poll #{} for job {}", polls, job_id);
        match video_service.get_status(&job_id).await {
            Ok(result) => {
                let is_complete = matches!(
                    result.status,
                    VideoGenStatus::Completed | VideoGenStatus::Failed | VideoGenStatus::Cancelled
                );
                
                // Update the cached job info
                {
                    let mut jobs = pending_jobs.write().await;
                    if let Some(cached) = jobs.get_mut(&job_id) {
                        cached.status = format!("{:?}", result.status);
                        if let Some(ref url) = result.video_url {
                            cached.video_url = Some(url.clone());
                        }
                    }
                }
                
                if result.status == VideoGenStatus::Completed {
                    if let Some(ref video_url) = result.video_url {
                        info!(
                            "Background processor: Job {} completed with video URL: {}",
                            job_id, video_url
                        );
                        
                        // Post to platforms if configured
                        let platforms = job_info.platforms.clone();
                        if !platforms.is_empty() {
                            let caption = "AI Generated Video".to_string();
                            let hashtags: Vec<String> = vec![];
                            
                            let post_results = poster
                                .post_to_platforms(video_url, &caption, &hashtags, &platforms)
                                .await;
                            
                            // Update cache with post results
                            {
                                let mut jobs = pending_jobs.write().await;
                                if let Some(cached) = jobs.get_mut(&job_id) {
                                    cached.post_results = Some(post_results.clone());
                                }
                            }
                        }
                        
                        // Send callback if configured
                        if let Some(ref callback_url) = job_info.callback_url {
                            let status_response = JobStatusResponse {
                                job_id: job_id.clone(),
                                status: "Completed".to_string(),
                                progress: 100,
                                video_url: Some(video_url.clone()),
                                thumbnail_url: result.thumbnail_url.clone(),
                                error: None,
                                platform_results: None,
                                complete: true,
                            };
                            send_status_callback(callback_url, &status_response).await;
                        }
                        
                        break;
                    }
                }
                
                if is_complete {
                    info!(
                        "Background processor: Job {} finished with status {:?}",
                        job_id, result.status
                    );
                    
                    // Send callback for failed/cancelled jobs
                    if let Some(ref callback_url) = job_info.callback_url {
                        let status_response = JobStatusResponse {
                            job_id: job_id.clone(),
                            status: format!("{:?}", result.status),
                            progress: result.progress,
                            video_url: result.video_url.clone(),
                            thumbnail_url: result.thumbnail_url.clone(),
                            error: result.error.clone(),
                            platform_results: None,
                            complete: true,
                        };
                        send_status_callback(callback_url, &status_response).await;
                    }
                    break;
                }
            }
            Err(e) => {
                warn!("Background poll for job {} failed: {}", job_id, e);
                // Continue polling on error
            }
        }
    }
    
    info!("Background processor for job {} finished", job_id);
}

// ============================================================================
// Route Handlers
// ============================================================================

/// GET / - Default route returning x402scan compatible service info
async fn get_root(
    AxumState(state): AxumState<Arc<X402VideoRouteState>>,
) -> impl IntoResponse {
    // Get the base URL from environment or use default
    let base_url = std::env::var("X402_VIDEO_BASE_URL")
        .unwrap_or_else(|_| "https://x402.getzoey.ai".to_string());

    // Calculate price in USDC units (6 decimals)
    // e.g., 100 cents = $1.00 = 1_000_000 USDC units
    let price_usdc_units = (state.config.x402.default_price_cents as u64) * 10_000;

    // Get USDC token address for the network
    let usdc_address = get_token_address_for_network(
        &state.config.x402.supported_networks.first().cloned().unwrap_or_else(|| "base".to_string())
    );

    // Common output schema for both payment options
    let network = state.config.x402.supported_networks.first().cloned().unwrap_or_else(|| "base".to_string());
    let eth_amount_wei = "330000000000000"; // ~$1 in ETH at $3000/ETH
    
    let output_schema = X402OutputSchema {
        input: X402InputDef {
            input_type: "http".to_string(),
            method: "POST".to_string(),
            body_type: Some("json".to_string()),
            query_params: None,
            body_fields: Some(serde_json::json!({
                "prompt": {
                    "type": "string",
                    "required": true,
                    "description": "Text description of the video to generate. NOTE: Video generation takes 1-3 minutes. Keep window open until complete."
                },
                "image_url": {
                    "type": "string",
                    "required": false,
                    "description": "Optional starting image URL for image-to-video generation"
                },
                "caption": {
                    "type": "string",
                    "required": false,
                    "description": "Caption for social media posts"
                },
                "hashtags": {
                    "type": "array",
                    "required": false,
                    "description": "Hashtags for social media posts"
                },
                "platforms": {
                    "type": "array",
                    "required": false,
                    "description": "Target platforms to post to (instagram, tiktok, snapchat)",
                    "enum": ["instagram", "tiktok", "snapchat", "all"]
                },
                "options": {
                    "type": "object",
                    "required": false,
                    "description": "Video generation options",
                    "properties": {
                        "duration_secs": {
                            "type": "number",
                            "description": "Video duration in seconds (Sora supports 4, 8, or 12 seconds)"
                        },
                        "resolution": {
                            "type": "string",
                            "description": "Video resolution (Sora supports 1280x720 landscape or 720x1280 portrait)",
                            "enum": ["HD720p", "Vertical720p"]
                        },
                        "aspect_ratio": {
                            "type": "string",
                            "description": "Aspect ratio (e.g., '16:9', '9:16', '1:1')"
                        },
                        "style": {
                            "type": "string",
                            "description": "Style preset or model variant"
                        }
                    }
                }
            })),
            header_fields: Some(serde_json::json!({
                "X-402": {
                    "type": "string",
                    "required": true,
                    "description": "X402 payment proof header"
                }
            })),
            discoverable: Some(true),
        },
        output: Some(serde_json::json!({
            "job_id": {
                "type": "string",
                "description": "Unique job identifier for tracking"
            },
            "status": {
                "type": "string",
                "description": "Current job status (Processing, Completed, Failed)"
            },
            "estimated_time_secs": {
                "type": "number",
                "description": "Estimated time to completion in seconds"
            },
            "status_url": {
                "type": "string",
                "description": "URL to poll for job status"
            },
            "payment_receipt": {
                "type": "object",
                "description": "Payment receipt details"
            }
        })),
    };

    let accepts = vec![
        // USDC payment option
        X402Accepts {
            scheme: "exact".to_string(),
            network: network.clone(),
            max_amount_required: price_usdc_units.to_string(),
            resource: format!("{}/x402-video/generate", base_url),
            description: "AI Video Generation using Sora - Pay with USDC".to_string(),
            mime_type: "application/json".to_string(),
            // Route payments through PayAI facilitator for x402scan tracking
            pay_to: state.config.x402.facilitator_pay_to_address.clone(),
            max_timeout_seconds: state.config.x402.payment_timeout_secs,
            asset: usdc_address,
            output_schema: Some(output_schema.clone()),
            extra: Some(serde_json::json!({
                "name": "USD Coin",
                "symbol": "USDC",
                "decimals": 6,
                "version": "2",
                // Settlement address - PayAI will forward payments here
                "settleTo": state.config.x402.wallet_address.clone(),
                "provider": format!("{:?}", state.config.video_generation.provider),
                "enabled_platforms": state.poster.enabled_platforms(),
                "max_duration_secs": state.config.video_generation.max_duration_secs,
                "default_duration_secs": state.config.video_generation.default_duration_secs
            })),
        },
        // Native ETH payment option
        X402Accepts {
            scheme: "exact".to_string(),
            network: network.clone(),
            max_amount_required: eth_amount_wei.to_string(),
            resource: format!("{}/x402-video/generate", base_url),
            description: "AI Video Generation using Sora - Pay with ETH".to_string(),
            mime_type: "application/json".to_string(),
            // Route payments through PayAI facilitator for x402scan tracking
            pay_to: state.config.x402.facilitator_pay_to_address.clone(),
            max_timeout_seconds: state.config.x402.payment_timeout_secs,
            asset: "0x0000000000000000000000000000000000000000".to_string(), // Native ETH
            output_schema: Some(output_schema),
            extra: Some(serde_json::json!({
                "name": "Ethereum",
                "symbol": "ETH",
                "decimals": 18,
                "version": "2",
                // Settlement address - PayAI will forward payments here
                "settleTo": state.config.x402.wallet_address.clone(),
                "provider": format!("{:?}", state.config.video_generation.provider),
                "enabled_platforms": state.poster.enabled_platforms(),
                "max_duration_secs": state.config.video_generation.max_duration_secs,
                "default_duration_secs": state.config.video_generation.default_duration_secs
            })),
        },
    ];

    Json(X402Response {
        x402_version: 1,
        error: None,
        accepts: Some(accepts),
        payer: None,
    })
}

/// Get USDC token address for a network
fn get_token_address_for_network(network: &str) -> String {
    match network {
        "base" => "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".to_string(),
        "ethereum" => "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
        "polygon" => "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174".to_string(),
        "arbitrum" => "0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8".to_string(),
        _ => "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".to_string(), // Default to Base USDC
    }
}

/// GET /x402-video/pricing - Get pricing and configuration info
async fn get_pricing(
    AxumState(state): AxumState<Arc<X402VideoRouteState>>,
) -> impl IntoResponse {
    let enabled_platforms = state.poster.enabled_platforms();

    Json(PricingResponse {
        base_price_cents: state.config.x402.default_price_cents,
        networks: state.config.x402.supported_networks.clone(),
        tokens: state.config.x402.supported_tokens.clone(),
        wallet_address: state.config.x402.wallet_address.clone(),
        video_provider: format!("{:?}", state.config.video_generation.provider),
        enabled_platforms: enabled_platforms.iter().map(|s| s.to_string()).collect(),
    })
}

/// POST /x402-video/generate - Generate a video (requires x402 payment)
async fn generate_video(
    headers: HeaderMap,
    AxumState(state): AxumState<Arc<X402VideoRouteState>>,
    body: axum::body::Bytes,
) -> Response {
    // Generate resource ID for this request
    let resource_id = format!("video-{}", uuid::Uuid::new_v4());

    // Check for x402 payment header FIRST (before parsing body)
    // Support multiple header names that x402scan or other clients might use
    let payment_header = headers
        .get("X-402")
        .or_else(|| headers.get("x-402"))
        .or_else(|| headers.get("X-PAYMENT"))
        .or_else(|| headers.get("x-payment"))
        .or_else(|| headers.get("Authorization"))
        .and_then(|v| v.to_str().ok());
    
    // Log headers for debugging (DEBUG level - not shown by default)
    tracing::debug!("x402-video: Received generate request");
    tracing::debug!("x402-video: Payment header present: {:?}", payment_header.is_some());

    // If no payment header, return 402 Payment Required immediately
    // This allows x402scan to validate the endpoint without a valid request body
    if payment_header.is_none() {
        let price_cents = state.config.x402.default_price_cents;
        // Calculate price in USDC units (6 decimals)
        let price_usdc_units = (price_cents as u64) * 10_000;
        
        // Get the base URL from environment or use default
        let base_url = std::env::var("X402_VIDEO_BASE_URL")
            .unwrap_or_else(|_| "https://x402.getzoey.ai".to_string());
        
        // Get USDC token address for the network
        let network = state.config.x402.supported_networks.first()
            .cloned()
            .unwrap_or_else(|| "base".to_string());
        let usdc_address = get_token_address_for_network(&network);

        // Build x402scan-compatible 402 response with multiple payment options
        // Calculate ETH price (~$1.00 worth of ETH, assuming ~$3000/ETH = 0.00033 ETH = 330000000000000 wei)
        let eth_amount_wei = "330000000000000"; // ~$1 in ETH at $3000/ETH
        
        // Common output schema for both payment options
        let output_schema = X402OutputSchema {
            input: X402InputDef {
                input_type: "http".to_string(),
                method: "POST".to_string(),
                body_type: Some("json".to_string()),
                query_params: None,
                body_fields: Some(serde_json::json!({
                    "prompt": {
                        "type": "string",
                        "required": true,
                        "description": "Text description of the video to generate"
                    },
                    "image_url": {
                        "type": "string",
                        "required": false,
                        "description": "Optional starting image URL for image-to-video generation"
                    },
                    "options": {
                        "type": "object",
                        "required": false,
                        "description": "Video generation options",
                        "properties": {
                            "duration_secs": {
                                "type": "number",
                                "description": "Video duration in seconds (Sora supports 4, 8, or 12 seconds)"
                            },
                            "resolution": {
                                "type": "string",
                                "description": "Video resolution (Sora supports 1280x720 landscape or 720x1280 portrait)",
                                "enum": ["HD720p", "Vertical720p"]
                            }
                        }
                    }
                })),
                header_fields: Some(serde_json::json!({
                    "X-402": {
                        "type": "string",
                        "required": true,
                        "description": "X402 payment proof header"
                    }
                })),
                discoverable: Some(true),
            },
            output: Some(serde_json::json!({
                "job_id": { "type": "string", "description": "Unique job identifier" },
                "status": { "type": "string", "description": "Job status" },
                "status_url": { "type": "string", "description": "URL to poll for status" }
            })),
        };

        let accepts = vec![
            // USDC payment option
            X402Accepts {
                scheme: "exact".to_string(),
                network: network.clone(),
                max_amount_required: price_usdc_units.to_string(),
                resource: format!("{}/x402-video/generate", base_url),
                description: "AI Video Generation using Sora - Pay with USDC".to_string(),
                mime_type: "application/json".to_string(),
                // Route payments through PayAI facilitator for x402scan tracking
                pay_to: state.config.x402.facilitator_pay_to_address.clone(),
                max_timeout_seconds: state.config.x402.payment_timeout_secs,
                asset: usdc_address.clone(),
                output_schema: Some(output_schema.clone()),
                extra: Some(serde_json::json!({
                    "name": "USD Coin",
                    "symbol": "USDC",
                    "decimals": 6,
                    "version": "2",
                    // Settlement address - PayAI will forward payments here
                    "settleTo": state.config.x402.wallet_address.clone(),
                    "provider": format!("{:?}", state.config.video_generation.provider)
                })),
            },
            // Native ETH payment option
            X402Accepts {
                scheme: "exact".to_string(),
                network: network.clone(),
                max_amount_required: eth_amount_wei.to_string(),
                resource: format!("{}/x402-video/generate", base_url),
                description: "AI Video Generation using Sora - Pay with ETH".to_string(),
                mime_type: "application/json".to_string(),
                // Route payments through PayAI facilitator for x402scan tracking
                pay_to: state.config.x402.facilitator_pay_to_address.clone(),
                max_timeout_seconds: state.config.x402.payment_timeout_secs,
                asset: "0x0000000000000000000000000000000000000000".to_string(), // Native ETH
                output_schema: Some(output_schema),
                extra: Some(serde_json::json!({
                    "name": "Ethereum",
                    "symbol": "ETH",
                    "decimals": 18,
                    "version": "2",
                    // Settlement address - PayAI will forward payments here
                    "settleTo": state.config.x402.wallet_address.clone(),
                    "provider": format!("{:?}", state.config.video_generation.provider)
                })),
            },
        ];

        let x402_response = X402Response {
            x402_version: 1,
            error: Some("X-PAYMENT header is required".to_string()),
            accepts: Some(accepts),
            payer: None,
        };

        let mut response = (
            StatusCode::PAYMENT_REQUIRED,
            Json(x402_response),
        )
            .into_response();

        // Add WWW-Authenticate header
        if let Ok(requirement) = state
            .payment_service
            .create_payment_requirement(
                &resource_id,
                Some(price_cents),
                Some("Generate AI video with Sora (takes 1-3 minutes, keep window open)".to_string()),
            )
            .await
        {
            let headers_vec = state.payment_service.format_402_headers(&requirement);
            for (key, value) in headers_vec {
                if let Ok(name) = key.parse::<header::HeaderName>() {
                    if let Ok(val) = value.parse::<header::HeaderValue>() {
                        response.headers_mut().insert(name, val);
                    }
                }
            }
        }

        return response;
    }

    // Now parse the request body (payment header exists)
    let request: GenerateVideoRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid request body: {}", e)
                })),
            )
                .into_response();
        }
    };

    // We have a payment header (guaranteed by early return above), verify it
    let header = payment_header.unwrap(); // Safe: we returned early if None
    
    // Verify payment
    match state
        .payment_service
        .verify_payment(header, Some(&resource_id))
        .await
    {
        Ok(verification) if verification.valid => {
            info!(
                "Payment verified for resource {}: {:?}",
                resource_id, verification.receipt
            );

            // Start video generation
            let gen_request = VideoGenRequest {
                prompt: request.prompt.clone(),
                image_url: request.image_url.clone(),
                duration_secs: request
                    .options
                    .duration_secs
                    .unwrap_or(state.config.video_generation.default_duration_secs),
                resolution: request
                    .options
                    .resolution
                    .unwrap_or(state.config.video_generation.default_resolution),
                aspect_ratio: request.options.aspect_ratio.clone(),
                style: request.options.style.clone(),
                seed: request.options.seed,
                negative_prompt: None,
                guidance_scale: None,
            };

            match state.video_service.generate(gen_request).await {
                Ok(result) => {
                    let job_id = result.job_id.clone();
                    
                    // Store job info - return immediately, client will poll status
                    let job_info = JobInfo {
                        job_id: job_id.clone(),
                        status: format!("{:?}", result.status),
                        prompt: request.prompt.clone(),
                        created_at: chrono::Utc::now().timestamp(),
                        payment_receipt: verification.receipt.clone(),
                        video_url: result.video_url.clone(),
                        platforms: request.platforms.clone(),
                        post_results: None,
                        callback_url: request.callback_url.clone(),
                    };

                    state
                        .pending_jobs
                        .write()
                        .await
                        .insert(job_id.clone(), job_info);

                    // Spawn background processor to automatically poll and process video
                    // This ensures video is downloaded/uploaded without requiring client polling
                    {
                        let job_id_clone = job_id.clone();
                        let video_service = state.video_service.clone();
                        let pending_jobs = state.pending_jobs.clone();
                        let poster = state.poster.clone();
                        
                        tokio::spawn(async move {
                            background_video_processor(
                                job_id_clone,
                                video_service,
                                pending_jobs,
                                poster,
                            ).await;
                        });
                    }

                    // Build full status_url using the request's origin (preserves proxy URL for x402scan)
                    let base_url = get_base_url_from_headers(&headers);
                    let status_url = format!("{}/x402-video/status/{}", base_url, job_id);
                    
                    info!(
                        "Video generation started for job {}: poll {} for updates (takes 1-3 min)",
                        job_id, status_url
                    );

                    // Return immediately - client can poll status_url OR wait for automatic processing
                    // NOTE: Video generation takes 1-3 minutes. Poll status endpoint until complete.
                    (
                        StatusCode::ACCEPTED,
                        Json(GenerateVideoResponse {
                            job_id: job_id.clone(),
                            status: format!("{:?}", result.status),
                            estimated_time_secs: Some(120), // ~2 minutes typical
                            status_url,
                            video_url: None, // Will be populated when polling status
                            payment_receipt: verification.receipt,
                        }),
                    )
                        .into_response()
                }
                Err(e) => {
                    error!("Video generation failed: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error": format!("Video generation failed: {}", e)
                        })),
                    )
                        .into_response()
                }
            }
        }
        Ok(verification) => {
            warn!("Payment verification failed: {}", verification.message);
            (
                StatusCode::PAYMENT_REQUIRED,
                Json(serde_json::json!({
                    "error": format!("Payment verification failed: {}", verification.message)
                })),
            )
                .into_response()
        }
        Err(e) => {
            error!("Payment verification error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Payment verification error: {}", e)
                })),
            )
                .into_response()
        }
    }
}

/// GET /x402-video/status/:job_id - Get job status
async fn get_job_status(
    Path(job_id): Path<String>,
    AxumState(state): AxumState<Arc<X402VideoRouteState>>,
) -> Response {
    // Check local cache first - we only track jobs we created
    let cached_job = state.pending_jobs.read().await.get(&job_id).cloned();

    // If job is not in our cache, return 404 - we don't know about this job
    let cached_job = match cached_job {
        Some(job) => job,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("Job not found: {}", job_id)
                })),
            )
                .into_response();
        }
    };

    // If we have a cached completed job with video URL, return it directly
    if cached_job.video_url.is_some() {
        return (
            StatusCode::OK,
            Json(JobStatusResponse {
                job_id: cached_job.job_id.clone(),
                status: cached_job.status.clone(),
                progress: 100,
                video_url: cached_job.video_url.clone(),
                thumbnail_url: None,
                error: None,
                platform_results: cached_job.post_results.clone(),
                complete: true,
            }),
        )
            .into_response();
    }

    // Poll the video service for current status
    match state.video_service.get_status(&job_id).await {
        Ok(result) => {
            let complete = matches!(
                result.status,
                VideoGenStatus::Completed | VideoGenStatus::Failed | VideoGenStatus::Cancelled
            );

            // If just completed and we have pending platforms, post to them
            if result.status == VideoGenStatus::Completed {
                if let Some(ref video_url) = result.video_url {
                    let mut post_results_opt = None;
                    
                    if !cached_job.platforms.is_empty() && cached_job.post_results.is_none() {
                        let caption = "AI Generated Video".to_string();
                        let hashtags: Vec<String> = vec![];

                        let post_results = state
                            .poster
                            .post_to_platforms(video_url, &caption, &hashtags, &cached_job.platforms)
                            .await;
                        post_results_opt = Some(post_results.clone());

                        // Update cache
                        if let Some(cached) =
                            state.pending_jobs.write().await.get_mut(&job_id)
                        {
                            cached.video_url = Some(video_url.clone());
                            cached.status = "Completed".to_string();
                            cached.post_results = Some(post_results);
                        }
                    }

                    // Build response
                    let status_response = JobStatusResponse {
                        job_id: result.job_id.clone(),
                        status: format!("{:?}", result.status),
                        progress: result.progress,
                        video_url: result.video_url.clone(),
                        thumbnail_url: result.thumbnail_url.clone(),
                        error: result.error.clone(),
                        platform_results: post_results_opt.clone().or_else(|| cached_job.post_results.clone()),
                        complete,
                    };

                    // Send callback to x402scan if configured
                    if let Some(ref callback_url) = cached_job.callback_url {
                        // Spawn callback in background so we don't block the response
                        let callback_url = callback_url.clone();
                        let status_response_clone = status_response.clone();
                        tokio::spawn(async move {
                            send_status_callback(&callback_url, &status_response_clone).await;
                        });
                    }

                    return (StatusCode::OK, Json(status_response)).into_response();
                }
            }

            // Update cache
            if let Some(cached) = state.pending_jobs.write().await.get_mut(&job_id) {
                cached.video_url = result.video_url.clone();
                cached.status = format!("{:?}", result.status);
            }

            let status_response = JobStatusResponse {
                job_id: result.job_id,
                status: format!("{:?}", result.status),
                progress: result.progress,
                video_url: result.video_url,
                thumbnail_url: result.thumbnail_url,
                error: result.error,
                platform_results: cached_job.post_results.clone(),
                complete,
            };

            // Send callback when job completes (success or failure)
            if complete {
                if let Some(ref callback_url) = cached_job.callback_url {
                    let callback_url = callback_url.clone();
                    let status_response_clone = status_response.clone();
                    tokio::spawn(async move {
                        send_status_callback(&callback_url, &status_response_clone).await;
                    });
                }
            }

            (StatusCode::OK, Json(status_response)).into_response()
        }
        Err(e) => {
            // Job exists in cache but provider returned error - return cached state with error
            (
                StatusCode::OK,
                Json(JobStatusResponse {
                    job_id: cached_job.job_id.clone(),
                    status: cached_job.status.clone(),
                    progress: 0,
                    video_url: cached_job.video_url.clone(),
                    thumbnail_url: None,
                    error: Some(format!("Status check failed: {}", e)),
                    platform_results: cached_job.post_results.clone(),
                    complete: false,
                }),
            )
                .into_response()
        }
    }
}

/// POST /x402-video/post/:job_id - Post a completed video to platforms
async fn post_video(
    Path(job_id): Path<String>,
    AxumState(state): AxumState<Arc<X402VideoRouteState>>,
    Json(request): Json<PostVideoRequest>,
) -> Response {
    // Get job info
    let job = state.pending_jobs.read().await.get(&job_id).cloned();

    match job {
        Some(job) if job.video_url.is_some() => {
            let video_url = job.video_url.unwrap();
            let caption = request
                .caption
                .unwrap_or_else(|| "AI Generated Video".to_string());

            let results = state
                .poster
                .post_to_platforms(&video_url, &caption, &request.hashtags, &request.platforms)
                .await;

            // Update cache
            if let Some(cached) = state.pending_jobs.write().await.get_mut(&job_id) {
                cached.post_results = Some(results.clone());
            }

            let success_count = results.iter().filter(|r| r.success).count();

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": success_count > 0,
                    "posted": success_count,
                    "total": results.len(),
                    "results": results
                })),
            )
                .into_response()
        }
        Some(_) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Video generation not yet complete"
            })),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Job not found: {}", job_id)
            })),
        )
            .into_response(),
    }
}

/// Request to post a video to platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostVideoRequest {
    /// Target platforms
    pub platforms: Vec<PlatformTarget>,

    /// Caption for the post
    #[serde(default)]
    pub caption: Option<String>,

    /// Hashtags
    #[serde(default)]
    pub hashtags: Vec<String>,
}

/// GET /x402-video/health - Health check
async fn health_check(
    AxumState(state): AxumState<Arc<X402VideoRouteState>>,
) -> impl IntoResponse {
    let enabled_platforms = state.poster.enabled_platforms();

    Json(serde_json::json!({
        "status": "healthy",
        "plugin": "x402-video",
        "wallet_configured": !state.config.x402.wallet_address.is_empty(),
        "video_provider": format!("{:?}", state.config.video_generation.provider),
        "enabled_platforms": enabled_platforms,
        "pending_jobs": state.pending_jobs.read().await.len()
    }))
}

// ============================================================================
// Router Builder
// ============================================================================

/// Build the router for x402-video plugin routes
pub fn build_router(state: Arc<X402VideoRouteState>) -> Router {
    Router::new()
        .route("/", get(get_root))
        .route("/x402-video/pricing", get(get_pricing))
        .route("/x402-video/generate", post(generate_video))
        .route("/x402-video/status/:job_id", get(get_job_status))
        .route("/x402-video/post/:job_id", post(post_video))
        .route("/x402-video/health", get(health_check))
        .with_state(state)
}

/// Convert to Zoey Route definitions
pub fn get_routes() -> Vec<Route> {
    vec![
        Route {
            route_type: zoey_core::types::RouteType::Get,
            path: "/".to_string(),
            file_path: None,
            public: true,
            name: Some("X402 Video Service Info".to_string()),
            handler: None,
            is_multipart: false,
        },
        Route {
            route_type: zoey_core::types::RouteType::Get,
            path: "/x402-video/pricing".to_string(),
            file_path: None,
            public: true,
            name: Some("X402 Video Pricing".to_string()),
            handler: None,
            is_multipart: false,
        },
        Route {
            route_type: zoey_core::types::RouteType::Post,
            path: "/x402-video/generate".to_string(),
            file_path: None,
            public: true,
            name: Some("X402 Video Generate".to_string()),
            handler: None,
            is_multipart: false,
        },
        Route {
            route_type: zoey_core::types::RouteType::Get,
            path: "/x402-video/status/:job_id".to_string(),
            file_path: None,
            public: true,
            name: Some("X402 Video Status".to_string()),
            handler: None,
            is_multipart: false,
        },
        Route {
            route_type: zoey_core::types::RouteType::Post,
            path: "/x402-video/post/:job_id".to_string(),
            file_path: None,
            public: true,
            name: Some("X402 Video Post".to_string()),
            handler: None,
            is_multipart: false,
        },
        Route {
            route_type: zoey_core::types::RouteType::Get,
            path: "/x402-video/health".to_string(),
            file_path: None,
            public: true,
            name: Some("X402 Video Health".to_string()),
            handler: None,
            is_multipart: false,
        },
    ]
}

/// Helper to truncate strings
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("this is long", 7), "this is...");
    }

    #[test]
    fn test_routes_defined() {
        let routes = get_routes();
        assert_eq!(routes.len(), 6);

        let paths: Vec<&str> = routes.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"/"));
        assert!(paths.contains(&"/x402-video/pricing"));
        assert!(paths.contains(&"/x402-video/generate"));
        assert!(paths.contains(&"/x402-video/status/:job_id"));
    }
}


