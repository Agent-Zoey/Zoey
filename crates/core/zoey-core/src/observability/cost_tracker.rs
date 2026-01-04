use super::types::*;
use crate::error::ZoeyError;
use crate::types::IDatabaseAdapter;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;
use uuid::Uuid as UUID;

/// Global cost tracker instance for use by handlers
static GLOBAL_COST_TRACKER: OnceLock<Arc<CostTracker>> = OnceLock::new();

/// Set the global cost tracker (call once at startup)
pub fn set_global_cost_tracker(tracker: Arc<CostTracker>) {
    let _ = GLOBAL_COST_TRACKER.set(tracker);
}

/// Get the global cost tracker
pub fn get_global_cost_tracker() -> Option<Arc<CostTracker>> {
    GLOBAL_COST_TRACKER.get().cloned()
}

pub struct CostTracker {
    pricing: Arc<RwLock<HashMap<String, ProviderPricing>>>,
    hourly_costs: Arc<RwLock<HashMap<String, f64>>>,
    daily_costs: Arc<RwLock<HashMap<String, f64>>>,
    total_calls: Arc<RwLock<usize>>,
    total_tokens: Arc<RwLock<usize>>,
    total_latency_ms: Arc<RwLock<u64>>,
    model_breakdown: Arc<RwLock<HashMap<String, (usize, usize, f64, u64)>>>, // (calls, tokens, cost, latency_ms)
    provider_breakdown: Arc<RwLock<HashMap<String, (usize, usize, f64, u64)>>>, // (calls, tokens, cost, latency_ms)
    db: Option<Arc<dyn IDatabaseAdapter + Send + Sync>>,
}

impl CostTracker {
    pub fn new(db: Option<Arc<dyn IDatabaseAdapter + Send + Sync>>) -> Self {
        let pricing_map = Self::get_default_pricing();

        Self {
            pricing: Arc::new(RwLock::new(pricing_map)),
            hourly_costs: Arc::new(RwLock::new(HashMap::new())),
            daily_costs: Arc::new(RwLock::new(HashMap::new())),
            total_calls: Arc::new(RwLock::new(0)),
            total_tokens: Arc::new(RwLock::new(0)),
            total_latency_ms: Arc::new(RwLock::new(0)),
            model_breakdown: Arc::new(RwLock::new(HashMap::new())),
            provider_breakdown: Arc::new(RwLock::new(HashMap::new())),
            db,
        }
    }

    fn get_default_pricing() -> HashMap<String, ProviderPricing> {
        let mut pricing_map = HashMap::new();

        // OpenAI pricing (as of Jan 2025)
        pricing_map.insert(
            "openai:gpt-4".to_string(),
            ProviderPricing {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                input_cost_per_1k_tokens: 0.03,
                output_cost_per_1k_tokens: 0.06,
                updated_at: Utc::now(),
            },
        );

        pricing_map.insert(
            "openai:gpt-3.5-turbo".to_string(),
            ProviderPricing {
                provider: "openai".to_string(),
                model: "gpt-3.5-turbo".to_string(),
                input_cost_per_1k_tokens: 0.0015,
                output_cost_per_1k_tokens: 0.002,
                updated_at: Utc::now(),
            },
        );

        // Anthropic pricing
        pricing_map.insert(
            "anthropic:claude-3-opus-20240229".to_string(),
            ProviderPricing {
                provider: "anthropic".to_string(),
                model: "claude-3-opus-20240229".to_string(),
                input_cost_per_1k_tokens: 0.015,
                output_cost_per_1k_tokens: 0.075,
                updated_at: Utc::now(),
            },
        );

        pricing_map.insert(
            "anthropic:claude-3-sonnet-20240229".to_string(),
            ProviderPricing {
                provider: "anthropic".to_string(),
                model: "claude-3-sonnet-20240229".to_string(),
                input_cost_per_1k_tokens: 0.003,
                output_cost_per_1k_tokens: 0.015,
                updated_at: Utc::now(),
            },
        );

        // Ollama (local) - zero cost
        pricing_map.insert(
            "ollama:*".to_string(),
            ProviderPricing {
                provider: "ollama".to_string(),
                model: "*".to_string(),
                input_cost_per_1k_tokens: 0.0,
                output_cost_per_1k_tokens: 0.0,
                updated_at: Utc::now(),
            },
        );

        pricing_map
    }

