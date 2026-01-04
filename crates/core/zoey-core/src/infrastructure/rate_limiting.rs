//! Rate limiting tiers
//!
//! Provides tiered rate limiting for multi-tenant deployments:
//! - Per-user limits
//! - Tier-based quotas
//! - Token bucket algorithm
//! - Burst allowance

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Rate limit tiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitTier {
    /// Free tier with basic limits
    Free,
    /// Basic paid tier
    Basic,
    /// Premium tier with higher limits
    Premium,
    /// Enterprise tier with custom limits
    Enterprise,
    /// Unlimited (for internal use)
    Unlimited,
}

impl Default for RateLimitTier {
    fn default() -> Self {
        Self::Free
    }
}

/// Rate limit configuration for a tier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Requests per minute
    pub requests_per_minute: u32,
    /// Tokens per minute (for LLM calls)
    pub tokens_per_minute: u32,
    /// Burst allowance (additional requests allowed in short bursts)
    pub burst_allowance: u32,
    /// Maximum concurrent requests
    pub max_concurrent: u32,
    /// Priority in queue (higher = processed first)
    pub priority: u32,
    /// Cost multiplier for billing
    pub cost_multiplier: f64,
}

impl TierConfig {
    /// Create a new tier configuration
    pub fn new(requests_per_minute: u32, tokens_per_minute: u32) -> Self {
        Self {
            requests_per_minute,
            tokens_per_minute,
            burst_allowance: requests_per_minute / 10,
            max_concurrent: 5,
            priority: 0,
            cost_multiplier: 1.0,
        }
    }
}

impl Default for TierConfig {
    fn default() -> Self {
        Self::new(60, 10000)
    }
}

/// Default tier configurations
pub fn default_tier_configs() -> HashMap<RateLimitTier, TierConfig> {
    let mut configs = HashMap::new();

    configs.insert(
        RateLimitTier::Free,
        TierConfig {
            requests_per_minute: 10,
            tokens_per_minute: 5000,
            burst_allowance: 2,
            max_concurrent: 2,
            priority: 0,
            cost_multiplier: 1.0,
        },
    );

    configs.insert(
        RateLimitTier::Basic,
        TierConfig {
            requests_per_minute: 60,
            tokens_per_minute: 50000,
            burst_allowance: 10,
            max_concurrent: 5,
            priority: 10,
            cost_multiplier: 1.0,
        },
    );

    configs.insert(
        RateLimitTier::Premium,
        TierConfig {
            requests_per_minute: 300,
            tokens_per_minute: 200000,
            burst_allowance: 50,
            max_concurrent: 20,
            priority: 50,
            cost_multiplier: 0.9,
        },
    );

    configs.insert(
        RateLimitTier::Enterprise,
        TierConfig {
            requests_per_minute: 1000,
            tokens_per_minute: 1000000,
            burst_allowance: 200,
            max_concurrent: 100,
            priority: 100,
            cost_multiplier: 0.7,
        },
    );

    configs.insert(
        RateLimitTier::Unlimited,
        TierConfig {
            requests_per_minute: u32::MAX,
            tokens_per_minute: u32::MAX,
            burst_allowance: u32::MAX,
            max_concurrent: u32::MAX,
            priority: 1000,
            cost_multiplier: 0.0,
        },
    );

    configs
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max_tokens: u32, refill_per_minute: u32) -> Self {
        Self {
            tokens: max_tokens as f64,
            max_tokens: max_tokens as f64,
            refill_rate: refill_per_minute as f64 / 60.0,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }

    fn try_consume(&mut self, amount: f64) -> bool {
        self.refill();
        if self.tokens >= amount {
            self.tokens -= amount;
            true
        } else {
            false
        }
    }

    fn available(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    fn time_until_available(&mut self, amount: f64) -> Duration {
        self.refill();
        if self.tokens >= amount {
            Duration::ZERO
        } else {
            let needed = amount - self.tokens;
            Duration::from_secs_f64(needed / self.refill_rate)
        }
    }
}

/// User rate limit state
struct UserRateLimitState {
    tier: RateLimitTier,
    request_bucket: TokenBucket,
    token_bucket: TokenBucket,
    concurrent_requests: u32,
    last_request: Instant,
}

/// Rate limit check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Remaining requests
    pub remaining_requests: u32,
    /// Remaining tokens
    pub remaining_tokens: u32,
    /// Time until limit resets (seconds)
    pub reset_in_seconds: u64,
    /// Retry after (seconds, if rate limited)
    pub retry_after: Option<u64>,
    /// User's tier
    pub tier: RateLimitTier,
}

