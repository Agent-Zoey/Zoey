//! X402 Video Plugin for ZoeyOS
//!
//! This plugin provides x402 payment-gated video generation and social media posting.
//!
//! ## Features
//!
//! - **X402 Payments**: Accept HTTP 402 payments for video generation
//! - **AI Video Generation**: Support for multiple providers (Replicate, Runway, Pika, Luma)
//! - **Multi-Platform Posting**: Post to Instagram, TikTok, and Snapchat
//!
//! ## Platform Configuration
//!
//! Each platform requires its own configuration:
//!
//! ### Instagram
//! - Requires Instagram Business Account connected to Facebook
//! - Uses Instagram Graph API for posting
//! - Supports Reels and regular video posts
//!
//! ### TikTok
//! - Requires TikTok for Developers account
//! - Uses TikTok Content Posting API
//! - Supports various privacy levels
//!
//! ### Snapchat
//! - Requires Snapchat Marketing API access
//! - Supports Stories and Spotlight submissions
//!
//! ## Usage
//!
//! ```ignore
//! use zoey_plugin_x402_video::{X402VideoPlugin, config::X402VideoConfig};
//!
//! let config = X402VideoConfig::default();
//! let plugin = X402VideoPlugin::new(config);
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

// Module declarations
pub mod actions;
pub mod config;
pub mod providers;
pub mod routes;
pub mod services;

// Re-exports
pub use actions::*;
pub use config::*;
pub use zoey_core;
pub use providers::*;
pub use routes::{build_router, get_routes, X402VideoRouteState};
pub use services::*;

// ============================================================================
// Banner Rendering
// ============================================================================

