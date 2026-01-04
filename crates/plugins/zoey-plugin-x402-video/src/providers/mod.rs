//! Providers for X402 Video Plugin
//!
//! Provides context about video generation capabilities and platform status.

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

use crate::config::X402VideoConfig;
use crate::services::MultiPlatformPoster;

/// Provider that supplies information about available video platforms
pub struct VideoPlatformsProvider {
    poster: Arc<MultiPlatformPoster>,
    config: X402VideoConfig,
}

impl VideoPlatformsProvider {
    /// Create a new video platforms provider
    pub fn new(poster: Arc<MultiPlatformPoster>, config: X402VideoConfig) -> Self {
        Self { poster, config }
    }
}

#[async_trait]
impl Provider for VideoPlatformsProvider {
    fn name(&self) -> &str {
        "video_platforms"
    }

    fn description(&self) -> Option<String> {
        Some("Provides information about enabled video posting platforms".to_string())
    }

    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        let enabled = self.poster.enabled_platforms();

        let mut text = String::from("Available video posting platforms:\n");

        if enabled.is_empty() {
            text.push_str("No platforms are currently enabled.\n");
        } else {
            for platform in &enabled {
                text.push_str(&format!("- {} (enabled)\n", platform));
            }
        }

        // Add pricing info
        text.push_str(&format!(
            "\nVideo generation pricing: ${:.2} USDC per video\n",
            (self.config.x402.default_price_cents as f64) / 100.0
        ));

        text.push_str(&format!(
            "Video provider: {:?}\n",
            self.config.video_generation.provider
        ));

        text.push_str(&format!(
            "Default duration: {} seconds\n",
            self.config.video_generation.default_duration_secs
        ));

        let mut data = std::collections::HashMap::new();
        data.insert(
            "enabled_platforms".to_string(),
            serde_json::json!(enabled),
        );
        data.insert(
            "price_cents".to_string(),
            serde_json::json!(self.config.x402.default_price_cents),
        );
        data.insert(
            "video_provider".to_string(),
            serde_json::json!(format!("{:?}", self.config.video_generation.provider)),
        );

        Ok(ProviderResult {
            text: Some(text),
            values: Some({
                let mut values = std::collections::HashMap::new();
                values.insert("enabled_platforms".to_string(), enabled.join(", "));
                values.insert(
                    "price_usd".to_string(),
                    format!("{:.2}", (self.config.x402.default_price_cents as f64) / 100.0),
                );
                values
            }),
            data: Some(data),
        })
    }
}

/// Provider that supplies x402 payment information
pub struct X402PaymentProvider {
    config: X402VideoConfig,
}

impl X402PaymentProvider {
    /// Create a new x402 payment provider
    pub fn new(config: X402VideoConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Provider for X402PaymentProvider {
    fn name(&self) -> &str {
        "x402_payment"
    }

    fn description(&self) -> Option<String> {
        Some("Provides x402 payment protocol information for video generation".to_string())
    }

    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        let text = format!(
            "X402 Payment Information:\n\
            - Payment network: {}\n\
            - Accepted tokens: {}\n\
            - Wallet address: {}\n\
            - Default price: ${:.2}\n\
            - Payment timeout: {} seconds\n",
            self.config.x402.supported_networks.join(", "),
            self.config.x402.supported_tokens.join(", "),
            if self.config.x402.wallet_address.is_empty() {
                "Not configured"
            } else {
                &self.config.x402.wallet_address
            },
            (self.config.x402.default_price_cents as f64) / 100.0,
            self.config.x402.payment_timeout_secs
        );

        let mut data = std::collections::HashMap::new();
        data.insert(
            "networks".to_string(),
            serde_json::json!(self.config.x402.supported_networks),
        );
        data.insert(
            "tokens".to_string(),
            serde_json::json!(self.config.x402.supported_tokens),
        );
        data.insert(
            "wallet_address".to_string(),
            serde_json::json!(self.config.x402.wallet_address),
        );
        data.insert(
            "price_cents".to_string(),
            serde_json::json!(self.config.x402.default_price_cents),
        );

        Ok(ProviderResult {
            text: Some(text),
            values: Some({
                let mut values = std::collections::HashMap::new();
                values.insert(
                    "payment_network".to_string(),
                    self.config.x402.supported_networks.first().cloned().unwrap_or_default(),
                );
                values.insert(
                    "payment_token".to_string(),
                    self.config.x402.supported_tokens.first().cloned().unwrap_or_default(),
                );
                values
            }),
            data: Some(data),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_video_platforms_provider() {
        let poster = MultiPlatformPoster::new(None, None, None);
        let config = X402VideoConfig::default();
        let provider = VideoPlatformsProvider::new(Arc::new(poster), config);

        assert_eq!(provider.name(), "video_platforms");
        assert!(provider.description().is_some());
    }

    #[tokio::test]
    async fn test_x402_payment_provider() {
        let config = X402VideoConfig::default();
        let provider = X402PaymentProvider::new(config);

        assert_eq!(provider.name(), "x402_payment");
        assert!(provider.description().is_some());
    }
}

