//! Video Generation Service
//!
//! Supports multiple AI video generation providers (Replicate, Runway, Pika, Luma)

use crate::config::{VideoGenerationConfig, VideoOptions, VideoProvider, VideoResolution};
use async_trait::async_trait;
use zoey_core::{error::ZoeyError, types::*, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Video generation error types
#[derive(Debug, thiserror::Error)]
pub enum VideoGenError {
    #[error("Generation failed: {0}")]
    GenerationFailed(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Invalid prompt: {0}")]
    InvalidPrompt(String),

    #[error("Timeout waiting for video generation")]
    Timeout,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("API key not configured for provider: {0}")]
    MissingApiKey(String),

    #[error("Upload failed: {0}")]
    UploadFailed(String),
}

/// Video generation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGenRequest {
    /// Text prompt describing the video
    pub prompt: String,

    /// Optional starting image URL for img2vid
    pub image_url: Option<String>,

    /// Duration in seconds
    pub duration_secs: u32,

    /// Target resolution
    pub resolution: VideoResolution,

    /// Aspect ratio override (e.g., "16:9", "9:16")
    pub aspect_ratio: Option<String>,

    /// Style preset or model variant
    pub style: Option<String>,

    /// Seed for reproducibility
    pub seed: Option<i64>,

    /// Negative prompt (what to avoid)
    pub negative_prompt: Option<String>,

    /// CFG scale / guidance scale
    pub guidance_scale: Option<f32>,
}

/// Video generation status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoGenStatus {
    /// Job queued
    Queued,
    /// Generation in progress
    Processing,
    /// Generation complete
    Completed,
    /// Generation failed
    Failed,
    /// Job cancelled
    Cancelled,
}

/// Video generation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGenResult {
    /// Generation job ID
    pub job_id: String,

    /// Current status
    pub status: VideoGenStatus,

    /// Video URL when complete
    pub video_url: Option<String>,

    /// Thumbnail URL
    pub thumbnail_url: Option<String>,

    /// Duration of generated video
    pub duration_secs: Option<f32>,

    /// Generation progress (0-100)
    pub progress: u8,

    /// Error message if failed
    pub error: Option<String>,

    /// Provider-specific metadata
    pub metadata: serde_json::Value,
}

/// Internal state for video generation service
struct VideoServiceState {
    config: VideoGenerationConfig,
    client: Client,
    api_key: Option<String>,
}

/// Cache for downloaded video bytes awaiting upload
struct PendingUpload {
    video_bytes: Vec<u8>,
    downloaded_at: std::time::Instant,
}

/// Video Generation Service
pub struct VideoGenerationService {
    state: Arc<RwLock<VideoServiceState>>,
    running: Arc<RwLock<bool>>,
    /// Cache of downloaded videos pending upload (job_id -> video bytes)
    pending_uploads: Arc<RwLock<std::collections::HashMap<String, PendingUpload>>>,
}