/// Render the plugin banner with configuration status
fn render_x402_video_banner(config: &X402VideoConfig) {
    let cyan = "\x1b[36m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let red = "\x1b[31m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!(
        "{cyan}+{line}+{reset}",
        line = "=".repeat(78),
        cyan = cyan,
        reset = reset
    );

    // ASCII Art Header
    println!(
        "{cyan}|{bold} __  __  _  _   ___   ___   __     __ ___  ___   ___   ___  {reset}{cyan}           |{reset}",
        cyan = cyan, bold = bold, reset = reset
    );
    println!(
        "{cyan}|{bold} \\ \\/ / | || | / _ \\ |__ \\ /  \\ _ / /|_ _||   \\ | __| / _ \\ {reset}{cyan}           |{reset}",
        cyan = cyan, bold = bold, reset = reset
    );
    println!(
        "{cyan}|{bold}  >  <  | __ || (_) |  /_/  | () / /  | | | |) || _| | (_) |{reset}{cyan}           |{reset}",
        cyan = cyan, bold = bold, reset = reset
    );
    println!(
        "{cyan}|{bold} /_/\\_\\ |_||_| \\___/ |___| \\__/_/  |___||___/ |___| \\___/ {reset}{cyan}           |{reset}",
        cyan = cyan, bold = bold, reset = reset
    );

    // Tagline
    println!("{cyan}|{reset}", cyan = cyan, reset = reset);
    println!(
        "{cyan}|  {yellow}Payment-Gated AI Video Generation{reset}  {dim}*{reset}  {yellow}Multi-Platform Posting{reset}           {cyan}|{reset}",
        cyan = cyan, yellow = yellow, dim = dim, reset = reset
    );

    // Separator
    println!(
        "{cyan}+{line}+{reset}",
        line = "-".repeat(78),
        cyan = cyan,
        reset = reset
    );

    // Configuration status
    println!(
        "{cyan}| {bold}Configuration Status:{reset}                                                       {cyan}|{reset}",
        cyan = cyan, bold = bold, reset = reset
    );
    println!(
        "{cyan}+{line}+{reset}",
        line = "-".repeat(78),
        cyan = cyan,
        reset = reset
    );

    // X402 config
    let x402_status = if config.x402.wallet_address.is_empty() {
        format!("{red}NOT CONFIGURED{reset}", red = red, reset = reset)
    } else {
        format!(
            "{green}{}...{}{reset}",
            &config.x402.wallet_address[..6],
            &config.x402.wallet_address[config.x402.wallet_address.len() - 4..],
            green = green,
            reset = reset
        )
    };

    println!(
        "{cyan}|  X402 Wallet:      {status:<56} {cyan}|{reset}",
        status = x402_status,
        cyan = cyan,
        reset = reset
    );

    println!(
        "{cyan}|  Payment Price:    ${:.2} USDC                                                  {cyan}|{reset}",
        (config.x402.default_price_cents as f64) / 100.0,
        cyan = cyan,
        reset = reset
    );

    println!(
        "{cyan}|  Video Provider:   {:<56} {cyan}|{reset}",
        format!("{:?}", config.video_generation.provider),
        cyan = cyan,
        reset = reset
    );

    // Platform status
    println!(
        "{cyan}+{line}+{reset}",
        line = "-".repeat(78),
        cyan = cyan,
        reset = reset
    );
    println!(
        "{cyan}| {bold}Platform Status:{reset}                                                            {cyan}|{reset}",
        cyan = cyan, bold = bold, reset = reset
    );
    println!(
        "{cyan}+{line}+{reset}",
        line = "-".repeat(78),
        cyan = cyan,
        reset = reset
    );

    // Instagram
    let ig_status = match &config.platforms.instagram {
        Some(ig) if ig.enabled => format!(
            "{green}ENABLED{reset} ({})",
            if ig.post_as_reels { "Reels" } else { "Video" },
            green = green,
            reset = reset
        ),
        Some(_) => format!("{yellow}DISABLED{reset}", yellow = yellow, reset = reset),
        None => format!("{dim}NOT CONFIGURED{reset}", dim = dim, reset = reset),
    };
    println!(
        "{cyan}|  Instagram:        {status:<56} {cyan}|{reset}",
        status = ig_status,
        cyan = cyan,
        reset = reset
    );

    // TikTok
    let tt_status = match &config.platforms.tiktok {
        Some(tt) if tt.enabled => format!(
            "{green}ENABLED{reset} ({:?})",
            tt.privacy_level,
            green = green,
            reset = reset
        ),
        Some(_) => format!("{yellow}DISABLED{reset}", yellow = yellow, reset = reset),
        None => format!("{dim}NOT CONFIGURED{reset}", dim = dim, reset = reset),
    };
    println!(
        "{cyan}|  TikTok:           {status:<56} {cyan}|{reset}",
        status = tt_status,
        cyan = cyan,
        reset = reset
    );

    // Snapchat
    let sc_status = match &config.platforms.snapchat {
        Some(sc) if sc.enabled => format!(
            "{green}ENABLED{reset} ({})",
            if sc.post_to_spotlight {
                "Spotlight"
            } else {
                "Story"
            },
            green = green,
            reset = reset
        ),
        Some(_) => format!("{yellow}DISABLED{reset}", yellow = yellow, reset = reset),
        None => format!("{dim}NOT CONFIGURED{reset}", dim = dim, reset = reset),
    };
    println!(
        "{cyan}|  Snapchat:         {status:<56} {cyan}|{reset}",
        status = sc_status,
        cyan = cyan,
        reset = reset
    );

    // Bottom border
    println!(
        "{cyan}+{line}+{reset}",
        line = "=".repeat(78),
        cyan = cyan,
        reset = reset
    );
}

// ============================================================================
// Plugin Implementation
// ============================================================================

/// X402 Video Plugin implementation
pub struct X402VideoPlugin {
    config: X402VideoConfig,
    video_service: Arc<VideoGenerationService>,
    payment_service: Arc<X402PaymentService>,
    poster: Arc<MultiPlatformPoster>,
}

impl X402VideoPlugin {
    /// Create a new X402 Video plugin with the given configuration
    pub fn new(config: X402VideoConfig) -> Self {
        let video_service = Arc::new(VideoGenerationService::new(
            config.video_generation.clone(),
        ));

        let payment_service = Arc::new(X402PaymentService::new(config.x402.clone()));

        let poster = Arc::new(MultiPlatformPoster::new(
            config.platforms.instagram.clone(),
            config.platforms.tiktok.clone(),
            config.platforms.snapchat.clone(),
        ));

        Self {
            config,
            video_service,
            payment_service,
            poster,
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(X402VideoConfig::default())
    }

    /// Build configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = X402VideoConfig::default();

        // X402 config from env
        if let Ok(wallet) = std::env::var("X402_WALLET_ADDRESS") {
            config.x402.wallet_address = wallet;
        }
        if let Ok(price) = std::env::var("X402_PRICE_CENTS") {
            if let Ok(price_val) = price.parse() {
                config.x402.default_price_cents = price_val;
            }
        }
        if let Ok(facilitator_url) = std::env::var("X402_FACILITATOR_URL") {
            config.x402.facilitator_url = facilitator_url;
        }
        // Facilitator pay_to address (for x402scan tracking)
        // Defaults to PayAI's facilitator address on Base
        if let Ok(facilitator_address) = std::env::var("X402_FACILITATOR_PAY_TO_ADDRESS") {
            config.x402.facilitator_pay_to_address = facilitator_address;
        }

        // Video provider from env (default is Sora)
        if let Ok(provider) = std::env::var("VIDEO_PROVIDER") {
            config.video_generation.provider = match provider.to_lowercase().as_str() {
                "runway" => VideoProvider::Runway,
                "pika" => VideoProvider::Pika,
                "luma" => VideoProvider::Luma,
                "sora" | "openai" => VideoProvider::Sora,
                "replicate" => VideoProvider::Replicate,
                _ => VideoProvider::Sora, // Default to Sora
            };
        }

        // Set API key and URL based on provider
        match config.video_generation.provider {
            VideoProvider::Sora => {
                config.video_generation.api_key_env = "OPENAI_API_KEY".to_string();
                config.video_generation.api_url = "https://api.openai.com/v1".to_string();
            }
            VideoProvider::Replicate => {
                config.video_generation.api_key_env = "REPLICATE_API_KEY".to_string();
                config.video_generation.api_url = "https://api.replicate.com/v1".to_string();
            }
            VideoProvider::Runway => {
                config.video_generation.api_key_env = "RUNWAY_API_KEY".to_string();
                config.video_generation.api_url = "https://api.runwayml.com/v1".to_string();
            }
            VideoProvider::Pika => {
                config.video_generation.api_key_env = "PIKA_API_KEY".to_string();
                config.video_generation.api_url = "https://api.pika.art/v1".to_string();
            }
            VideoProvider::Luma => {
                config.video_generation.api_key_env = "LUMA_API_KEY".to_string();
                config.video_generation.api_url = "https://api.lumalabs.ai/v1".to_string();
            }
            VideoProvider::Custom => {
                // Custom provider - use env vars for URL and key
                if let Ok(url) = std::env::var("VIDEO_API_URL") {
                    config.video_generation.api_url = url;
                }
                if let Ok(key_env) = std::env::var("VIDEO_API_KEY_ENV") {
                    config.video_generation.api_key_env = key_env;
                }
            }
        }

        // Instagram config from env
        if std::env::var("INSTAGRAM_ACCESS_TOKEN").is_ok() {
            let mut ig_config = InstagramConfig::default();
            ig_config.enabled = true;
            if let Ok(account_id) = std::env::var("INSTAGRAM_BUSINESS_ACCOUNT_ID") {
                ig_config.business_account_id = account_id;
            }
            config.platforms.instagram = Some(ig_config);
        }

        // TikTok config from env
        if std::env::var("TIKTOK_ACCESS_TOKEN").is_ok() {
            let mut tt_config = TikTokConfig::default();
            tt_config.enabled = true;
            if let Ok(creator_id) = std::env::var("TIKTOK_CREATOR_ID") {
                tt_config.creator_id = creator_id;
            }
            config.platforms.tiktok = Some(tt_config);
        }

        // Snapchat config from env
        if std::env::var("SNAPCHAT_ACCESS_TOKEN").is_ok() {
            let mut sc_config = SnapchatConfig::default();
            sc_config.enabled = true;
            if let Ok(org_id) = std::env::var("SNAPCHAT_ORGANIZATION_ID") {
                sc_config.organization_id = org_id;
            }
            config.platforms.snapchat = Some(sc_config);
        }

        Self::new(config)
    }

    /// Create route state for HTTP endpoints
    pub fn create_route_state(&self) -> Arc<X402VideoRouteState> {
        Arc::new(X402VideoRouteState {
            video_service: self.video_service.clone(),
            payment_service: self.payment_service.clone(),
            poster: self.poster.clone(),
            config: self.config.clone(),
            pending_jobs: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }

    /// Build an axum Router for the plugin endpoints
    pub fn build_router(&self) -> axum::Router {
        let state = self.create_route_state();
        routes::build_router(state)
    }
}

impl Default for X402VideoPlugin {
    fn default() -> Self {
        Self::default_config()
    }
}

#[async_trait]
impl Plugin for X402VideoPlugin {
    fn name(&self) -> &str {
        "x402-video"
    }

    fn description(&self) -> &str {
        "X402 payment-gated AI video generation and multi-platform social media posting"
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        let generate_action = Arc::new(GenerateVideoAction::new(
            self.video_service.clone(),
            self.payment_service.clone(),
            self.config.clone(),
        ));

        let post_action = Arc::new(PostVideoAction::new(
            self.poster.clone(),
            self.config.clone(),
        ));

        let combined_action = Arc::new(GenerateAndPostVideoAction::new(
            generate_action.clone(),
            post_action.clone(),
        ));

        vec![generate_action, post_action, combined_action]
    }

    fn providers(&self) -> Vec<Arc<dyn Provider>> {
        vec![
            Arc::new(VideoPlatformsProvider::new(
                self.poster.clone(),
                self.config.clone(),
            )),
            Arc::new(X402PaymentProvider::new(self.config.clone())),
        ]
    }

    fn services(&self) -> Vec<Arc<dyn Service>> {
        // Services are managed internally, not exposed as plugin services
        vec![]
    }

    fn routes(&self) -> Vec<Route> {
        routes::get_routes()
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        render_x402_video_banner(&self.config);

        // Initialize the video generation service (loads API key from env)
        {
            // Get a mutable reference through interior mutability
            // Since VideoGenerationService uses RwLock internally, we can call initialize
            use zoey_core::Service;
            let mut video_service = VideoGenerationService::new(self.config.video_generation.clone());
            if let Err(e) = video_service.initialize(runtime.clone()).await {
                warn!("Failed to initialize video service: {}", e);
            }
            // Update the shared service's state with the API key
            // The service loads the key during initialize()
        }
        
        // Also initialize on the shared instance
        {
            use zoey_core::Service;
            // We need to get mutable access - use a workaround
            let api_key = std::env::var(&self.config.video_generation.api_key_env).ok();
            if let Some(key) = api_key {
                info!("Loaded API key from {} for video generation", self.config.video_generation.api_key_env);
                // Store it in the video service
                self.video_service.set_api_key(key).await;
            } else {
                warn!(
                    "Video generation API key not found in environment variable '{}'",
                    self.config.video_generation.api_key_env
                );
            }
        }

        // Validate configuration
        if self.config.x402.wallet_address.is_empty() {
            warn!("X402 wallet address not configured - payments will be rejected");
        }

        let enabled_platforms = self.poster.enabled_platforms();
        if enabled_platforms.is_empty() {
            warn!("No social media platforms are enabled");
        } else {
            info!(
                "Enabled platforms: {}",
                enabled_platforms.join(", ")
            );
        }

        info!(
            "Video generation provider: {:?}",
            self.config.video_generation.provider
        );

        // Start the HTTP server for x402-video routes
        let port: u16 = std::env::var("X402_VIDEO_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9402);

        let host = std::env::var("X402_VIDEO_HOST")
            .unwrap_or_else(|_| "0.0.0.0".to_string());

        let route_state = self.create_route_state();
        let router = routes::build_router(route_state);

        let addr = format!("{}:{}", host, port);
        
        // Check if port is available
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                info!("Starting X402 Video API server on http://{}", addr);
                
                tokio::spawn(async move {
                    if let Err(e) = axum::serve(listener, router).await {
                        eprintln!("[x402-video] Server error: {}", e);
                    }
                });
            }
            Err(e) => {
                warn!("Could not start X402 Video API server on {}: {} - routes will not be available", addr, e);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_creation() {
        let plugin = X402VideoPlugin::default();
        assert_eq!(plugin.name(), "x402-video");
        assert!(!plugin.description().is_empty());
    }

    #[test]
    fn test_plugin_components() {
        let plugin = X402VideoPlugin::default();

        let actions = plugin.actions();
        assert_eq!(actions.len(), 3);

        let providers = plugin.providers();
        assert_eq!(providers.len(), 2);
    }

    #[test]
    fn test_action_names() {
        let plugin = X402VideoPlugin::default();
        let actions = plugin.actions();

        let names: Vec<&str> = actions.iter().map(|a| a.name()).collect();
        assert!(names.contains(&"GENERATE_VIDEO"));
        assert!(names.contains(&"POST_VIDEO"));
        assert!(names.contains(&"GENERATE_AND_POST_VIDEO"));
    }

    #[test]
    fn test_provider_names() {
        let plugin = X402VideoPlugin::default();
        let providers = plugin.providers();

        let names: Vec<&str> = providers.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"video_platforms"));
        assert!(names.contains(&"x402_payment"));
    }
}

