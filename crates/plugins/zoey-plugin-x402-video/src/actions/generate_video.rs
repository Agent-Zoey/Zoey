//! Generate Video Action
//!
//! Handles x402 payment-gated AI video generation requests.

use async_trait::async_trait;
use zoey_core::{error::ZoeyError, types::*, Result};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::config::{VideoPostRequest, X402VideoConfig};
use crate::services::{
    VideoGenRequest, VideoGenStatus, VideoGenerationService, X402PaymentService,
};

/// Action to generate a video with x402 payment
pub struct GenerateVideoAction {
    video_service: Arc<VideoGenerationService>,
    payment_service: Arc<X402PaymentService>,
    config: X402VideoConfig,
}

impl GenerateVideoAction {
    /// Create a new generate video action
    pub fn new(
        video_service: Arc<VideoGenerationService>,
        payment_service: Arc<X402PaymentService>,
        config: X402VideoConfig,
    ) -> Self {
        Self {
            video_service,
            payment_service,
            config,
        }
    }

    /// Parse video generation request from message content
    fn parse_request(&self, message: &Memory, state: &State) -> Option<VideoGenRequest> {
        // Try to get structured request from state
        if let Some(request_data) = state.get_data("video_request") {
            if let Ok(request) = serde_json::from_value::<VideoPostRequest>(request_data.clone()) {
                return Some(self.video_service.build_request(
                    request.prompt,
                    request.initial_image_url,
                    request.video_options,
                    &self.config.video_generation,
                ));
            }
        }

        // Fallback: parse from message text
        let text = &message.content.text;

        // Look for video generation keywords
        let is_video_request = text.to_lowercase().contains("generate video")
            || text.to_lowercase().contains("create video")
            || text.to_lowercase().contains("make video")
            || text.to_lowercase().contains("video of");

        if !is_video_request {
            return None;
        }

        // Extract prompt (everything after the keyword)
        let prompt = text
            .replace("generate video", "")
            .replace("Generate video", "")
            .replace("create video", "")
            .replace("Create video", "")
            .replace("make video", "")
            .replace("Make video", "")
            .replace("video of", "")
            .replace("Video of", "")
            .trim()
            .to_string();

        if prompt.is_empty() {
            return None;
        }

        Some(VideoGenRequest {
            prompt,
            image_url: None,
            duration_secs: self.config.video_generation.default_duration_secs,
            resolution: self.config.video_generation.default_resolution,
            aspect_ratio: None,
            style: None,
            seed: None,
            negative_prompt: None,
            guidance_scale: None,
        })
    }

    /// Check for payment proof in message or state
    fn get_payment_proof(&self, message: &Memory, state: &State) -> Option<String> {
        // Check message content for x402 header
        if let Some(attachments) = &message.content.attachments {
            for attachment in attachments {
                if let Some(ref text) = attachment.text {
                    if text.starts_with("x402 ") {
                        return Some(text.clone());
                    }
                }
            }
        }

        // Check state for pre-validated payment
        if let Some(proof) = state.get_value("x402_payment_proof") {
            return Some(proof.clone());
        }

        // Check state data
        if let Some(proof_data) = state.get_data("x402_payment_proof") {
            if let Some(proof_str) = proof_data.as_str() {
                return Some(proof_str.to_string());
            }
        }

        None
    }
}

#[async_trait]
impl Action for GenerateVideoAction {
    fn name(&self) -> &str {
        "GENERATE_VIDEO"
    }

    fn description(&self) -> &str {
        "Generate an AI video from a text prompt. Requires x402 payment authorization."
    }

    fn similes(&self) -> Vec<String> {
        vec![
            "CREATE_VIDEO".to_string(),
            "MAKE_VIDEO".to_string(),
            "AI_VIDEO".to_string(),
            "TEXT_TO_VIDEO".to_string(),
        ]
    }

