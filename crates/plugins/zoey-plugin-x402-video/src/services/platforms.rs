//! Platform Posting Services
//!
//! Handles posting generated videos to Instagram, TikTok, and Snapchat.

use crate::config::{
    InstagramConfig, PlatformPostResult, SnapchatConfig, TikTokConfig, TikTokPrivacyLevel,
};
use async_trait::async_trait;
use reqwest::Client;
use tracing::{debug, error, info};

/// Platform posting error types
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("Platform not configured: {0}")]
    NotConfigured(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Upload failed: {0}")]
    UploadFailed(String),

    #[error("Publish failed: {0}")]
    PublishFailed(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Invalid video format: {0}")]
    InvalidFormat(String),
}

/// Common interface for platform posting
#[async_trait]
pub trait PlatformPoster: Send + Sync {
    /// Platform name
    fn platform_name(&self) -> &str;

    /// Check if platform is enabled and configured
    fn is_enabled(&self) -> bool;

    /// Post a video to the platform
    async fn post_video(
        &self,
        video_url: &str,
        caption: &str,
        hashtags: &[String],
    ) -> Result<PlatformPostResult, PlatformError>;

    /// Get platform-specific requirements for video
    fn video_requirements(&self) -> VideoRequirements;
}

/// Video requirements for a platform
#[derive(Debug, Clone)]
pub struct VideoRequirements {
    /// Minimum duration in seconds
    pub min_duration_secs: f32,
    /// Maximum duration in seconds
    pub max_duration_secs: f32,
    /// Supported aspect ratios
    pub aspect_ratios: Vec<String>,
    /// Maximum file size in bytes
    pub max_file_size_bytes: u64,
    /// Supported formats
    pub formats: Vec<String>,
}

// ============================================================================
// Instagram Posting Service
// ============================================================================

/// Instagram posting service using Instagram Graph API
pub struct InstagramPoster {
    config: Option<InstagramConfig>,
    client: Client,
    access_token: Option<String>,
}

impl InstagramPoster {
    /// Create a new Instagram poster
    pub fn new(config: Option<InstagramConfig>) -> Self {
        let access_token = config.as_ref().and_then(|c| {
            std::env::var(&c.access_token_env).ok()
        });

        Self {
            config,
            client: Client::new(),
            access_token,
        }
    }

    /// Upload video to Instagram container
    async fn create_media_container(
        &self,
        video_url: &str,
        caption: &str,
        config: &InstagramConfig,
    ) -> Result<String, PlatformError> {
        let token = self.access_token.as_ref().ok_or_else(|| {
            PlatformError::AuthFailed("Instagram access token not configured".to_string())
        })?;

        let media_type = if config.post_as_reels { "REELS" } else { "VIDEO" };

        let mut params = vec![
            ("media_type", media_type.to_string()),
            ("video_url", video_url.to_string()),
            ("caption", caption.to_string()),
            ("access_token", token.clone()),
        ];

        // Add cover thumbnail position
        if config.post_as_reels {
            params.push((
                "thumb_offset",
                format!("{}", (config.posting_settings.cover_frame_position * 1000.0) as i32),
            ));
        }

        // Add share to Facebook if enabled
        if config.share_to_facebook {
            params.push(("share_to_feed", "true".to_string()));
        }

        let url = format!(
            "https://graph.facebook.com/v18.0/{}/media",
            config.business_account_id
        );

        let response = self
            .client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(PlatformError::UploadFailed(format!(
                "Instagram API error: {}",
                error_text
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        result["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| PlatformError::UploadFailed("No container ID in response".to_string()))
    }

    /// Check container upload status
    async fn check_container_status(
        &self,
        container_id: &str,
    ) -> Result<bool, PlatformError> {
        let token = self.access_token.as_ref().ok_or_else(|| {
            PlatformError::AuthFailed("Instagram access token not configured".to_string())
        })?;

        let url = format!(
            "https://graph.facebook.com/v18.0/{}?fields=status_code&access_token={}",
            container_id, token
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        match result["status_code"].as_str() {
            Some("FINISHED") => Ok(true),
            Some("ERROR") => Err(PlatformError::UploadFailed(
                "Container processing failed".to_string(),
            )),
            Some("EXPIRED") => Err(PlatformError::UploadFailed("Container expired".to_string())),
            _ => Ok(false), // Still processing
        }
    }

    /// Publish the container
    async fn publish_container(
        &self,
        container_id: &str,
        config: &InstagramConfig,
    ) -> Result<String, PlatformError> {
        let token = self.access_token.as_ref().ok_or_else(|| {
            PlatformError::AuthFailed("Instagram access token not configured".to_string())
        })?;

        let url = format!(
            "https://graph.facebook.com/v18.0/{}/media_publish",
            config.business_account_id
        );

        let response = self
            .client
            .post(&url)
            .form(&[
                ("creation_id", container_id),
                ("access_token", token.as_str()),
            ])
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(PlatformError::PublishFailed(format!(
                "Instagram publish error: {}",
                error_text
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        result["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| PlatformError::PublishFailed("No media ID in response".to_string()))
    }
}

#[async_trait]
impl PlatformPoster for InstagramPoster {
    fn platform_name(&self) -> &str {
        "instagram"
    }

    fn is_enabled(&self) -> bool {
        self.config.as_ref().map(|c| c.enabled).unwrap_or(false)
            && self.access_token.is_some()
    }

    async fn post_video(
        &self,
        video_url: &str,
        caption: &str,
        hashtags: &[String],
    ) -> Result<PlatformPostResult, PlatformError> {
        let config = self.config.as_ref().ok_or_else(|| {
            PlatformError::NotConfigured("Instagram".to_string())
        })?;

        if !config.enabled {
            return Err(PlatformError::NotConfigured(
                "Instagram posting is disabled".to_string(),
            ));
        }

        info!("Posting video to Instagram");

        // Build caption with hashtags
        let mut full_caption = caption.to_string();
        let all_hashtags: Vec<_> = config
            .default_hashtags
            .iter()
            .chain(hashtags.iter())
            .collect();

        if !all_hashtags.is_empty() {
            full_caption.push_str("\n\n");
            for tag in all_hashtags {
                if !tag.starts_with('#') {
                    full_caption.push('#');
                }
                full_caption.push_str(tag);
                full_caption.push(' ');
            }
        }

        // Step 1: Create media container
        let container_id = self
            .create_media_container(video_url, &full_caption, config)
            .await?;

        debug!("Created Instagram container: {}", container_id);

        // Step 2: Wait for processing
        let max_attempts = 30;
        for attempt in 0..max_attempts {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            match self.check_container_status(&container_id).await {
                Ok(true) => break,
                Ok(false) => {
                    debug!("Container still processing, attempt {}/{}", attempt + 1, max_attempts);
                }
                Err(e) => return Err(e),
            }

            if attempt == max_attempts - 1 {
                return Err(PlatformError::UploadFailed(
                    "Container processing timed out".to_string(),
                ));
            }
        }

        // Step 3: Publish
        let media_id = self.publish_container(&container_id, config).await?;

        info!("Successfully posted to Instagram: {}", media_id);

        Ok(PlatformPostResult {
            platform: "instagram".to_string(),
            success: true,
            post_id: Some(media_id.clone()),
            post_url: Some(format!(
                "https://www.instagram.com/p/{}/",
                media_id
            )),
            error: None,
        })
    }

    fn video_requirements(&self) -> VideoRequirements {
        VideoRequirements {
            min_duration_secs: 3.0,
            max_duration_secs: 90.0, // Reels max
            aspect_ratios: vec![
                "9:16".to_string(),
                "1:1".to_string(),
                "4:5".to_string(),
            ],
            max_file_size_bytes: 1_073_741_824, // 1GB
            formats: vec!["mp4".to_string(), "mov".to_string()],
        }
    }
}

// ============================================================================
// TikTok Posting Service
// ============================================================================

/// TikTok posting service using TikTok Content Posting API
pub struct TikTokPoster {
    config: Option<TikTokConfig>,
    client: Client,
    access_token: Option<String>,
}

impl TikTokPoster {
    /// Create a new TikTok poster
    pub fn new(config: Option<TikTokConfig>) -> Self {
        let access_token = config.as_ref().and_then(|c| {
            std::env::var(&c.access_token_env).ok()
        });

        Self {
            config,
            client: Client::new(),
            access_token,
        }
    }

    /// Initialize video upload
    async fn init_upload(
        &self,
        config: &TikTokConfig,
    ) -> Result<(String, String), PlatformError> {
        let token = self.access_token.as_ref().ok_or_else(|| {
            PlatformError::AuthFailed("TikTok access token not configured".to_string())
        })?;

        let privacy = match config.privacy_level {
            TikTokPrivacyLevel::PublicToEveryone => "PUBLIC_TO_EVERYONE",
            TikTokPrivacyLevel::FollowersOnly => "MUTUAL_FOLLOW_FRIENDS",
            TikTokPrivacyLevel::FriendsOnly => "FOLLOWER_OF_CREATOR",
            TikTokPrivacyLevel::PrivateToSelf => "SELF_ONLY",
        };

        let response = self
            .client
            .post("https://open.tiktokapis.com/v2/post/publish/video/init/")
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=UTF-8")
            .json(&serde_json::json!({
                "post_info": {
                    "title": "",
                    "privacy_level": privacy,
                    "disable_duet": !config.allow_duet,
                    "disable_comment": !config.allow_comments,
                    "disable_stitch": !config.allow_stitch,
                    "video_cover_timestamp_ms": 1000,
                },
                "source_info": {
                    "source": "PULL_FROM_URL",
                }
            }))
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(PlatformError::UploadFailed(format!(
                "TikTok init error: {}",
                error_text
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        let publish_id = result["data"]["publish_id"]
            .as_str()
            .ok_or_else(|| PlatformError::UploadFailed("No publish_id in response".to_string()))?
            .to_string();

        let upload_url = result["data"]["upload_url"]
            .as_str()
            .ok_or_else(|| PlatformError::UploadFailed("No upload_url in response".to_string()))?
            .to_string();

        Ok((publish_id, upload_url))
    }

    /// Upload video to TikTok
    async fn upload_video(
        &self,
        upload_url: &str,
        video_url: &str,
    ) -> Result<(), PlatformError> {
        // Download video content first
        let video_response = self
            .client
            .get(video_url)
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        let video_bytes = video_response
            .bytes()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        // Upload to TikTok
        let response = self
            .client
            .put(upload_url)
            .header("Content-Type", "video/mp4")
            .header("Content-Length", video_bytes.len())
            .body(video_bytes)
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(PlatformError::UploadFailed(
                "Failed to upload video to TikTok".to_string(),
            ));
        }

        Ok(())
    }

    /// Check publish status
    async fn check_publish_status(
        &self,
        publish_id: &str,
    ) -> Result<Option<String>, PlatformError> {
        let token = self.access_token.as_ref().ok_or_else(|| {
            PlatformError::AuthFailed("TikTok access token not configured".to_string())
        })?;

        let response = self
            .client
            .post("https://open.tiktokapis.com/v2/post/publish/status/fetch/")
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=UTF-8")
            .json(&serde_json::json!({
                "publish_id": publish_id
            }))
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        let status = result["data"]["status"].as_str().unwrap_or("");

        match status {
            "PUBLISH_COMPLETE" => {
                let video_id = result["data"]["video_id"]
                    .as_str()
                    .map(|s| s.to_string());
                Ok(video_id)
            }
            "FAILED" => {
                let error_code = result["data"]["fail_reason"].as_str().unwrap_or("unknown");
                Err(PlatformError::PublishFailed(format!(
                    "TikTok publish failed: {}",
                    error_code
                )))
            }
            _ => Ok(None), // Still processing
        }
    }
}

#[async_trait]
impl PlatformPoster for TikTokPoster {
    fn platform_name(&self) -> &str {
        "tiktok"
    }

    fn is_enabled(&self) -> bool {
        self.config.as_ref().map(|c| c.enabled).unwrap_or(false)
            && self.access_token.is_some()
    }

    async fn post_video(
        &self,
        video_url: &str,
        _caption: &str, // TikTok sets title in init_upload
        _hashtags: &[String], // TikTok doesn't support hashtags in API
    ) -> Result<PlatformPostResult, PlatformError> {
        let config = self.config.as_ref().ok_or_else(|| {
            PlatformError::NotConfigured("TikTok".to_string())
        })?;

        if !config.enabled {
            return Err(PlatformError::NotConfigured(
                "TikTok posting is disabled".to_string(),
            ));
        }

        info!("Posting video to TikTok");

        // Step 1: Initialize upload
        let (publish_id, upload_url) = self.init_upload(config).await?;

        debug!("TikTok upload initialized: {}", publish_id);

        // Step 2: Upload video
        self.upload_video(&upload_url, video_url).await?;

        // Step 3: Wait for publishing to complete
        let max_attempts = 60;
        let mut video_id = None;

        for attempt in 0..max_attempts {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            match self.check_publish_status(&publish_id).await {
                Ok(Some(id)) => {
                    video_id = Some(id);
                    break;
                }
                Ok(None) => {
                    debug!("TikTok publish in progress, attempt {}/{}", attempt + 1, max_attempts);
                }
                Err(e) => return Err(e),
            }
        }

        let video_id = video_id.ok_or_else(|| {
            PlatformError::PublishFailed("TikTok publish timed out".to_string())
        })?;

        info!("Successfully posted to TikTok: {}", video_id);

        Ok(PlatformPostResult {
            platform: "tiktok".to_string(),
            success: true,
            post_id: Some(video_id.clone()),
            post_url: Some(format!(
                "https://www.tiktok.com/@{}/video/{}",
                config.creator_id, video_id
            )),
            error: None,
        })
    }

    fn video_requirements(&self) -> VideoRequirements {
        VideoRequirements {
            min_duration_secs: 1.0,
            max_duration_secs: 180.0, // 3 minutes
            aspect_ratios: vec![
                "9:16".to_string(),
                "1:1".to_string(),
            ],
            max_file_size_bytes: 4_294_967_296, // 4GB
            formats: vec!["mp4".to_string(), "webm".to_string(), "mov".to_string()],
        }
    }
}

// ============================================================================
// Snapchat Posting Service
// ============================================================================

/// Snapchat posting service using Snapchat Marketing API
pub struct SnapchatPoster {
    config: Option<SnapchatConfig>,
    client: Client,
    access_token: Option<String>,
}

impl SnapchatPoster {
    /// Create a new Snapchat poster
    pub fn new(config: Option<SnapchatConfig>) -> Self {
        let access_token = config.as_ref().and_then(|c| {
            std::env::var(&c.access_token_env).ok()
        });

        Self {
            config,
            client: Client::new(),
            access_token,
        }
    }

    /// Upload video to Snapchat
    async fn upload_media(
        &self,
        video_url: &str,
        config: &SnapchatConfig,
    ) -> Result<String, PlatformError> {
        let token = self.access_token.as_ref().ok_or_else(|| {
            PlatformError::AuthFailed("Snapchat access token not configured".to_string())
        })?;

        // First, download the video
        let video_response = self
            .client
            .get(video_url)
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        let video_bytes = video_response
            .bytes()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        // Upload to Snapchat
        let upload_url = format!(
            "https://businessapi.snapchat.com/v1/organizations/{}/media",
            config.organization_id
        );

        let part = reqwest::multipart::Part::bytes(video_bytes.to_vec())
            .file_name("video.mp4")
            .mime_str("video/mp4")
            .map_err(|e| PlatformError::UploadFailed(e.to_string()))?;

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("type", "VIDEO");

        let response = self
            .client
            .post(&upload_url)
            .header("Authorization", format!("Bearer {}", token))
            .multipart(form)
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(PlatformError::UploadFailed(format!(
                "Snapchat upload error: {}",
                error_text
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        result["media"][0]["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| PlatformError::UploadFailed("No media ID in response".to_string()))
    }

    /// Post to Spotlight
    async fn post_to_spotlight(
        &self,
        media_id: &str,
        caption: &str,
        config: &SnapchatConfig,
    ) -> Result<String, PlatformError> {
        let token = self.access_token.as_ref().ok_or_else(|| {
            PlatformError::AuthFailed("Snapchat access token not configured".to_string())
        })?;

        let url = "https://businessapi.snapchat.com/v1/spotlight/submit";

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&serde_json::json!({
                "organization_id": config.organization_id,
                "media_id": media_id,
                "caption": caption,
            }))
            .send()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(PlatformError::PublishFailed(format!(
                "Snapchat Spotlight error: {}",
                error_text
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PlatformError::NetworkError(e.to_string()))?;

        result["submission"]["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| {
                PlatformError::PublishFailed("No submission ID in response".to_string())
            })
    }
}

#[async_trait]
impl PlatformPoster for SnapchatPoster {
    fn platform_name(&self) -> &str {
        "snapchat"
    }

    fn is_enabled(&self) -> bool {
        self.config.as_ref().map(|c| c.enabled).unwrap_or(false)
            && self.access_token.is_some()
    }

    async fn post_video(
        &self,
        video_url: &str,
        caption: &str,
        hashtags: &[String],
    ) -> Result<PlatformPostResult, PlatformError> {
        let config = self.config.as_ref().ok_or_else(|| {
            PlatformError::NotConfigured("Snapchat".to_string())
        })?;

        if !config.enabled {
            return Err(PlatformError::NotConfigured(
                "Snapchat posting is disabled".to_string(),
            ));
        }

        info!("Posting video to Snapchat");

        // Build caption with hashtags
        let mut full_caption = caption.to_string();
        if !hashtags.is_empty() {
            full_caption.push(' ');
            for tag in hashtags {
                if !tag.starts_with('#') {
                    full_caption.push('#');
                }
                full_caption.push_str(tag);
                full_caption.push(' ');
            }
        }

        // Step 1: Upload media
        let media_id = self.upload_media(video_url, config).await?;

        debug!("Snapchat media uploaded: {}", media_id);

        // Step 2: Post to Spotlight (if enabled) or regular story
        let post_id = if config.post_to_spotlight {
            self.post_to_spotlight(&media_id, &full_caption, config)
                .await?
        } else {
            // For regular story posts, the media_id is the post ID
            media_id.clone()
        };

        info!("Successfully posted to Snapchat: {}", post_id);

        Ok(PlatformPostResult {
            platform: "snapchat".to_string(),
            success: true,
            post_id: Some(post_id.clone()),
            post_url: if config.post_to_spotlight {
                Some(format!(
                    "https://www.snapchat.com/spotlight/{}",
                    post_id
                ))
            } else {
                None // Stories don't have permanent URLs
            },
            error: None,
        })
    }

    fn video_requirements(&self) -> VideoRequirements {
        VideoRequirements {
            min_duration_secs: 3.0,
            max_duration_secs: 60.0, // Spotlight max
            aspect_ratios: vec![
                "9:16".to_string(),
            ],
            max_file_size_bytes: 1_073_741_824, // 1GB
            formats: vec!["mp4".to_string(), "mov".to_string()],
        }
    }
}

/// Multi-platform poster that coordinates posting across all enabled platforms
pub struct MultiPlatformPoster {
    instagram: InstagramPoster,
    tiktok: TikTokPoster,
    snapchat: SnapchatPoster,
}

impl MultiPlatformPoster {
    /// Create a new multi-platform poster
    pub fn new(
        instagram_config: Option<InstagramConfig>,
        tiktok_config: Option<TikTokConfig>,
        snapchat_config: Option<SnapchatConfig>,
    ) -> Self {
        Self {
            instagram: InstagramPoster::new(instagram_config),
            tiktok: TikTokPoster::new(tiktok_config),
            snapchat: SnapchatPoster::new(snapchat_config),
        }
    }

    /// Post video to all specified platforms
    pub async fn post_to_platforms(
        &self,
        video_url: &str,
        caption: &str,
        hashtags: &[String],
        platforms: &[crate::config::PlatformTarget],
    ) -> Vec<PlatformPostResult> {
        use crate::config::PlatformTarget;

        let mut results = Vec::new();
        let post_all = platforms.iter().any(|p| matches!(p, PlatformTarget::All));

        // Instagram
        if post_all || platforms.iter().any(|p| matches!(p, PlatformTarget::Instagram)) {
            if self.instagram.is_enabled() {
                match self.instagram.post_video(video_url, caption, hashtags).await {
                    Ok(result) => results.push(result),
                    Err(e) => results.push(PlatformPostResult {
                        platform: "instagram".to_string(),
                        success: false,
                        post_id: None,
                        post_url: None,
                        error: Some(e.to_string()),
                    }),
                }
            } else {
                results.push(PlatformPostResult {
                    platform: "instagram".to_string(),
                    success: false,
                    post_id: None,
                    post_url: None,
                    error: Some("Instagram is not enabled or configured".to_string()),
                });
            }
        }

        // TikTok
        if post_all || platforms.iter().any(|p| matches!(p, PlatformTarget::TikTok)) {
            if self.tiktok.is_enabled() {
                match self.tiktok.post_video(video_url, caption, hashtags).await {
                    Ok(result) => results.push(result),
                    Err(e) => results.push(PlatformPostResult {
                        platform: "tiktok".to_string(),
                        success: false,
                        post_id: None,
                        post_url: None,
                        error: Some(e.to_string()),
                    }),
                }
            } else {
                results.push(PlatformPostResult {
                    platform: "tiktok".to_string(),
                    success: false,
                    post_id: None,
                    post_url: None,
                    error: Some("TikTok is not enabled or configured".to_string()),
                });
            }
        }

        // Snapchat
        if post_all || platforms.iter().any(|p| matches!(p, PlatformTarget::Snapchat)) {
            if self.snapchat.is_enabled() {
                match self.snapchat.post_video(video_url, caption, hashtags).await {
                    Ok(result) => results.push(result),
                    Err(e) => results.push(PlatformPostResult {
                        platform: "snapchat".to_string(),
                        success: false,
                        post_id: None,
                        post_url: None,
                        error: Some(e.to_string()),
                    }),
                }
            } else {
                results.push(PlatformPostResult {
                    platform: "snapchat".to_string(),
                    success: false,
                    post_id: None,
                    post_url: None,
                    error: Some("Snapchat is not enabled or configured".to_string()),
                });
            }
        }

        results
    }

    /// Get enabled platforms
    pub fn enabled_platforms(&self) -> Vec<&str> {
        let mut platforms = Vec::new();
        if self.instagram.is_enabled() {
            platforms.push("instagram");
        }
        if self.tiktok.is_enabled() {
            platforms.push("tiktok");
        }
        if self.snapchat.is_enabled() {
            platforms.push("snapchat");
        }
        platforms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instagram_poster_disabled_by_default() {
        let poster = InstagramPoster::new(None);
        assert!(!poster.is_enabled());
        assert_eq!(poster.platform_name(), "instagram");
    }

    #[test]
    fn test_tiktok_poster_disabled_by_default() {
        let poster = TikTokPoster::new(None);
        assert!(!poster.is_enabled());
        assert_eq!(poster.platform_name(), "tiktok");
    }

    #[test]
    fn test_snapchat_poster_disabled_by_default() {
        let poster = SnapchatPoster::new(None);
        assert!(!poster.is_enabled());
        assert_eq!(poster.platform_name(), "snapchat");
    }

    #[test]
    fn test_video_requirements() {
        let instagram = InstagramPoster::new(None);
        let reqs = instagram.video_requirements();
        assert_eq!(reqs.max_duration_secs, 90.0);
        assert!(reqs.aspect_ratios.contains(&"9:16".to_string()));
    }
}

