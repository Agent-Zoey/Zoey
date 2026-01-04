pub mod config;
pub mod cost_tracker;
pub mod rest;
pub mod security_monitor;
pub mod types;

pub use config::*;
pub use cost_tracker::{get_global_cost_tracker, set_global_cost_tracker, CostTracker};
pub use rest::start_rest_api;
pub use security_monitor::{AlertChannel, AlertSeverity, SecurityAlert, SecurityMonitor};
pub use types::*;

use crate::error::ZoeyError;
use crate::types::IDatabaseAdapter;
use crate::AgentRuntime;
use crate::Plugin;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Main observability service
pub struct Observability {
    pub config: ObservabilityConfig,
    pub cost_tracker: Option<Arc<CostTracker>>,
    pub security_monitor: Option<Arc<SecurityMonitor>>,
    pub rate_limits: Arc<RwLock<HashMap<String, ProviderRateLimit>>>,
}

impl Observability {
    pub fn new(
        config: ObservabilityConfig,
        db: Option<Arc<dyn IDatabaseAdapter + Send + Sync>>,
    ) -> Self {
        // Always initialize a cost tracker; persistence depends on storage availability.
        let cost_tracker = Some(Arc::new(CostTracker::new(db.clone())));

        let security_monitor = if config.enabled {
            Some(Arc::new(SecurityMonitor::new(config.clone())))
        } else {
            None
        };

        Self {
            config,
            cost_tracker,
            security_monitor,
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize observability (called on startup)
    pub async fn initialize(&self) -> Result<(), ZoeyError> {
        // Start REST API server if enabled
        if self.config.rest_api.enabled {
            let cost_tracker = self.cost_tracker.clone();
            let rest_config = self.config.rest_api.clone();

            tokio::spawn(async move {
                if let Err(e) = start_rest_api(rest_config, cost_tracker).await {
                    tracing::error!("Failed to start REST API: {}", e);
                }
            });

            tracing::info!(
                "Observability REST API started on {}:{}",
                self.config.rest_api.host,
                self.config.rest_api.port
            );
        }

        Ok(())
    }
}

use sha2::{Digest, Sha256};

pub fn compute_prompt_preview(s: &str) -> String {
    s.chars().take(200).collect()
}

pub fn compute_prompt_hash(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let hash = hasher.finalize();
    hex::encode(hash)
}

/// Observability plugin wrapper to integrate with the plugin system
pub struct ObservabilityPlugin;

impl ObservabilityPlugin {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Plugin for ObservabilityPlugin {
    fn name(&self) -> &str {
        "observability"
    }
    fn description(&self) -> &str {
        "Observability service: cost tracking, security monitor, optional REST API"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        runtime_any: Arc<dyn Any + Send + Sync>,
    ) -> crate::Result<()> {
        let cfg = ObservabilityConfig::from_env();

        // Downcast the erased runtime to Arc<RwLock<AgentRuntime>> and fetch adapter without holding lock across await
        if let Some(rt_arc) = runtime_any.downcast_ref::<Arc<RwLock<AgentRuntime>>>() {
            let db_opt = {
                let rt = rt_arc.read().unwrap();
                rt.get_adapter()
            };
            let obs = Observability::new(cfg, db_opt);
            obs.initialize()
                .await
                .map_err(|e| crate::ZoeyError::other(e.to_string()))?;
        }

        Ok(())
    }
}

impl Observability {
    pub fn set_rate_limit(&self, provider: &str, rl: ProviderRateLimit) {
        if let Ok(mut m) = self.rate_limits.write() {
            m.insert(provider.to_string(), rl);
        }
    }

    pub fn get_rate_limit_remaining(&self, provider: &str) -> Option<u32> {
        self.rate_limits
            .read()
            .ok()
            .and_then(|m| m.get(provider).and_then(|rl| rl.remaining))
    }
}