impl VideoGenerationService {
    /// Create a new video generation service
    pub fn new(config: VideoGenerationConfig) -> Self {
        let state = VideoServiceState {
            config,
            client: Client::new(),
            api_key: None,
        };

        Self {
            state: Arc::new(RwLock::new(state)),
            running: Arc::new(RwLock::new(false)),
            pending_uploads: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Set the API key for video generation
    pub async fn set_api_key(&self, key: String) {
        let mut state = self.state.write().await;
        state.api_key = Some(key);
        info!("API key configured for video generation provider: {:?}", state.config.provider);
    }

    /// Generate a video from a prompt
    pub async fn generate(&self, request: VideoGenRequest) -> Result<VideoGenResult> {
        let state = self.state.read().await;

        // Check API key
        let api_key = state.api_key.as_ref().ok_or_else(|| {
            ZoeyError::config(format!(
                "API key not configured for provider: {:?}",
                state.config.provider
            ))
        })?;

        info!(
            "Generating video with {:?}: {}",
            state.config.provider,
            truncate_prompt(&request.prompt, 50)
        );

        match state.config.provider {
            VideoProvider::Replicate => {
                self.generate_replicate(&request, api_key, &state).await
            }
            VideoProvider::Runway => {
                self.generate_runway(&request, api_key, &state).await
            }
            VideoProvider::Pika => {
                self.generate_pika(&request, api_key, &state).await
            }
            VideoProvider::Luma => {
                self.generate_luma(&request, api_key, &state).await
            }
            VideoProvider::Sora => {
                self.generate_sora(&request, api_key, &state).await
            }
            VideoProvider::Custom => {
                self.generate_custom(&request, api_key, &state).await
            }
        }
    }

    /// Poll for video generation status
    pub async fn get_status(&self, job_id: &str) -> Result<VideoGenResult> {
        let state = self.state.read().await;

        let api_key = state.api_key.as_ref().ok_or_else(|| {
            ZoeyError::config(format!(
                "API key not configured for provider: {:?}",
                state.config.provider
            ))
        })?;

        match state.config.provider {
            VideoProvider::Replicate => {
                self.poll_replicate(job_id, api_key, &state).await
            }
            VideoProvider::Runway => {
                self.poll_runway(job_id, api_key, &state).await
            }
            VideoProvider::Pika => {
                self.poll_pika(job_id, api_key, &state).await
            }
            VideoProvider::Luma => {
                self.poll_luma(job_id, api_key, &state).await
            }
            VideoProvider::Sora => {
                self.poll_and_upload_sora(job_id, api_key, &state).await
            }
            VideoProvider::Custom => {
                self.poll_custom(job_id, api_key, &state).await
            }
        }
    }

    /// Wait for video generation to complete with polling
    pub async fn wait_for_completion(
        &self,
        job_id: &str,
        timeout_secs: u64,
        poll_interval_secs: u64,
    ) -> Result<VideoGenResult> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let interval = std::time::Duration::from_secs(poll_interval_secs);

        loop {
            if start.elapsed() > timeout {
                return Err(ZoeyError::timeout("Video generation timed out"));
            }

            let result = self.get_status(job_id).await?;

            match result.status {
                VideoGenStatus::Completed => return Ok(result),
                VideoGenStatus::Failed => {
                    return Err(ZoeyError::service(format!(
                        "Video generation failed: {}",
                        result.error.unwrap_or_else(|| "Unknown error".to_string())
                    )));
                }
                VideoGenStatus::Cancelled => {
                    return Err(ZoeyError::service("Video generation job cancelled"));
                }
                _ => {
                    debug!(
                        "Video generation {} in progress: {}%",
                        job_id, result.progress
                    );
                    tokio::time::sleep(interval).await;
                }
            }
        }
    }

    // ========================================================================
    // Provider-specific implementations
    // ========================================================================

    /// Generate video using Replicate API
    async fn generate_replicate(
        &self,
        request: &VideoGenRequest,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        // Replicate uses various video models - default to Stable Video Diffusion
        let model = request
            .style
            .clone()
            .unwrap_or_else(|| "stability-ai/stable-video-diffusion:3f0457e4619daac51203dedb472816fd4af51f3149fa7a9e0b5ffcf1b8172438".to_string());

        let (width, height) = request.resolution.dimensions();

        let mut input = serde_json::json!({
            "prompt": request.prompt,
            "width": width,
            "height": height,
            "num_frames": (request.duration_secs * 24) as i32, // Assume 24fps
        });

        // Add optional parameters
        if let Some(ref img_url) = request.image_url {
            input["image"] = serde_json::json!(img_url);
        }
        if let Some(seed) = request.seed {
            input["seed"] = serde_json::json!(seed);
        }
        if let Some(ref neg) = request.negative_prompt {
            input["negative_prompt"] = serde_json::json!(neg);
        }
        if let Some(guidance) = request.guidance_scale {
            input["guidance_scale"] = serde_json::json!(guidance);
        }

        let response = state
            .client
            .post(format!("{}/predictions", state.config.api_url))
            .header("Authorization", format!("Token {}", api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "version": model.split(':').last().unwrap_or(&model),
                "input": input,
                "webhook": state.config.webhook_url,
            }))
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ZoeyError::service(format!(
                "Replicate API error: {}",
                error_text
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let job_id = result["id"].as_str().unwrap_or_default().to_string();
        let status = match result["status"].as_str() {
            Some("starting") | Some("processing") => VideoGenStatus::Processing,
            Some("succeeded") => VideoGenStatus::Completed,
            Some("failed") => VideoGenStatus::Failed,
            _ => VideoGenStatus::Queued,
        };

        Ok(VideoGenResult {
            job_id,
            status,
            video_url: result["output"].as_str().map(|s| s.to_string()),
            thumbnail_url: None,
            duration_secs: Some(request.duration_secs as f32),
            progress: 0,
            error: result["error"].as_str().map(|s| s.to_string()),
            metadata: result,
        })
    }

    /// Poll Replicate for status
    async fn poll_replicate(
        &self,
        job_id: &str,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let response = state
            .client
            .get(format!("{}/predictions/{}", state.config.api_url, job_id))
            .header("Authorization", format!("Token {}", api_key))
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let status = match result["status"].as_str() {
            Some("starting") => VideoGenStatus::Queued,
            Some("processing") => VideoGenStatus::Processing,
            Some("succeeded") => VideoGenStatus::Completed,
            Some("failed") => VideoGenStatus::Failed,
            Some("canceled") => VideoGenStatus::Cancelled,
            _ => VideoGenStatus::Processing,
        };

        // Extract video URL from output (could be string or array)
        let video_url = if result["output"].is_string() {
            result["output"].as_str().map(|s| s.to_string())
        } else if result["output"].is_array() {
            result["output"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        // Calculate progress from logs if available
        let progress = if status == VideoGenStatus::Completed {
            100
        } else {
            result["logs"]
                .as_str()
                .and_then(|logs| extract_progress(logs))
                .unwrap_or(0)
        };

        Ok(VideoGenResult {
            job_id: job_id.to_string(),
            status,
            video_url,
            thumbnail_url: None,
            duration_secs: None,
            progress,
            error: result["error"].as_str().map(|s| s.to_string()),
            metadata: result,
        })
    }

    /// Generate video using Runway API
    async fn generate_runway(
        &self,
        request: &VideoGenRequest,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let (width, height) = request.resolution.dimensions();

        let mut body = serde_json::json!({
            "text_prompt": request.prompt,
            "resolution": format!("{}x{}", width, height),
            "duration": request.duration_secs,
        });

        if let Some(ref img_url) = request.image_url {
            body["init_image"] = serde_json::json!(img_url);
        }
        if let Some(seed) = request.seed {
            body["seed"] = serde_json::json!(seed);
        }

        let response = state
            .client
            .post(format!("{}/generate", state.config.api_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ZoeyError::service(format!(
                "Runway API error: {}",
                error_text
            )));
        }

        let result: serde_json::Value = response.json().await?;

        Ok(VideoGenResult {
            job_id: result["id"].as_str().unwrap_or_default().to_string(),
            status: VideoGenStatus::Queued,
            video_url: None,
            thumbnail_url: None,
            duration_secs: Some(request.duration_secs as f32),
            progress: 0,
            error: None,
            metadata: result,
        })
    }

    /// Poll Runway for status
    async fn poll_runway(
        &self,
        job_id: &str,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let response = state
            .client
            .get(format!("{}/tasks/{}", state.config.api_url, job_id))
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let result: serde_json::Value = response.json().await?;

        let status = match result["status"].as_str() {
            Some("PENDING") => VideoGenStatus::Queued,
            Some("RUNNING") => VideoGenStatus::Processing,
            Some("SUCCEEDED") => VideoGenStatus::Completed,
            Some("FAILED") => VideoGenStatus::Failed,
            _ => VideoGenStatus::Processing,
        };

        Ok(VideoGenResult {
            job_id: job_id.to_string(),
            status,
            video_url: result["output"]["video_url"].as_str().map(|s| s.to_string()),
            thumbnail_url: result["output"]["thumbnail_url"].as_str().map(|s| s.to_string()),
            duration_secs: result["output"]["duration"].as_f64().map(|d| d as f32),
            progress: result["progress"].as_u64().unwrap_or(0) as u8,
            error: result["error"].as_str().map(|s| s.to_string()),
            metadata: result,
        })
    }

    /// Generate video using Pika Labs API
    async fn generate_pika(
        &self,
        request: &VideoGenRequest,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let body = serde_json::json!({
            "prompt": request.prompt,
            "duration": request.duration_secs,
            "aspectRatio": request.aspect_ratio.clone().unwrap_or_else(|| {
                if request.resolution.is_vertical() { "9:16" } else { "16:9" }.to_string()
            }),
            "negativePrompt": request.negative_prompt,
            "seed": request.seed,
            "guidanceScale": request.guidance_scale.unwrap_or(7.0),
        });

        let response = state
            .client
            .post(format!("{}/generate", state.config.api_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let result: serde_json::Value = response.json().await?;

        Ok(VideoGenResult {
            job_id: result["id"].as_str().unwrap_or_default().to_string(),
            status: VideoGenStatus::Queued,
            video_url: None,
            thumbnail_url: None,
            duration_secs: Some(request.duration_secs as f32),
            progress: 0,
            error: None,
            metadata: result,
        })
    }

    /// Poll Pika for status
    async fn poll_pika(
        &self,
        job_id: &str,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let response = state
            .client
            .get(format!("{}/status/{}", state.config.api_url, job_id))
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let result: serde_json::Value = response.json().await?;

        let status = match result["status"].as_str() {
            Some("queued") => VideoGenStatus::Queued,
            Some("processing") => VideoGenStatus::Processing,
            Some("completed") => VideoGenStatus::Completed,
            Some("failed") => VideoGenStatus::Failed,
            _ => VideoGenStatus::Processing,
        };

        Ok(VideoGenResult {
            job_id: job_id.to_string(),
            status,
            video_url: result["videoUrl"].as_str().map(|s| s.to_string()),
            thumbnail_url: result["thumbnailUrl"].as_str().map(|s| s.to_string()),
            duration_secs: result["duration"].as_f64().map(|d| d as f32),
            progress: result["progress"].as_u64().unwrap_or(0) as u8,
            error: result["error"].as_str().map(|s| s.to_string()),
            metadata: result,
        })
    }

    /// Generate video using Luma AI Dream Machine
    async fn generate_luma(
        &self,
        request: &VideoGenRequest,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let mut body = serde_json::json!({
            "prompt": request.prompt,
            "aspect_ratio": request.aspect_ratio.clone().unwrap_or_else(|| {
                if request.resolution.is_vertical() { "9:16" } else { "16:9" }.to_string()
            }),
        });

        if let Some(ref img_url) = request.image_url {
            body["keyframes"] = serde_json::json!({
                "frame0": {
                    "type": "image",
                    "url": img_url
                }
            });
        }

        let response = state
            .client
            .post(format!("{}/generations", state.config.api_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let result: serde_json::Value = response.json().await?;

        Ok(VideoGenResult {
            job_id: result["id"].as_str().unwrap_or_default().to_string(),
            status: VideoGenStatus::Queued,
            video_url: None,
            thumbnail_url: None,
            duration_secs: Some(request.duration_secs as f32),
            progress: 0,
            error: None,
            metadata: result,
        })
    }

    /// Poll Luma for status
    async fn poll_luma(
        &self,
        job_id: &str,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let response = state
            .client
            .get(format!("{}/generations/{}", state.config.api_url, job_id))
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let result: serde_json::Value = response.json().await?;

        let status = match result["state"].as_str() {
            Some("queued") | Some("dreaming") => VideoGenStatus::Processing,
            Some("completed") => VideoGenStatus::Completed,
            Some("failed") => VideoGenStatus::Failed,
            _ => VideoGenStatus::Processing,
        };

        Ok(VideoGenResult {
            job_id: job_id.to_string(),
            status,
            video_url: result["assets"]["video"].as_str().map(|s| s.to_string()),
            thumbnail_url: result["assets"]["thumbnail"].as_str().map(|s| s.to_string()),
            duration_secs: None,
            progress: 50, // Luma doesn't provide progress
            error: result["failure_reason"].as_str().map(|s| s.to_string()),
            metadata: result,
        })
    }

    /// Convert VideoResolution to Sora-compatible size string
    /// Sora sora-2 model ONLY supports: 720x1280 (portrait) and 1280x720 (landscape)
    fn resolution_to_sora_size(resolution: VideoResolution) -> &'static str {
        match resolution {
            // All landscape/horizontal resolutions -> 1280x720
            VideoResolution::SD480p => "1280x720",
            VideoResolution::HD720p => "1280x720",
            VideoResolution::FullHD1080p => "1280x720",
            VideoResolution::UHD4K => "1280x720",
            // All portrait/vertical resolutions -> 720x1280
            VideoResolution::Vertical720p => "720x1280",
            VideoResolution::Vertical1080p => "720x1280",
        }
    }

    /// Generate video using OpenAI Sora
    /// API Reference: https://platform.openai.com/docs/guides/video-generation
    async fn generate_sora(
        &self,
        request: &VideoGenRequest,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        // Convert to Sora-compatible size
        let sora_size = Self::resolution_to_sora_size(request.resolution);
        info!("Mapped resolution {:?} to Sora size: {}", request.resolution, sora_size);

        // Sora supports specific durations: "4", "8", or "12" seconds (as strings!)
        let duration_str = match request.duration_secs {
            0..=5 => "4",
            6..=10 => "8",
            _ => "12",
        };

        // Build Sora request body per OpenAI API spec
        // POST /v1/videos with model, prompt, size, seconds (as string)
        let mut body = serde_json::json!({
            "model": "sora-2",
            "prompt": request.prompt,
            "size": sora_size,
            "seconds": duration_str,
        });

        // Add optional reference image for img2vid
        if let Some(ref img_url) = request.image_url {
            body["input_reference"] = serde_json::json!({
                "type": "image",
                "url": img_url
            });
        }

        info!("Sora request body: {}", serde_json::to_string_pretty(&body).unwrap_or_default());

        // Use OpenAI API endpoint: POST /v1/videos
        let api_url = if state.config.api_url.contains("openai.com") || state.config.api_url.is_empty() {
            "https://api.openai.com/v1/videos".to_string()
        } else {
            format!("{}/videos", state.config.api_url.trim_end_matches('/'))
        };

        info!("Calling Sora API at: {}", api_url);

        let response = state
            .client
            .post(&api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!("Sora API error ({}): {}", status, error_text);
            return Err(ZoeyError::service(format!(
                "Sora API error ({}): {}",
                status, error_text
            )));
        }

        let result: serde_json::Value = response.json().await?;
        info!("Sora API response: {}", serde_json::to_string_pretty(&result).unwrap_or_default());

        // Sora returns a video_id for async polling
        let job_id = result["id"]
            .as_str()
            .or_else(|| result["video_id"].as_str())
            .unwrap_or_default()
            .to_string();

        let status = match result["status"].as_str() {
            Some("completed") | Some("succeeded") => VideoGenStatus::Completed,
            Some("failed") => VideoGenStatus::Failed,
            _ => VideoGenStatus::Processing,
        };

        // Calculate progress based on status
        let progress = if status == VideoGenStatus::Completed { 100 } else { 0 };

        // If completed synchronously, extract video URL
        let video_url = result["data"][0]["url"]
            .as_str()
            .or_else(|| result["video_url"].as_str())
            .or_else(|| result["output"]["url"].as_str())
            .map(|s| s.to_string());

        Ok(VideoGenResult {
            job_id,
            status,
            video_url,
            thumbnail_url: result["thumbnail_url"].as_str().map(|s| s.to_string()),
            duration_secs: Some(request.duration_secs as f32),
            progress,
            error: result["error"]["message"].as_str().map(|s| s.to_string()),
            metadata: result,
        })
    }

    /// Poll Sora for generation status
    /// API Reference: GET /v1/videos/{video_id}
    async fn poll_sora(
        &self,
        job_id: &str,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let api_url = if state.config.api_url.contains("openai.com") || state.config.api_url.is_empty() {
            format!("https://api.openai.com/v1/videos/{}", job_id)
        } else {
            format!("{}/videos/{}", state.config.api_url.trim_end_matches('/'), job_id)
        };

        info!("Polling Sora status at: {}", api_url);

        let response = state
            .client
            .get(&api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let result: serde_json::Value = response.json().await?;
        debug!("Sora poll response: {:?}", result);

        // Sora status values: pending, processing, completed, failed
        let status = match result["status"].as_str() {
            Some("pending") | Some("queued") => VideoGenStatus::Queued,
            Some("processing") | Some("in_progress") | Some("running") => VideoGenStatus::Processing,
            Some("completed") | Some("succeeded") | Some("done") => VideoGenStatus::Completed,
            Some("failed") | Some("error") => VideoGenStatus::Failed,
            Some("cancelled") => VideoGenStatus::Cancelled,
            _ => VideoGenStatus::Processing,
        };

        // Sora returns video URL in various possible locations
        let video_url = result["video_url"]
            .as_str()
            .or_else(|| result["url"].as_str())
            .or_else(|| result["data"][0]["url"].as_str())
            .or_else(|| result["output"]["url"].as_str())
            .or_else(|| result["result"]["url"].as_str())
            .map(|s| s.to_string());

        let progress = if status == VideoGenStatus::Completed {
            100
        } else {
            result["progress"]
                .as_u64()
                .unwrap_or(result["percent_complete"].as_u64().unwrap_or(50)) as u8
        };

        Ok(VideoGenResult {
            job_id: job_id.to_string(),
            status,
            video_url,
            thumbnail_url: result["thumbnail_url"].as_str().map(|s| s.to_string()),
            duration_secs: result["duration"].as_f64().map(|d| d as f32),
            progress,
            error: result["error"]["message"]
                .as_str()
                .or_else(|| result["error"].as_str())
                .map(|s| s.to_string()),
            metadata: result,
        })
    }

    /// Download video content from Sora
    /// API Reference: GET /v1/videos/{video_id}/content
    async fn download_sora_video(
        &self,
        job_id: &str,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<Vec<u8>> {
        let api_url = if state.config.api_url.contains("openai.com") || state.config.api_url.is_empty() {
            format!("https://api.openai.com/v1/videos/{}/content", job_id)
        } else {
            format!("{}/videos/{}/content", state.config.api_url.trim_end_matches('/'), job_id)
        };

        info!("Downloading video from Sora: {}", api_url);

        let response = state
            .client
            .get(&api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ZoeyError::provider(format!(
                "Failed to download video: {}",
                error_text
            )));
        }

        let bytes = response.bytes().await.map_err(|e| ZoeyError::Network(e))?;
        info!("Downloaded {} bytes of video data", bytes.len());
        Ok(bytes.to_vec())
    }

    /// Upload video to catbox.moe and return the public URL
    /// Includes retry logic with exponential backoff and fallback to 0x0.st
    async fn upload_to_catbox(
        &self,
        video_bytes: Vec<u8>,
        state: &VideoServiceState,
    ) -> Result<String> {
        info!("Uploading {} bytes to file host", video_bytes.len());

        // Try catbox.moe with retries first
        let max_retries = 3;
        let mut last_error = String::new();
        
        for attempt in 0..max_retries {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(2u64.pow(attempt as u32));
                info!("Retry attempt {} for catbox.moe, waiting {:?}", attempt + 1, delay);
                tokio::time::sleep(delay).await;
            }

            match self.try_upload_catbox(&video_bytes, state).await {
                Ok(url) => {
                    info!("Video uploaded to catbox.moe: {}", url);
                    return Ok(url);
                }
                Err(e) => {
                    warn!("Catbox upload attempt {} failed: {}", attempt + 1, e);
                    last_error = e.to_string();
                }
            }
        }

        // Fallback to 0x0.st if catbox fails
        info!("Catbox.moe failed after {} retries, trying fallback host 0x0.st", max_retries);
        match self.try_upload_0x0(&video_bytes, state).await {
            Ok(url) => {
                info!("Video uploaded to 0x0.st: {}", url);
                return Ok(url);
            }
            Err(e) => {
                warn!("0x0.st upload failed: {}", e);
            }
        }

        // Try litterbox (temporary catbox) as last resort
        info!("Trying litterbox.catbox.moe as last resort");
        match self.try_upload_litterbox(&video_bytes, state).await {
            Ok(url) => {
                info!("Video uploaded to litterbox.catbox.moe: {}", url);
                return Ok(url);
            }
            Err(e) => {
                error!("All upload hosts failed. Last error: {}", e);
            }
        }

        Err(ZoeyError::provider(format!(
            "All upload hosts failed. Catbox error: {}",
            last_error
        )))
    }

    /// Try uploading to catbox.moe
    async fn try_upload_catbox(
        &self,
        video_bytes: &[u8],
        state: &VideoServiceState,
    ) -> Result<String> {
        let part = reqwest::multipart::Part::bytes(video_bytes.to_vec())
            .file_name("video.mp4")
            .mime_str("video/mp4")
            .map_err(|e| ZoeyError::provider(format!("Failed to create multipart: {}", e)))?;

        let form = reqwest::multipart::Form::new()
            .text("reqtype", "fileupload")
            .part("fileToUpload", part);

        let response = state
            .client
            .post("https://catbox.moe/user/api.php")
            .header("User-Agent", "Mozilla/5.0 (compatible; ZoeyBot/1.0)")
            .timeout(std::time::Duration::from_secs(120))
            .multipart(form)
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ZoeyError::provider(format!(
                "Catbox upload failed: {}",
                error_text
            )));
        }

        let url = response.text().await.map_err(|e| ZoeyError::Network(e))?;
        Ok(url.trim().to_string())
    }

    /// Try uploading to 0x0.st (null pointer file hosting)
    async fn try_upload_0x0(
        &self,
        video_bytes: &[u8],
        state: &VideoServiceState,
    ) -> Result<String> {
        let part = reqwest::multipart::Part::bytes(video_bytes.to_vec())
            .file_name("video.mp4")
            .mime_str("video/mp4")
            .map_err(|e| ZoeyError::provider(format!("Failed to create multipart: {}", e)))?;

        let form = reqwest::multipart::Form::new()
            .part("file", part);

        let response = state
            .client
            .post("https://0x0.st")
            .header("User-Agent", "Mozilla/5.0 (compatible; ZoeyBot/1.0)")
            .timeout(std::time::Duration::from_secs(120))
            .multipart(form)
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ZoeyError::provider(format!(
                "0x0.st upload failed: {}",
                error_text
            )));
        }

        let url = response.text().await.map_err(|e| ZoeyError::Network(e))?;
        Ok(url.trim().to_string())
    }

    /// Try uploading to litterbox.catbox.moe (temporary file hosting, 72h expiry)
    async fn try_upload_litterbox(
        &self,
        video_bytes: &[u8],
        state: &VideoServiceState,
    ) -> Result<String> {
        let part = reqwest::multipart::Part::bytes(video_bytes.to_vec())
            .file_name("video.mp4")
            .mime_str("video/mp4")
            .map_err(|e| ZoeyError::provider(format!("Failed to create multipart: {}", e)))?;

        let form = reqwest::multipart::Form::new()
            .text("reqtype", "fileupload")
            .text("time", "72h")
            .part("fileToUpload", part);

        let response = state
            .client
            .post("https://litterbox.catbox.moe/resources/internals/api.php")
            .header("User-Agent", "Mozilla/5.0 (compatible; ZoeyBot/1.0)")
            .timeout(std::time::Duration::from_secs(120))
            .multipart(form)
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ZoeyError::provider(format!(
                "Litterbox upload failed: {}",
                error_text
            )));
        }

        let url = response.text().await.map_err(|e| ZoeyError::Network(e))?;
        Ok(url.trim().to_string())
    }

    /// Poll Sora, and when complete, download video and upload to catbox
    /// Caches downloaded video bytes so upload can be retried on subsequent polls
    async fn poll_and_upload_sora(
        &self,
        job_id: &str,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let mut result = self.poll_sora(job_id, api_key, state).await?;

        // If completed but no video_url, we need to download and upload
        if result.status == VideoGenStatus::Completed && result.video_url.is_none() {
            // Check if we have cached video bytes from a previous failed upload
            let cached_bytes = {
                let pending = self.pending_uploads.read().await;
                pending.get(job_id).map(|p| p.video_bytes.clone())
            };

            let video_bytes = if let Some(bytes) = cached_bytes {
                info!("Using cached video bytes ({} bytes) for retry upload", bytes.len());
                bytes
            } else {
                // Download from Sora
                info!("Video completed, downloading from Sora");
                match self.download_sora_video(job_id, api_key, state).await {
                    Ok(bytes) => {
                        info!("Downloaded {} bytes, caching for upload", bytes.len());
                        // Cache the bytes
                        self.pending_uploads.write().await.insert(
                            job_id.to_string(),
                            PendingUpload {
                                video_bytes: bytes.clone(),
                                downloaded_at: std::time::Instant::now(),
                            },
                        );
                        bytes
                    }
                    Err(e) => {
                        error!("Failed to download video: {}", e);
                        result.error = Some(format!("Download failed: {}", e));
                        // Return as "Processing" so client knows to retry
                        result.status = VideoGenStatus::Processing;
                        result.progress = 99;
                        return Ok(result);
                    }
                }
            };

            // Try to upload
            info!("Uploading {} bytes to file host", video_bytes.len());
            match self.upload_to_catbox(video_bytes, state).await {
                Ok(catbox_url) => {
                    info!("Upload successful: {}", catbox_url);
                    result.video_url = Some(catbox_url);
                    // Remove from cache on success
                    self.pending_uploads.write().await.remove(job_id);
                }
                Err(e) => {
                    error!("Failed to upload to file host: {}", e);
                    result.error = Some(format!("Upload pending, will retry: {}", e));
                    // Keep status as Processing so client retries
                    result.status = VideoGenStatus::Processing;
                    result.progress = 99;
                }
            }
        }

        Ok(result)
    }

    /// Generate video using custom provider
    async fn generate_custom(
        &self,
        request: &VideoGenRequest,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let (width, height) = request.resolution.dimensions();

        let body = serde_json::json!({
            "prompt": request.prompt,
            "image_url": request.image_url,
            "duration": request.duration_secs,
            "width": width,
            "height": height,
            "seed": request.seed,
            "negative_prompt": request.negative_prompt,
            "guidance_scale": request.guidance_scale,
        });

        let response = state
            .client
            .post(&state.config.api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let result: serde_json::Value = response.json().await?;

        Ok(VideoGenResult {
            job_id: result["job_id"]
                .as_str()
                .or_else(|| result["id"].as_str())
                .unwrap_or_default()
                .to_string(),
            status: VideoGenStatus::Queued,
            video_url: result["video_url"].as_str().map(|s| s.to_string()),
            thumbnail_url: None,
            duration_secs: Some(request.duration_secs as f32),
            progress: 0,
            error: None,
            metadata: result,
        })
    }

    /// Poll custom provider for status
    async fn poll_custom(
        &self,
        job_id: &str,
        api_key: &str,
        state: &VideoServiceState,
    ) -> Result<VideoGenResult> {
        let response = state
            .client
            .get(format!("{}/status/{}", state.config.api_url, job_id))
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| ZoeyError::Network(e))?;

        let result: serde_json::Value = response.json().await?;

        let status_str = result["status"]
            .as_str()
            .or_else(|| result["state"].as_str())
            .unwrap_or("processing");

        let status = match status_str.to_lowercase().as_str() {
            "queued" | "pending" => VideoGenStatus::Queued,
            "processing" | "running" => VideoGenStatus::Processing,
            "completed" | "succeeded" | "done" => VideoGenStatus::Completed,
            "failed" | "error" => VideoGenStatus::Failed,
            _ => VideoGenStatus::Processing,
        };

        Ok(VideoGenResult {
            job_id: job_id.to_string(),
            status,
            video_url: result["video_url"]
                .as_str()
                .or_else(|| result["output"].as_str())
                .map(|s| s.to_string()),
            thumbnail_url: result["thumbnail_url"].as_str().map(|s| s.to_string()),
            duration_secs: result["duration"].as_f64().map(|d| d as f32),
            progress: result["progress"].as_u64().unwrap_or(0) as u8,
            error: result["error"].as_str().map(|s| s.to_string()),
            metadata: result,
        })
    }

    /// Build a video generation request from options
    pub fn build_request(
        &self,
        prompt: String,
        image_url: Option<String>,
        options: VideoOptions,
        config: &VideoGenerationConfig,
    ) -> VideoGenRequest {
        VideoGenRequest {
            prompt,
            image_url,
            duration_secs: options
                .duration_secs
                .unwrap_or(config.default_duration_secs)
                .min(config.max_duration_secs),
            resolution: options.resolution.unwrap_or(config.default_resolution),
            aspect_ratio: options.aspect_ratio,
            style: options.style,
            seed: options.seed,
            negative_prompt: None,
            guidance_scale: None,
        }
    }
}

/// Helper to truncate prompts for logging
fn truncate_prompt(prompt: &str, max_len: usize) -> String {
    if prompt.len() <= max_len {
        prompt.to_string()
    } else {
        format!("{}...", &prompt[..max_len])
    }
}

/// Extract progress percentage from log strings
fn extract_progress(logs: &str) -> Option<u8> {
    // Look for patterns like "50%", "progress: 50", etc.
    let re = regex::Regex::new(r"(\d{1,3})%").ok()?;
    re.captures_iter(logs)
        .last()
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse::<u8>().ok())
        .map(|p| p.min(100))
}

