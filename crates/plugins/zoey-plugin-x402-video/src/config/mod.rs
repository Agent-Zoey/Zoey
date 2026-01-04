//! Configuration types for X402 Video Plugin
//!
//! Supports multiple platform providers: Instagram, TikTok, Snapchat

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X402VideoConfig {
    /// X402 payment configuration
    pub x402: X402Config,

    /// Video generation configuration
    pub video_generation: VideoGenerationConfig,

    /// Platform-specific configurations
    pub platforms: PlatformConfigs,
}

impl Default for X402VideoConfig {
    fn default() -> Self {
        Self {
            x402: X402Config::default(),
            video_generation: VideoGenerationConfig::default(),
            platforms: PlatformConfigs::default(),
        }
    }
}

/// X402 HTTP Payment Protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X402Config {
    /// Payment facilitator URL
    pub facilitator_url: String,

    /// Facilitator address for payment routing (x402scan-tracked)
    /// Payments go to this address, and the facilitator settles to wallet_address
    /// Default is PayAI's facilitator address on Base
    pub facilitator_pay_to_address: String,

    /// Wallet address to receive settlement from facilitator
    pub wallet_address: String,

    /// Private key for signing (encrypted or from env)
    pub private_key_env: String,

    /// Supported payment networks (e.g., "base", "ethereum")
    pub supported_networks: Vec<String>,

    /// Supported tokens (e.g., "USDC", "ETH")
    pub supported_tokens: Vec<String>,

    /// Default price in USD cents for video generation
    pub default_price_cents: u64,

    /// Maximum payment timeout in seconds
    pub payment_timeout_secs: u64,
}

/// PayAI facilitator address on Base (tracked by x402scan)
/// See: https://github.com/Merit-Systems/x402scan/blob/main/packages/external/facilitators/src/facilitators/payai.ts
pub const PAYAI_FACILITATOR_ADDRESS_BASE: &str = "0xc6699d2aada6c36dfea5c248dd70f9cb0235cb63";

impl Default for X402Config {
    fn default() -> Self {
        Self {
            facilitator_url: "https://facilitator.payai.network".to_string(),
            // PayAI facilitator address on Base - payments go here for x402scan tracking
            facilitator_pay_to_address: PAYAI_FACILITATOR_ADDRESS_BASE.to_string(),
            wallet_address: String::new(),
            private_key_env: "X402_PRIVATE_KEY".to_string(),
            supported_networks: vec!["base".to_string()],
            supported_tokens: vec!["USDC".to_string()],
            default_price_cents: 100, // $1.00 default
            payment_timeout_secs: 300,
        }
    }
}

/// Video generation service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGenerationConfig {
    /// Video generation provider (e.g., "runway", "pika", "replicate")
    pub provider: VideoProvider,

    /// API endpoint
    pub api_url: String,

    /// API key environment variable name
    pub api_key_env: String,

    /// Default video duration in seconds
    pub default_duration_secs: u32,

    /// Default resolution
    pub default_resolution: VideoResolution,

    /// Maximum video duration in seconds
    pub max_duration_secs: u32,

    /// Webhook URL for async generation callbacks
    pub webhook_url: Option<String>,
}

impl Default for VideoGenerationConfig {
    fn default() -> Self {
        Self {
            provider: VideoProvider::Sora,
            api_url: "https://api.openai.com/v1".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            default_duration_secs: 8,
            default_resolution: VideoResolution::HD720p,
            max_duration_secs: 20,
            webhook_url: None,
        }
    }
}

/// Supported video generation providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoProvider {
    /// Runway ML video generation
    Runway,
    /// Pika Labs video generation
    Pika,
    /// Replicate API (supports multiple models)
    Replicate,
    /// Luma AI Dream Machine
    Luma,
    /// OpenAI Sora video generation
    Sora,
    /// Custom provider
    Custom,
}

/// Video resolution options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoResolution {
    /// 480p (854x480)
    SD480p,
    /// 720p (1280x720)
    HD720p,
    /// 1080p (1920x1080)
    FullHD1080p,
    /// 4K (3840x2160)
    UHD4K,
    /// 9:16 vertical (1080x1920)
    Vertical1080p,
    /// 9:16 vertical (720x1280)
    Vertical720p,
}

impl VideoResolution {
    /// Get width and height for this resolution
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::SD480p => (854, 480),
            Self::HD720p => (1280, 720),
            Self::FullHD1080p => (1920, 1080),
            Self::UHD4K => (3840, 2160),
            Self::Vertical1080p => (1080, 1920),
            Self::Vertical720p => (720, 1280),
        }
    }

    /// Check if this is a vertical (portrait) resolution
    pub fn is_vertical(&self) -> bool {
        matches!(self, Self::Vertical1080p | Self::Vertical720p)
    }
}

/// Container for all platform configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformConfigs {
    /// Instagram configuration
    pub instagram: Option<InstagramConfig>,

    /// TikTok configuration
    pub tiktok: Option<TikTokConfig>,

    /// Snapchat configuration
    pub snapchat: Option<SnapchatConfig>,
}

