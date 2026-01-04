use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid as UUID;

/// LLM cost record for a single call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMCostRecord {
    // Identity
    pub id: UUID,
    pub timestamp: DateTime<Utc>,

    // Attribution (WHO caused this cost?)
    pub agent_id: UUID,
    pub user_id: Option<String>,
    pub conversation_id: Option<UUID>,
    pub action_name: Option<String>,
    pub evaluator_name: Option<String>,

    // LLM Details (WHAT was called?)
    pub provider: String,
    pub model: String,
    pub temperature: f32,

    // Usage (HOW MUCH was used?)
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub cached_tokens: Option<usize>,

    // Cost (HOW MUCH did it cost?)
    pub input_cost_usd: f64,
    pub output_cost_usd: f64,
    pub total_cost_usd: f64,

    // Performance
    pub latency_ms: u64,
    pub ttft_ms: Option<u64>, // Time to first token

    // Outcome
    pub success: bool,
    pub error: Option<String>,

    // Compliance
    pub prompt_hash: Option<String>,
    pub prompt_preview: Option<String>,
}

/// Context for LLM call (passed to cost tracker)
#[derive(Debug, Clone, Default)]
pub struct LLMCallContext {
    pub agent_id: UUID,
    pub user_id: Option<String>,
    pub conversation_id: Option<UUID>,
    pub action_name: Option<String>,
    pub evaluator_name: Option<String>,
    pub temperature: Option<f32>,
    pub cached_tokens: Option<usize>,
    pub ttft_ms: Option<u64>,
    pub prompt_hash: Option<String>,
    pub prompt_preview: Option<String>,
}

/// Provider pricing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPricing {
    pub provider: String,
    pub model: String,
    pub input_cost_per_1k_tokens: f64,
    pub output_cost_per_1k_tokens: f64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderRateLimit {
    pub remaining: Option<u32>,
    pub reset_epoch_s: Option<u64>,
}

/// Cost breakdown row (for queries)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdownRow {
    pub group_key: String,
    pub total_calls: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub avg_latency_ms: f64,
}

/// Cost summary response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub total_cost_usd: f64,
    pub total_calls: u64,
    pub total_tokens: u64,
    pub avg_latency_ms: f64,
    pub breakdown_by_model: Vec<CostBreakdownRow>,
    pub breakdown_by_provider: Vec<CostBreakdownRow>,
}