#[async_trait]
impl Service for VideoGenerationService {
    fn service_type(&self) -> &str {
        "video-generation"
    }

    async fn initialize(
        &mut self,
        _runtime: Arc<dyn Any + Send + Sync>,
    ) -> Result<()> {
        info!("Initializing Video Generation Service");

        let mut state = self.state.write().await;

        // Load API key from environment
        let api_key = std::env::var(&state.config.api_key_env).ok();

        if api_key.is_none() {
            warn!(
                "Video generation API key not found in environment variable '{}'",
                state.config.api_key_env
            );
        }

        state.api_key = api_key;

        info!(
            "Video generation service configured with provider: {:?}",
            state.config.provider
        );

        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        *self.running.write().await = true;
        info!("Video Generation Service started");
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        *self.running.write().await = false;
        info!("Video Generation Service stopped");
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.try_read().map(|r| *r).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_prompt() {
        assert_eq!(truncate_prompt("short", 10), "short");
        assert_eq!(truncate_prompt("this is a long prompt", 10), "this is a ...");
    }

    #[test]
    fn test_extract_progress() {
        assert_eq!(extract_progress("Processing... 50%"), Some(50));
        assert_eq!(extract_progress("25% complete... now 75%"), Some(75));
        assert_eq!(extract_progress("no progress here"), None);
    }

    #[tokio::test]
    async fn test_service_creation() {
        let config = VideoGenerationConfig::default();
        let service = VideoGenerationService::new(config);

        assert_eq!(service.service_type(), "video-generation");
    }
}

