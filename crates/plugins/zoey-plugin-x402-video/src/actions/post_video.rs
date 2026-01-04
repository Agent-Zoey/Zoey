//! Post Video Action
//!
//! Posts a generated video to specified social media platforms.

use async_trait::async_trait;
use zoey_core::{error::ZoeyError, types::*, Result};
use std::sync::Arc;
use tracing::info;

use crate::config::{PlatformTarget, VideoPostResult, X402VideoConfig};
use crate::services::MultiPlatformPoster;

/// Action to post a video to social media platforms
pub struct PostVideoAction {
    poster: Arc<MultiPlatformPoster>,
    config: X402VideoConfig,
}

impl PostVideoAction {
    /// Create a new post video action
    pub fn new(poster: Arc<MultiPlatformPoster>, config: X402VideoConfig) -> Self {
        Self { poster, config }
    }

    /// Parse posting request from message and state
    fn parse_request(
        &self,
        message: &Memory,
        state: &State,
    ) -> Option<(String, String, Vec<String>, Vec<PlatformTarget>)> {
        // Get video URL from state (set by GENERATE_VIDEO action)
        let video_url = state
            .get_value("video_url")
            .cloned()
            .or_else(|| {
                state.get_data("video_url").and_then(|v| v.as_str().map(|s| s.to_string()))
            })?;

        // Get caption from message or default
        let text = message.content.text.to_lowercase();
        let caption = if text.contains("caption:") {
            message
                .content
                .text
                .split("caption:")
                .nth(1)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "Check out this AI-generated video!".to_string())
        } else if text.contains("with caption") {
            message
                .content
                .text
                .split("with caption")
                .nth(1)
                .map(|s| s.trim().trim_matches('"').to_string())
                .unwrap_or_else(|| "Check out this AI-generated video!".to_string())
        } else {
            "Check out this AI-generated video! 🎬✨".to_string()
        };

        // Parse hashtags from message
        let mut hashtags: Vec<String> = text
            .split_whitespace()
            .filter(|word| word.starts_with('#'))
            .map(|tag| tag.trim_start_matches('#').to_string())
            .collect();

        // Add default hashtags based on platforms
        if hashtags.is_empty() {
            hashtags = vec![
                "AI".to_string(),
                "AIVideo".to_string(),
                "AIGenerated".to_string(),
            ];
        }

        // Parse target platforms
        let mut platforms = Vec::new();

        if text.contains("all platforms") || text.contains("everywhere") {
            platforms.push(PlatformTarget::All);
        } else {
            if text.contains("instagram") || text.contains("ig") || text.contains("insta") {
                platforms.push(PlatformTarget::Instagram);
            }
            if text.contains("tiktok") || text.contains("tik tok") {
                platforms.push(PlatformTarget::TikTok);
            }
            if text.contains("snapchat") || text.contains("snap") {
                platforms.push(PlatformTarget::Snapchat);
            }
        }

        // Default to all enabled platforms if none specified
        if platforms.is_empty() {
            platforms.push(PlatformTarget::All);
        }

        Some((video_url, caption, hashtags, platforms))
    }
}

#[async_trait]
impl Action for PostVideoAction {
    fn name(&self) -> &str {
        "POST_VIDEO"
    }

    fn description(&self) -> &str {
        "Post a generated video to social media platforms (Instagram, TikTok, Snapchat)"
    }

    fn similes(&self) -> Vec<String> {
        vec![
            "SHARE_VIDEO".to_string(),
            "PUBLISH_VIDEO".to_string(),
            "UPLOAD_VIDEO".to_string(),
            "POST_TO_SOCIAL".to_string(),
        ]
    }