// ============================================================================
// Instagram Configuration
// ============================================================================

/// Instagram platform configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramConfig {
    /// Whether this platform is enabled
    pub enabled: bool,

    /// Instagram Business Account ID
    pub business_account_id: String,

    /// Facebook App ID (for Instagram Graph API)
    pub facebook_app_id: String,

    /// Access token environment variable
    pub access_token_env: String,

    /// Default hashtags to include
    pub default_hashtags: Vec<String>,

    /// Whether to post as Reels (short-form video)
    pub post_as_reels: bool,

    /// Whether to also share to Facebook
    pub share_to_facebook: bool,

    /// Custom posting settings
    pub posting_settings: InstagramPostingSettings,
}

impl Default for InstagramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            business_account_id: String::new(),
            facebook_app_id: String::new(),
            access_token_env: "INSTAGRAM_ACCESS_TOKEN".to_string(),
            default_hashtags: vec![],
            post_as_reels: true,
            share_to_facebook: false,
            posting_settings: InstagramPostingSettings::default(),
        }
    }
}

/// Instagram-specific posting settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramPostingSettings {
    /// Enable location tagging
    pub enable_location: bool,

    /// Default location ID (if any)
    pub default_location_id: Option<String>,

    /// Enable user tagging
    pub enable_user_tags: bool,

    /// Cover image frame (0.0 - 1.0, position in video for thumbnail)
    pub cover_frame_position: f32,
}

impl Default for InstagramPostingSettings {
    fn default() -> Self {
        Self {
            enable_location: false,
            default_location_id: None,
            enable_user_tags: false,
            cover_frame_position: 0.0,
        }
    }
}

// ============================================================================
// TikTok Configuration
// ============================================================================

/// TikTok platform configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TikTokConfig {
    /// Whether this platform is enabled
    pub enabled: bool,

    /// TikTok Open Platform App Key
    pub app_key: String,

    /// TikTok Open Platform App Secret env var
    pub app_secret_env: String,

    /// Access token environment variable
    pub access_token_env: String,

    /// Creator username or ID
    pub creator_id: String,

    /// Default hashtags
    pub default_hashtags: Vec<String>,

    /// Privacy level for posts
    pub privacy_level: TikTokPrivacyLevel,

    /// Whether to allow comments
    pub allow_comments: bool,

    /// Whether to allow duet
    pub allow_duet: bool,

    /// Whether to allow stitch
    pub allow_stitch: bool,

    /// Posting settings
    pub posting_settings: TikTokPostingSettings,
}

impl Default for TikTokConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            app_key: String::new(),
            app_secret_env: "TIKTOK_APP_SECRET".to_string(),
            access_token_env: "TIKTOK_ACCESS_TOKEN".to_string(),
            creator_id: String::new(),
            default_hashtags: vec![],
            privacy_level: TikTokPrivacyLevel::PublicToEveryone,
            allow_comments: true,
            allow_duet: true,
            allow_stitch: true,
            posting_settings: TikTokPostingSettings::default(),
        }
    }
}

/// TikTok privacy levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TikTokPrivacyLevel {
    /// Visible to everyone
    PublicToEveryone,
    /// Visible to followers only
    FollowersOnly,
    /// Visible to friends only
    FriendsOnly,
    /// Only visible to creator
    PrivateToSelf,
}

/// TikTok-specific posting settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TikTokPostingSettings {
    /// Enable branded content disclosure
    pub branded_content: bool,

    /// Enable commercial content disclosure
    pub commercial_content: bool,

    /// Auto-add disclosure label
    pub auto_disclosure: bool,
}

impl Default for TikTokPostingSettings {
    fn default() -> Self {
        Self {
            branded_content: false,
            commercial_content: false,
            auto_disclosure: false,
        }
    }
}

// ============================================================================
// Snapchat Configuration
// ============================================================================

/// Snapchat platform configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapchatConfig {
    /// Whether this platform is enabled
    pub enabled: bool,

    /// Snapchat Marketing API client ID
    pub client_id: String,

    /// Client secret environment variable
    pub client_secret_env: String,

    /// Access token environment variable
    pub access_token_env: String,

    /// Organization ID
    pub organization_id: String,

    /// Default story settings
    pub story_settings: SnapchatStorySettings,

    /// Whether to post to Spotlight (public discovery)
    pub post_to_spotlight: bool,
}

impl Default for SnapchatConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            client_id: String::new(),
            client_secret_env: "SNAPCHAT_CLIENT_SECRET".to_string(),
            access_token_env: "SNAPCHAT_ACCESS_TOKEN".to_string(),
            organization_id: String::new(),
            story_settings: SnapchatStorySettings::default(),
            post_to_spotlight: false,
        }
    }
}

/// Snapchat story-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapchatStorySettings {
    /// Story duration in hours (max 24)
    pub story_duration_hours: u8,

    /// Allow story replies
    pub allow_replies: bool,

    /// Allow story sharing
    pub allow_sharing: bool,

    /// Enable viewer list
    pub show_viewer_list: bool,
}

