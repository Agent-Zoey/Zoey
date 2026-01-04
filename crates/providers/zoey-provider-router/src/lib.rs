#![warn(missing_docs)]
#![warn(clippy::all)]

use zoey_core::{Result, ZoeyError};
use zoey_core::observability::{ProviderPricing};
use async_trait::async_trait;
use std::collections::HashMap;

/// Routing strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    /// Round-robin
    RoundRobin,
    /// Least loaded
    LeastLoaded,
    /// Capability-based
    Capability,
    /// Cost-based
    Cost,
}

/// Provider capability metadata
#[derive(Debug, Clone, Default)]
pub struct ProviderInfo {
    /// Name
    pub name: String,
    /// Capabilities
    pub capabilities: Vec<String>,
    /// Estimated cost per 1k tokens
    pub cost_per_1k: f32,
    /// Current load (arbitrary units)
    pub load: u32,
    /// Rate-limit remaining (optional)
    pub rate_limit_remaining: Option<u32>,
    /// Pricing details
    pub pricing: Option<ProviderPricing>,
}

/// Router for provider selection
pub struct ProviderRouter {
    strategy: RoutingStrategy,
    providers: Vec<ProviderInfo>,
    rr_index: usize,
}

impl ProviderRouter {
    /// Create router
    pub fn new(strategy: RoutingStrategy) -> Self {
        Self { strategy, providers: Vec::new(), rr_index: 0 }
    }

    /// Register provider
    pub fn register(&mut self, info: ProviderInfo) {
        self.providers.push(info);
    }

    /// Route by strategy
    pub fn route(&mut self, capability: Option<&str>) -> Result<&ProviderInfo> {
        if self.providers.is_empty() {
            return Err(ZoeyError::other("No providers registered"));
        }
        match self.strategy {
            RoutingStrategy::RoundRobin => {
                let p = &self.providers[self.rr_index % self.providers.len()];
                self.rr_index = (self.rr_index + 1) % self.providers.len();
                Ok(p)
            }
            RoutingStrategy::LeastLoaded => Ok(self.providers.iter().min_by_key(|p| p.load).unwrap()),
            RoutingStrategy::Capability => {
                let cap = capability.unwrap_or("");
                // prefer highest rate-limit remaining among capable providers
                let mut candidates: Vec<&ProviderInfo> = self.providers
                    .iter()
                    .filter(|p| p.capabilities.iter().any(|c| c == cap))
                    .collect();
                if candidates.is_empty() {
                    candidates = vec![&self.providers[0]];
                }
                candidates.sort_by_key(|p| (
                    p.rate_limit_remaining.unwrap_or(u32::MAX),
                    (p.cost_per_1k * 1000.0) as u32,
                    p.load,
                ));
                Ok(candidates[0])
            }
            RoutingStrategy::Cost => Ok(self.providers.iter().min_by(|a, b| a.cost_per_1k.partial_cmp(&b.cost_per_1k).unwrap()).unwrap()),
        }
    }
}

/// Integrate with core runtime model registry
pub fn route_model_for(runtime: &zoey_core::runtime::AgentRuntime, capability: Option<&str>) -> Option<String> {
    let mut router = ProviderRouter::new(RoutingStrategy::Capability);
    // Read provider capability info from runtime models map (populated on registration)
    for p in runtime.get_providers() {
        let caps = p.capabilities().unwrap_or_else(|| vec!["CHAT".to_string()]);
        let rl_remaining = runtime.observability.read().ok().and_then(|obs| obs.as_ref().and_then(|o| o.get_rate_limit_remaining(&p.name())));
        let load = zoey_core::runtime::RuntimeState::new().estimate_load(runtime);
        router.register(ProviderInfo { name: p.name().to_string(), capabilities: caps, cost_per_1k: 0.0, load, rate_limit_remaining: rl_remaining, pricing: None });
    }
    router.route(capability).ok().map(|pi| pi.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_robin() {
        let mut r = ProviderRouter::new(RoutingStrategy::RoundRobin);
        r.register(ProviderInfo { name: "openai".into(), ..Default::default() });
        r.register(ProviderInfo { name: "anthropic".into(), ..Default::default() });
        let p1_name = r.route(None).unwrap().name.clone();
        let p2_name = r.route(None).unwrap().name.clone();
        assert_ne!(p1_name, p2_name);
    }

    #[test]
    fn test_capability() {
        let mut r = ProviderRouter::new(RoutingStrategy::Capability);
        r.register(ProviderInfo { name: "openai".into(), capabilities: vec!["function_calling".into()], cost_per_1k: 0.002, load: 0, rate_limit_remaining: Some(1000), pricing: None });
        r.register(ProviderInfo { name: "anthropic".into(), capabilities: vec!["vision".into()], cost_per_1k: 0.003, load: 0, rate_limit_remaining: Some(500), pricing: None });
        let p = r.route(Some("vision")).unwrap();
        assert_eq!(p.name, "anthropic");
    }
    
    #[test]
    fn test_sorting_prefers_rate_limit_then_cost_then_load() {
        let mut r = ProviderRouter::new(RoutingStrategy::Capability);
        r.register(ProviderInfo { name: "cheap".into(), capabilities: vec!["CHAT".into()], cost_per_1k: 0.001, load: 5, rate_limit_remaining: Some(100), pricing: None });
        r.register(ProviderInfo { name: "pricy".into(), capabilities: vec!["CHAT".into()], cost_per_1k: 0.01, load: 1, rate_limit_remaining: Some(100), pricing: None });
        r.register(ProviderInfo { name: "ratelimit".into(), capabilities: vec!["CHAT".into()], cost_per_1k: 0.02, load: 10, rate_limit_remaining: Some(500), pricing: None });
        let p = r.route(Some("CHAT")).unwrap();
        assert_eq!(p.name, "ratelimit");
    }
}