    /// Record an LLM call
    pub async fn record_llm_call(
        &self,
        provider: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
        latency_ms: u64,
        agent_id: UUID,
        context: LLMCallContext,
    ) -> Result<LLMCostRecord, ZoeyError> {
        // Get pricing
        let pricing = self.get_pricing(provider, model).await?;

        // Calculate cost
        let input_cost = (prompt_tokens as f64 / 1000.0) * pricing.input_cost_per_1k_tokens;
        let output_cost = (completion_tokens as f64 / 1000.0) * pricing.output_cost_per_1k_tokens;
        let total_cost = input_cost + output_cost;

        // Create record
        let record = LLMCostRecord {
            id: UUID::new_v4(),
            timestamp: Utc::now(),
            agent_id,
            user_id: context.user_id,
            conversation_id: context.conversation_id,
            action_name: context.action_name,
            evaluator_name: context.evaluator_name,
            provider: provider.to_string(),
            model: model.to_string(),
            temperature: context.temperature.unwrap_or(0.7),
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            cached_tokens: context.cached_tokens,
            input_cost_usd: input_cost,
            output_cost_usd: output_cost,
            total_cost_usd: total_cost,
            latency_ms,
            ttft_ms: context.ttft_ms,
            success: true,
            error: None,
            prompt_hash: context.prompt_hash,
            prompt_preview: context.prompt_preview,
        };

        // Update in-memory aggregates
        self.update_aggregates(&record).await;

        // Store in database (async, non-blocking)
        if let Some(db) = &self.db {
            let db = Arc::clone(db);
            let record_clone = record.clone();
            tokio::spawn(async move {
                if let Err(e) = Self::persist_record(db, record_clone).await {
                    tracing::error!("Failed to persist cost record: {}", e);
                }
            });
        }

        Ok(record)
    }