impl Default for SnapchatStorySettings {
    fn default() -> Self {
        Self {
            story_duration_hours: 24,
            allow_replies: true,
            allow_sharing: true,
            show_viewer_list: true,
        }
    }
}

// ============================================================================
// Platform-agnostic types
// ============================================================================

/// Video post request with platform selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoPostRequest {
    /// Video generation prompt or description
    pub prompt: String,

    /// Optional initial image for img2vid
    pub initial_image_url: Option<String>,

    /// Caption/description for the post
    pub caption: String,

    /// Additional hashtags (merged with defaults)
    pub hashtags: Vec<String>,

    /// Target platforms to post to
    pub platforms: Vec<PlatformTarget>,

    /// Video generation options
    pub video_options: VideoOptions,

    /// X402 payment proof (if pre-paid)
    pub payment_proof: Option<String>,
}

/// Target platform specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlatformTarget {
    Instagram,
    TikTok,
    Snapchat,
    All,
}

/// Helper to deserialize a value that might be a string or number
mod flexible_number {
    use serde::{self, Deserialize, Deserializer};
    
    pub fn deserialize_option_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrNumber {
            String(String),
            Number(u32),
        }
        
        let opt: Option<StringOrNumber> = Option::deserialize(deserializer)?;
        match opt {
            None => Ok(None),
            Some(StringOrNumber::Number(n)) => Ok(Some(n)),
            Some(StringOrNumber::String(s)) => s.parse::<u32>()
                .map(Some)
                .map_err(|_| serde::de::Error::custom(format!("Invalid number: {}", s))),
        }
    }
    
    pub fn deserialize_option_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrNumber {
            String(String),
            Number(i64),
        }
        
        let opt: Option<StringOrNumber> = Option::deserialize(deserializer)?;
        match opt {
            None => Ok(None),
            Some(StringOrNumber::Number(n)) => Ok(Some(n)),
            Some(StringOrNumber::String(s)) => s.parse::<i64>()
                .map(Some)
                .map_err(|_| serde::de::Error::custom(format!("Invalid number: {}", s))),
        }
    }
}

/// Video generation options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoOptions {
    /// Duration in seconds (accepts both string "5" and number 5)
    #[serde(default, deserialize_with = "flexible_number::deserialize_option_u32")]
    pub duration_secs: Option<u32>,

    /// Resolution
    #[serde(default)]
    pub resolution: Option<VideoResolution>,

    /// Aspect ratio (overrides resolution)
    #[serde(default)]
    pub aspect_ratio: Option<String>,

    /// Style/model to use
    #[serde(default)]
    pub style: Option<String>,

    /// Seed for reproducibility (accepts both string and number)
    #[serde(default, deserialize_with = "flexible_number::deserialize_option_i64")]
    pub seed: Option<i64>,

    /// Additional model-specific parameters
    #[serde(default)]
    pub extra_params: HashMap<String, serde_json::Value>,
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            duration_secs: None,
            resolution: None,
            aspect_ratio: None,
            style: None,
            seed: None,
            extra_params: HashMap::new(),
        }
    }
}

/// Result of a video post operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoPostResult {
    /// Whether the operation succeeded
    pub success: bool,

    /// Generated video URL
    pub video_url: Option<String>,

    /// Platform-specific post results
    pub platform_results: Vec<PlatformPostResult>,

    /// Error message if failed
    pub error: Option<String>,

    /// X402 payment receipt
    pub payment_receipt: Option<PaymentReceipt>,
}

/// Result of posting to a single platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformPostResult {
    /// Platform name
    pub platform: String,

    /// Whether posting succeeded
    pub success: bool,

    /// Post ID on the platform
    pub post_id: Option<String>,

    /// Post URL on the platform
    pub post_url: Option<String>,

    /// Error message if failed
    pub error: Option<String>,
}

/// X402 payment receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentReceipt {
    /// Transaction hash
    pub tx_hash: String,

    /// Network used
    pub network: String,

    /// Token used
    pub token: String,

    /// Amount paid (in smallest unit)
    pub amount: String,

    /// Payer address
    pub payer: String,

    /// Timestamp
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = X402VideoConfig::default();
        assert_eq!(config.x402.default_price_cents, 100);
        assert_eq!(config.video_generation.provider, VideoProvider::Sora);
        assert_eq!(config.video_generation.api_key_env, "OPENAI_API_KEY");
    }

    #[test]
    fn test_video_resolution_dimensions() {
        assert_eq!(VideoResolution::HD720p.dimensions(), (1280, 720));
        assert_eq!(VideoResolution::Vertical1080p.dimensions(), (1080, 1920));
        assert!(VideoResolution::Vertical1080p.is_vertical());
        assert!(!VideoResolution::HD720p.is_vertical());
    }

    #[test]
    fn test_platform_configs_default() {
        let platforms = PlatformConfigs::default();
        assert!(platforms.instagram.is_none());
        assert!(platforms.tiktok.is_none());
        assert!(platforms.snapchat.is_none());
    }
}