    fn examples(&self) -> Vec<Vec<ActionExample>> {
        vec![
            vec![
                ActionExample {
                    name: "User".to_string(),
                    text: "Post that video to Instagram and TikTok".to_string(),
                },
                ActionExample {
                    name: "Assistant".to_string(),
                    text: "Posting your video to Instagram and TikTok now...".to_string(),
                },
            ],
            vec![
                ActionExample {
                    name: "User".to_string(),
                    text: "Share to all platforms with caption \"Amazing sunset!\" #sunset #nature"
                        .to_string(),
                },
                ActionExample {
                    name: "Assistant".to_string(),
                    text: "Posting to all enabled platforms with your caption and hashtags."
                        .to_string(),
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
        let text = message.content.text.to_lowercase();

        // Check if this looks like a post request
        let is_post_request = text.contains("post")
            || text.contains("share")
            || text.contains("publish")
            || text.contains("upload");

        // Check if we have a video to post
        let has_video = state.get_value("video_url").is_some()
            || state.get_data("video_url").is_some();

        Ok(is_post_request && has_video)
    }

    async fn handler(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
        _options: Option<HandlerOptions>,
        callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        // Parse the posting request
        let (video_url, caption, hashtags, platforms) = self
            .parse_request(message, state)
            .ok_or_else(|| ZoeyError::validation("Could not parse video posting request"))?;

        info!(
            "Posting video to platforms: {:?}",
            platforms
                .iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<_>>()
        );

        // Check enabled platforms
        let enabled = self.poster.enabled_platforms();
        if enabled.is_empty() {
            let error_msg =
                "No social media platforms are enabled. Please configure at least one platform.";

            if let Some(cb) = callback {
                cb(Content {
                    text: error_msg.to_string(),
                    ..Default::default()
                })
                .await?;
            }

            return Ok(Some(ActionResult {
                action_name: Some(self.name().to_string()),
                text: Some(error_msg.to_string()),
                values: None,
                data: None,
                success: false,
                error: Some("No platforms configured".to_string()),
            }));
        }

        // Notify starting
        if let Some(cb) = &callback {
            let platform_names: Vec<String> = platforms
                .iter()
                .map(|p| match p {
                    PlatformTarget::Instagram => "Instagram",
                    PlatformTarget::TikTok => "TikTok",
                    PlatformTarget::Snapchat => "Snapchat",
                    PlatformTarget::All => "all enabled platforms",
                })
                .map(|s| s.to_string())
                .collect();

            cb(Content {
                text: format!(
                    "Posting video to {}...\nCaption: \"{}\"\nHashtags: {}",
                    platform_names.join(", "),
                    caption,
                    hashtags.iter().map(|t| format!("#{}", t)).collect::<Vec<_>>().join(" ")
                ),
                ..Default::default()
            })
            .await?;
        }

        // Post to platforms
        let results = self
            .poster
            .post_to_platforms(&video_url, &caption, &hashtags, &platforms)
            .await;

        // Build response
        let success_count = results.iter().filter(|r| r.success).count();
        let total_count = results.len();

        let mut result_text = format!(
            "Posting complete: {}/{} platforms succeeded.\n\n",
            success_count, total_count
        );

        for result in &results {
            if result.success {
                result_text.push_str(&format!(
                    "✅ {} - Posted successfully\n   URL: {}\n",
                    result.platform,
                    result.post_url.as_deref().unwrap_or("N/A")
                ));
            } else {
                result_text.push_str(&format!(
                    "❌ {} - Failed: {}\n",
                    result.platform,
                    result.error.as_deref().unwrap_or("Unknown error")
                ));
            }
        }

        // Send final callback
        if let Some(cb) = callback {
            cb(Content {
                text: result_text.clone(),
                ..Default::default()
            })
            .await?;
        }

        // Build result data
        let mut data = std::collections::HashMap::new();
        data.insert(
            "platform_results".to_string(),
            serde_json::to_value(&results)?,
        );
        data.insert(
            "video_url".to_string(),
            serde_json::Value::String(video_url),
        );
        data.insert(
            "caption".to_string(),
            serde_json::Value::String(caption),
        );
        data.insert(
            "hashtags".to_string(),
            serde_json::to_value(&hashtags)?,
        );

        let post_result = VideoPostResult {
            success: success_count > 0,
            video_url: Some(state.get_value("video_url").cloned().unwrap_or_default()),
            platform_results: results,
            error: if success_count == 0 {
                Some("All platform posts failed".to_string())
            } else {
                None
            },
            payment_receipt: None,
        };

        data.insert(
            "post_result".to_string(),
            serde_json::to_value(&post_result)?,
        );

        Ok(Some(ActionResult {
            action_name: Some(self.name().to_string()),
            text: Some(result_text),
            values: Some({
                let mut values = std::collections::HashMap::new();
                values.insert("success_count".to_string(), success_count.to_string());
                values.insert("total_count".to_string(), total_count.to_string());
                values
            }),
            data: Some(data),
            success: success_count > 0,
            error: if success_count == 0 {
                Some("All platform posts failed".to_string())
            } else {
                None
            },
        }))
    }
}

/// Combined action that generates a video and posts it to platforms in one step
pub struct GenerateAndPostVideoAction {
    generate_action: Arc<super::GenerateVideoAction>,
    post_action: Arc<PostVideoAction>,
}

impl GenerateAndPostVideoAction {
    /// Create a new generate and post action
    pub fn new(
        generate_action: Arc<super::GenerateVideoAction>,
        post_action: Arc<PostVideoAction>,
    ) -> Self {
        Self {
            generate_action,
            post_action,
        }
    }
}

#[async_trait]
impl Action for GenerateAndPostVideoAction {
    fn name(&self) -> &str {
        "GENERATE_AND_POST_VIDEO"
    }

    fn description(&self) -> &str {
        "Generate an AI video and post it to social media platforms in one action"
    }

    fn similes(&self) -> Vec<String> {
        vec![
            "CREATE_AND_SHARE_VIDEO".to_string(),
            "MAKE_AND_POST_VIDEO".to_string(),
            "VIDEO_TO_SOCIAL".to_string(),
        ]
    }

    fn examples(&self) -> Vec<Vec<ActionExample>> {
        vec![vec![
            ActionExample {
                name: "User".to_string(),
                text: "Generate a video of a dancing robot and post it to TikTok and Instagram"
                    .to_string(),
            },
            ActionExample {
                name: "Assistant".to_string(),
                text: "I'll generate the video and post it to both platforms for you.".to_string(),
            },
        ]]
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        let text = message.content.text.to_lowercase();

        // Check for combined request
        let is_generate = text.contains("generate")
            || text.contains("create")
            || text.contains("make");

        let is_post = text.contains("post")
            || text.contains("share")
            || text.contains("publish");

        let is_video = text.contains("video");

        Ok(is_generate && is_post && is_video)
    }

    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
        options: Option<HandlerOptions>,
        callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        // Step 1: Generate the video
        let gen_result = self
            .generate_action
            .handler(runtime.clone(), message, state, options.clone(), callback.clone())
            .await?;

        let gen_result = gen_result.ok_or_else(|| {
            ZoeyError::action("Video generation returned no result")
        })?;

        if !gen_result.success {
            return Ok(Some(gen_result));
        }

        // Get video URL from generation result
        let video_url = gen_result
            .values
            .as_ref()
            .and_then(|v| v.get("video_url"))
            .cloned()
            .ok_or_else(|| ZoeyError::action("No video URL in generation result"))?;

        // Create updated state with video URL
        let mut new_state = state.clone();
        new_state.set_value("video_url", video_url.clone());

        // Step 2: Post the video
        let post_result = self
            .post_action
            .handler(runtime, message, &new_state, options, callback)
            .await?;

        // Combine results
        if let Some(mut post_result) = post_result {
            // Merge generation data into post result
            if let Some(gen_data) = gen_result.data {
                if let Some(ref mut post_data) = post_result.data {
                    post_data.insert(
                        "generation_result".to_string(),
                        serde_json::to_value(&gen_data)?,
                    );
                }
            }

            post_result.action_name = Some(self.name().to_string());
            Ok(Some(post_result))
        } else {
            Ok(Some(gen_result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_post_action_metadata() {
        let poster = MultiPlatformPoster::new(None, None, None);
        let action = PostVideoAction::new(Arc::new(poster), Default::default());

        assert_eq!(action.name(), "POST_VIDEO");
        assert!(!action.description().is_empty());
        assert!(!action.similes().is_empty());
    }
}