    fn examples(&self) -> Vec<Vec<ActionExample>> {
        vec![
            vec![
                ActionExample {
                    name: "User".to_string(),
                    text: "Generate video of a cat playing piano in a jazz club".to_string(),
                },
                ActionExample {
                    name: "Assistant".to_string(),
                    text: "I'll generate that video for you. Processing payment and starting generation...".to_string(),
                },
            ],
            vec![
                ActionExample {
                    name: "User".to_string(),
                    text: "Create video: sunset over mountains with birds flying".to_string(),
                },
                ActionExample {
                    name: "Assistant".to_string(),
                    text: "Creating your video of a mountain sunset scene. This will take a few minutes.".to_string(),
                },
            ],
        ]
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
    ) -> Result<bool> {
        // Check if this looks like a video generation request
        let request = self.parse_request(message, state);
        Ok(request.is_some())
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
        _options: Option<HandlerOptions>,
        callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        // Parse the video generation request
        let request = self.parse_request(message, state).ok_or_else(|| {
            ZoeyError::validation("Could not parse video generation request from message")
        })?;

        info!("Processing video generation request: {}", request.prompt);

        // Check for payment proof
        let payment_proof = self.get_payment_proof(message, state);

        if let Some(proof) = &payment_proof {
            // Verify the payment
            let resource_id = format!("video-{}", uuid::Uuid::new_v4());
            let verification = self
                .payment_service
                .verify_payment(proof, Some(&resource_id))
                .await?;

            if !verification.valid {
                // Payment invalid - return error
                let error_msg = format!("Payment verification failed: {}", verification.message);
                warn!("{}", error_msg);

                if let Some(cb) = callback {
                    cb(Content {
                        text: error_msg.clone(),
                        ..Default::default()
                    })
                    .await?;
                }

                return Ok(Some(ActionResult {
                    action_name: Some(self.name().to_string()),
                    text: Some(error_msg),
                    values: None,
                    data: None,
                    success: false,
                    error: Some("Payment verification failed".to_string()),
                }));
            }

            debug!("Payment verified successfully");
        } else {
            // No payment provided - create payment requirement
            let resource_id = format!("video-{}", uuid::Uuid::new_v4());
            let requirement = self
                .payment_service
                .create_payment_requirement(
                    &resource_id,
                    Some(self.config.x402.default_price_cents),
                    Some(format!("Generate video: {}", &request.prompt)),
                )
                .await?;

            let headers = self.payment_service.format_402_headers(&requirement);

            // Return 402 Payment Required response
            let response_text = format!(
                "Payment required to generate video.\n\n\
                Amount: {} USDC\n\
                Network: {}\n\
                Pay to: {}\n\n\
                Please submit payment using x402 protocol.",
                (self.config.x402.default_price_cents as f64) / 100.0,
                requirement.network,
                requirement.pay_to
            );

            if let Some(cb) = callback {
                cb(Content {
                    text: response_text.clone(),
                    ..Default::default()
                })
                .await?;
            }

            // Return payment requirement in result
            let mut data = std::collections::HashMap::new();
            data.insert(
                "payment_requirement".to_string(),
                serde_json::to_value(&requirement)?,
            );
            data.insert(
                "headers".to_string(),
                serde_json::to_value(&headers)?,
            );

            return Ok(Some(ActionResult {
                action_name: Some(self.name().to_string()),
                text: Some(response_text),
                values: None,
                data: Some(data),
                success: false,
                error: Some("Payment required".to_string()),
            }));
        }

        // Payment verified - start video generation
        if let Some(cb) = &callback {
            cb(Content {
                text: format!(
                    "Starting video generation for: \"{}\"\nThis may take a few minutes...",
                    request.prompt
                ),
                ..Default::default()
            })
            .await?;
        }

        // Generate video
        let gen_result = self.video_service.generate(request.clone()).await?;
        let initial_job_id = gen_result.job_id.clone();

        // Wait for completion (with timeout)
        // NOTE: Video generation typically takes 1-3 minutes
        let timeout_secs = 300; // 5 minutes
        let poll_interval = 45; // Poll every 45 seconds to avoid overwhelming Cloudflare

        let final_result = if gen_result.status == VideoGenStatus::Completed {
            gen_result
        } else {
            self.video_service
                .wait_for_completion(&initial_job_id, timeout_secs, poll_interval)
                .await?
        };

        let video_url = final_result.video_url.clone().unwrap_or_default();

        info!(
            "Video generation complete: {} -> {}",
            final_result.job_id, video_url
        );

        // Send completion callback
        if let Some(cb) = callback {
            cb(Content {
                text: format!(
                    "Video generated successfully!\n\nURL: {}\n\nYou can now post this to social media platforms.",
                    video_url
                ),
                ..Default::default()
            })
            .await?;
        }

        // Build result
        let mut result_data = std::collections::HashMap::new();
        result_data.insert(
            "video_url".to_string(),
            serde_json::Value::String(video_url.clone()),
        );
        result_data.insert(
            "job_id".to_string(),
            serde_json::Value::String(final_result.job_id.clone()),
        );
        result_data.insert("generation_result".to_string(), final_result.metadata);

        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some(format!("Video generated: {}", video_url)),
            values: Some({
                let mut values = std::collections::HashMap::new();
                values.insert("video_url".to_string(), video_url);
                values.insert("job_id".to_string(), final_result.job_id);
                values
            }),
            data: Some(result_data),
            success: true,
            error: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_metadata() {
        // Can't easily test without full service setup
        // Just verify the action name and description
        assert!(!GenerateVideoAction::name(&GenerateVideoAction {
            video_service: Arc::new(VideoGenerationService::new(Default::default())),
            payment_service: Arc::new(X402PaymentService::new(Default::default())),
            config: Default::default(),
        })
        .is_empty());
    }
}