    async fn get_pricing(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<ProviderPricing, ZoeyError> {
        let pricing = self.pricing.read().await;
        let key = format!("{}:{}", provider, model);

        // Try exact match
        if let Some(p) = pricing.get(&key) {
            return Ok(p.clone());
        }

        // Try wildcard for ollama
        if provider == "ollama" {
            if let Some(p) = pricing.get("ollama:*") {
                return Ok(p.clone());
            }
        }

        // Default to zero cost if not found (local models)
        Ok(ProviderPricing {
            provider: provider.to_string(),
            model: model.to_string(),
            input_cost_per_1k_tokens: 0.0,
            output_cost_per_1k_tokens: 0.0,
            updated_at: Utc::now(),
        })
    }

    async fn update_aggregates(&self, record: &LLMCostRecord) {
        let agent_key = record.agent_id.to_string();

        // Hourly
        let mut hourly = self.hourly_costs.write().await;
        *hourly.entry(agent_key.clone()).or_insert(0.0) += record.total_cost_usd;

        // Daily
        let mut daily = self.daily_costs.write().await;
        *daily.entry(agent_key).or_insert(0.0) += record.total_cost_usd;

        // Total calls
        let mut calls = self.total_calls.write().await;
        *calls += 1;

        // Total tokens
        let mut tokens = self.total_tokens.write().await;
        *tokens += record.total_tokens;

        // Total latency
        let mut latency = self.total_latency_ms.write().await;
        *latency += record.latency_ms;

        // Model breakdown (calls, tokens, cost, latency)
        let mut models = self.model_breakdown.write().await;
        let model_entry = models.entry(record.model.clone()).or_insert((0, 0, 0.0, 0));
        model_entry.0 += 1;
        model_entry.1 += record.total_tokens;
        model_entry.2 += record.total_cost_usd;
        model_entry.3 += record.latency_ms;

        // Provider breakdown (calls, tokens, cost, latency)
        let mut providers = self.provider_breakdown.write().await;
        let provider_entry = providers.entry(record.provider.clone()).or_insert((0, 0, 0.0, 0));
        provider_entry.0 += 1;
        provider_entry.1 += record.total_tokens;
        provider_entry.2 += record.total_cost_usd;
        provider_entry.3 += record.latency_ms;
    }

    async fn persist_record(
        db: Arc<dyn IDatabaseAdapter + Send + Sync>,
        record: LLMCostRecord,
    ) -> Result<(), ZoeyError> {
        db.persist_llm_cost(record).await
    }

    /// Get current hourly cost for an agent
    pub async fn get_hourly_cost(&self, agent_id: UUID) -> f64 {
        let hourly = self.hourly_costs.read().await;
        hourly.get(&agent_id.to_string()).copied().unwrap_or(0.0)
    }

    /// Get current daily cost for an agent
    pub async fn get_daily_cost(&self, agent_id: UUID) -> f64 {
        let daily = self.daily_costs.read().await;
        daily.get(&agent_id.to_string()).copied().unwrap_or(0.0)
    }

    /// Get cost summary
    pub async fn get_cost_summary(&self) -> CostSummary {
        let daily = self.daily_costs.read().await;
        let total_cost: f64 = daily.values().sum();
        
        let total_calls = *self.total_calls.read().await;
        let total_tokens = *self.total_tokens.read().await;
        let total_latency = *self.total_latency_ms.read().await;
        let avg_latency = if total_calls > 0 { total_latency as f64 / total_calls as f64 } else { 0.0 };
        
        let models = self.model_breakdown.read().await;
        let providers = self.provider_breakdown.read().await;
        
        let breakdown_by_model: Vec<CostBreakdownRow> = models.iter().map(|(model, (calls, tokens, cost, latency))| {
            CostBreakdownRow {
                group_key: model.clone(),
                total_calls: *calls as u64,
                total_tokens: *tokens as u64,
                total_cost_usd: *cost,
                avg_latency_ms: if *calls > 0 { *latency as f64 / *calls as f64 } else { 0.0 },
            }
        }).collect();
        
        let breakdown_by_provider: Vec<CostBreakdownRow> = providers.iter().map(|(provider, (calls, tokens, cost, latency))| {
            CostBreakdownRow {
                group_key: provider.clone(),
                total_calls: *calls as u64,
                total_tokens: *tokens as u64,
                total_cost_usd: *cost,
                avg_latency_ms: if *calls > 0 { *latency as f64 / *calls as f64 } else { 0.0 },
            }
        }).collect();

        CostSummary {
            total_cost_usd: total_cost,
            total_calls: total_calls as u64,
            total_tokens: total_tokens as u64,
            avg_latency_ms: avg_latency,
            breakdown_by_model,
            breakdown_by_provider,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cost_calculation_gpt4() {
        let tracker = CostTracker::new(None);

        let record = tracker
            .record_llm_call(
                "openai",
                "gpt-4",
                1000, // prompt tokens
                500,  // completion tokens
                1200, // latency ms
                UUID::new_v4(),
                LLMCallContext::default(),
            )
            .await
            .unwrap();

        // GPT-4 pricing: $0.03/1k input, $0.06/1k output
        assert_eq!(record.input_cost_usd, 0.03); // 1000 * 0.03/1000
        assert_eq!(record.output_cost_usd, 0.03); // 500 * 0.06/1000
        assert_eq!(record.total_cost_usd, 0.06);
    }

    #[tokio::test]
    async fn test_ollama_zero_cost() {
        let tracker = CostTracker::new(None);

        let record = tracker
            .record_llm_call(
                "ollama",
                "llama2",
                1000,
                500,
                800,
                UUID::new_v4(),
                LLMCallContext::default(),
            )
            .await
            .unwrap();

        assert_eq!(record.total_cost_usd, 0.0);
    }

    #[tokio::test]
    async fn test_hourly_cost_aggregation() {
        let tracker = CostTracker::new(None);
        let agent_id = UUID::new_v4();

        // Record two calls
        tracker
            .record_llm_call(
                "openai",
                "gpt-4",
                1000,
                500,
                1200,
                agent_id,
                LLMCallContext::default(),
            )
            .await
            .unwrap();

        tracker
            .record_llm_call(
                "openai",
                "gpt-4",
                2000,
                1000,
                1500,
                agent_id,
                LLMCallContext::default(),
            )
            .await
            .unwrap();

        let hourly_cost = tracker.get_hourly_cost(agent_id).await;
        assert_eq!(hourly_cost, 0.18); // 0.06 + 0.12
    }
}