/// Tiered rate limiter
pub struct TieredRateLimiter {
    /// Tier configurations
    configs: HashMap<RateLimitTier, TierConfig>,
    /// User states
    users: Arc<RwLock<HashMap<String, UserRateLimitState>>>,
    /// Default tier for unknown users
    default_tier: RateLimitTier,
    /// User tier assignments
    user_tiers: Arc<RwLock<HashMap<String, RateLimitTier>>>,
}

impl TieredRateLimiter {
    /// Create a new tiered rate limiter
    pub fn new() -> Self {
        Self {
            configs: default_tier_configs(),
            users: Arc::new(RwLock::new(HashMap::new())),
            default_tier: RateLimitTier::Free,
            user_tiers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with custom tier configurations
    pub fn with_configs(configs: HashMap<RateLimitTier, TierConfig>) -> Self {
        Self {
            configs,
            users: Arc::new(RwLock::new(HashMap::new())),
            default_tier: RateLimitTier::Free,
            user_tiers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set user tier
    pub fn set_user_tier(&self, user_id: &str, tier: RateLimitTier) {
        self.user_tiers
            .write()
            .unwrap()
            .insert(user_id.to_string(), tier);

        // Update existing user state if present
        if let Some(state) = self.users.write().unwrap().get_mut(user_id) {
            if let Some(config) = self.configs.get(&tier) {
                state.tier = tier;
                state.request_bucket = TokenBucket::new(
                    config.requests_per_minute + config.burst_allowance,
                    config.requests_per_minute,
                );
                state.token_bucket =
                    TokenBucket::new(config.tokens_per_minute, config.tokens_per_minute);
            }
        }
    }

    /// Get user tier
    pub fn get_user_tier(&self, user_id: &str) -> RateLimitTier {
        self.user_tiers
            .read()
            .unwrap()
            .get(user_id)
            .copied()
            .unwrap_or(self.default_tier)
    }

    /// Check if a request is allowed
    pub fn check_request(&self, user_id: &str) -> RateLimitResult {
        let tier = self.get_user_tier(user_id);
        let config = self.configs.get(&tier).cloned().unwrap_or_default();

        let mut users = self.users.write().unwrap();
        let state = users
            .entry(user_id.to_string())
            .or_insert_with(|| UserRateLimitState {
                tier,
                request_bucket: TokenBucket::new(
                    config.requests_per_minute + config.burst_allowance,
                    config.requests_per_minute,
                ),
                token_bucket: TokenBucket::new(config.tokens_per_minute, config.tokens_per_minute),
                concurrent_requests: 0,
                last_request: Instant::now(),
            });

        // Check concurrent limit
        if state.concurrent_requests >= config.max_concurrent {
            return RateLimitResult {
                allowed: false,
                remaining_requests: 0,
                remaining_tokens: state.token_bucket.available() as u32,
                reset_in_seconds: 1,
                retry_after: Some(1),
                tier,
            };
        }

        // Check request bucket
        if !state.request_bucket.try_consume(1.0) {
            let retry_after = state.request_bucket.time_until_available(1.0);
            return RateLimitResult {
                allowed: false,
                remaining_requests: 0,
                remaining_tokens: state.token_bucket.available() as u32,
                reset_in_seconds: 60,
                retry_after: Some(retry_after.as_secs().max(1)),
                tier,
            };
        }

        state.concurrent_requests += 1;
        state.last_request = Instant::now();

        RateLimitResult {
            allowed: true,
            remaining_requests: state.request_bucket.available() as u32,
            remaining_tokens: state.token_bucket.available() as u32,
            reset_in_seconds: 60,
            retry_after: None,
            tier,
        }
    }

    /// Check if tokens are available (for LLM calls)
    pub fn check_tokens(&self, user_id: &str, tokens: u32) -> RateLimitResult {
        let tier = self.get_user_tier(user_id);

        let mut users = self.users.write().unwrap();
        if let Some(state) = users.get_mut(user_id) {
            if !state.token_bucket.try_consume(tokens as f64) {
                let retry_after = state.token_bucket.time_until_available(tokens as f64);
                return RateLimitResult {
                    allowed: false,
                    remaining_requests: state.request_bucket.available() as u32,
                    remaining_tokens: state.token_bucket.available() as u32,
                    reset_in_seconds: 60,
                    retry_after: Some(retry_after.as_secs().max(1)),
                    tier,
                };
            }

            RateLimitResult {
                allowed: true,
                remaining_requests: state.request_bucket.available() as u32,
                remaining_tokens: state.token_bucket.available() as u32,
                reset_in_seconds: 60,
                retry_after: None,
                tier,
            }
        } else {
            // User not found, create new state
            self.check_request(user_id)
        }
    }

    /// Release a request (call when request completes)
    pub fn release_request(&self, user_id: &str) {
        let mut users = self.users.write().unwrap();
        if let Some(state) = users.get_mut(user_id) {
            state.concurrent_requests = state.concurrent_requests.saturating_sub(1);
        }
    }

    /// Get usage statistics for a user
    pub fn get_user_stats(&self, user_id: &str) -> Option<UserRateLimitStats> {
        let users = self.users.read().unwrap();
        let state = users.get(user_id)?;
        let config = self.configs.get(&state.tier)?;

        Some(UserRateLimitStats {
            user_id: user_id.to_string(),
            tier: state.tier,
            requests_remaining: state.request_bucket.tokens as u32,
            requests_limit: config.requests_per_minute + config.burst_allowance,
            tokens_remaining: state.token_bucket.tokens as u32,
            tokens_limit: config.tokens_per_minute,
            concurrent_requests: state.concurrent_requests,
            max_concurrent: config.max_concurrent,
        })
    }

    /// Clean up stale user states
    pub fn cleanup_stale_users(&self, max_age: Duration) {
        let mut users = self.users.write().unwrap();
        let now = Instant::now();

        users.retain(|_, state| now.duration_since(state.last_request) < max_age);
    }
}

impl Default for TieredRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// User rate limit statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRateLimitStats {
    /// User ID
    pub user_id: String,
    /// Current tier
    pub tier: RateLimitTier,
    /// Remaining requests
    pub requests_remaining: u32,
    /// Request limit
    pub requests_limit: u32,
    /// Remaining tokens
    pub tokens_remaining: u32,
    /// Token limit
    pub tokens_limit: u32,
    /// Current concurrent requests
    pub concurrent_requests: u32,
    /// Maximum concurrent requests
    pub max_concurrent: u32,
}

/// Rate limit guard that automatically releases on drop
pub struct RateLimitGuard {
    limiter: Arc<TieredRateLimiter>,
    user_id: String,
}

impl RateLimitGuard {
    /// Create a new rate limit guard
    pub fn new(limiter: Arc<TieredRateLimiter>, user_id: String) -> Self {
        Self { limiter, user_id }
    }
}

impl Drop for RateLimitGuard {
    fn drop(&mut self) {
        self.limiter.release_request(&self.user_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tier_configs() {
        let configs = default_tier_configs();

        assert!(configs.contains_key(&RateLimitTier::Free));
        assert!(configs.contains_key(&RateLimitTier::Premium));

        let free = configs.get(&RateLimitTier::Free).unwrap();
        let premium = configs.get(&RateLimitTier::Premium).unwrap();

        assert!(premium.requests_per_minute > free.requests_per_minute);
    }

    #[test]
    fn test_rate_limiter_allows_requests() {
        let limiter = TieredRateLimiter::new();

        // First request should be allowed
        let result = limiter.check_request("user1");
        assert!(result.allowed);
    }

    #[test]
    fn test_rate_limiter_respects_limits() {
        let mut configs = HashMap::new();
        configs.insert(
            RateLimitTier::Free,
            TierConfig {
                requests_per_minute: 2,
                tokens_per_minute: 100,
                burst_allowance: 0,
                max_concurrent: 10,
                priority: 0,
                cost_multiplier: 1.0,
            },
        );

        let limiter = TieredRateLimiter::with_configs(configs);

        // First two requests should be allowed
        assert!(limiter.check_request("user1").allowed);
        limiter.release_request("user1");
        assert!(limiter.check_request("user1").allowed);
        limiter.release_request("user1");

        // Third request should be rate limited
        let result = limiter.check_request("user1");
        assert!(!result.allowed);
    }

    #[test]
    fn test_tier_assignment() {
        let limiter = TieredRateLimiter::new();

        // Default tier should be Free
        assert_eq!(limiter.get_user_tier("user1"), RateLimitTier::Free);

        // Set to Premium
        limiter.set_user_tier("user1", RateLimitTier::Premium);
        assert_eq!(limiter.get_user_tier("user1"), RateLimitTier::Premium);
    }

    #[test]
    fn test_concurrent_limit() {
        let mut configs = HashMap::new();
        configs.insert(
            RateLimitTier::Free,
            TierConfig {
                requests_per_minute: 100,
                tokens_per_minute: 1000,
                burst_allowance: 10,
                max_concurrent: 2,
                priority: 0,
                cost_multiplier: 1.0,
            },
        );

        let limiter = TieredRateLimiter::with_configs(configs);

        // First two concurrent requests allowed
        assert!(limiter.check_request("user1").allowed);
        assert!(limiter.check_request("user1").allowed);

        // Third concurrent request denied
        assert!(!limiter.check_request("user1").allowed);

        // Release one
        limiter.release_request("user1");

        // Now another can proceed
        assert!(limiter.check_request("user1").allowed);
    }
}
