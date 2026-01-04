//! Cost calculation and model pricing

use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Model pricing information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPricing {
    /// Model name/identifier
    pub model_name: String,
    /// Input cost per 1M tokens (USD)
    pub input_cost_per_1m_tokens: f64,
    /// Output cost per 1M tokens (USD)
    pub output_cost_per_1m_tokens: f64,
    /// Context window size (tokens)
    pub context_window: usize,
    /// Max output tokens
    pub max_output_tokens: usize,
}

impl ModelPricing {
    /// Calculate cost for given token usage
    pub fn calculate_cost(&self, input_tokens: usize, output_tokens: usize) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_cost_per_1m_tokens;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_cost_per_1m_tokens;
        input_cost + output_cost
    }

    /// Check if token usage fits within limits
    pub fn fits_in_context(&self, total_tokens: usize) -> bool {
        total_tokens <= self.context_window
    }
}

/// Cost estimate for an operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimate {
    /// Input tokens
    pub input_tokens: usize,
    /// Output tokens
    pub output_tokens: usize,
    /// Total tokens
    pub total_tokens: usize,
    /// Estimated cost in USD
    pub estimated_cost_usd: f64,
    /// Model used
    pub model_used: String,
    /// Pricing information
    pub pricing: ModelPricing,
    /// Breakdown of costs
    pub breakdown: CostBreakdown,
}

/// Detailed cost breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostBreakdown {
    /// Input token cost
    pub input_cost: f64,
    /// Output token cost
    pub output_cost: f64,
    /// Total cost
    pub total_cost: f64,
    /// Cost per token
    pub cost_per_token: f64,
}

/// Cost calculator
pub struct CostCalculator {
    /// Model pricing database
    pricing_db: Arc<RwLock<HashMap<String, ModelPricing>>>,
}

impl CostCalculator {
    /// Create a new cost calculator with default pricing
    pub fn new() -> Self {
        let calculator = Self {
            pricing_db: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize with default pricing
        calculator.load_default_pricing();
        calculator
    }

    /// Load default model pricing
    fn load_default_pricing(&self) {
        let mut db = self.pricing_db.write().unwrap();

        // OpenAI GPT-4 models (as of 2024)
        db.insert(
            "gpt-4".to_string(),
            ModelPricing {
                model_name: "gpt-4".to_string(),
                input_cost_per_1m_tokens: 30.0,
                output_cost_per_1m_tokens: 60.0,
                context_window: 8192,
                max_output_tokens: 4096,
            },
        );

        db.insert(
            "gpt-4-turbo".to_string(),
            ModelPricing {
                model_name: "gpt-4-turbo".to_string(),
                input_cost_per_1m_tokens: 10.0,
                output_cost_per_1m_tokens: 30.0,
                context_window: 128000,
                max_output_tokens: 4096,
            },
        );

        db.insert(
            "gpt-4o".to_string(),
            ModelPricing {
                model_name: "gpt-4o".to_string(),
                input_cost_per_1m_tokens: 5.0,
                output_cost_per_1m_tokens: 15.0,
                context_window: 128000,
                max_output_tokens: 16384,
            },
        );

        // OpenAI GPT-3.5 models
        db.insert(
            "gpt-3.5-turbo".to_string(),
            ModelPricing {
                model_name: "gpt-3.5-turbo".to_string(),
                input_cost_per_1m_tokens: 0.5,
                output_cost_per_1m_tokens: 1.5,
                context_window: 16385,
                max_output_tokens: 4096,
            },
        );

        // Anthropic Claude models (as of 2024)
        db.insert(
            "claude-3-opus".to_string(),
            ModelPricing {
                model_name: "claude-3-opus".to_string(),
                input_cost_per_1m_tokens: 15.0,
                output_cost_per_1m_tokens: 75.0,
                context_window: 200000,
                max_output_tokens: 4096,
            },
        );

        db.insert(
            "claude-3-sonnet".to_string(),
            ModelPricing {
                model_name: "claude-3-sonnet".to_string(),
                input_cost_per_1m_tokens: 3.0,
                output_cost_per_1m_tokens: 15.0,
                context_window: 200000,
                max_output_tokens: 4096,
            },
        );

        db.insert(
            "claude-3-haiku".to_string(),
            ModelPricing {
                model_name: "claude-3-haiku".to_string(),
                input_cost_per_1m_tokens: 0.25,
                output_cost_per_1m_tokens: 1.25,
                context_window: 200000,
                max_output_tokens: 4096,
            },
        );

        db.insert(
            "claude-3.5-sonnet".to_string(),
            ModelPricing {
                model_name: "claude-3.5-sonnet".to_string(),
                input_cost_per_1m_tokens: 3.0,
                output_cost_per_1m_tokens: 15.0,
                context_window: 200000,
                max_output_tokens: 8192,
            },
        );

        // Local models (free)
        db.insert(
            "llama".to_string(),
            ModelPricing {
                model_name: "llama".to_string(),
                input_cost_per_1m_tokens: 0.0,
                output_cost_per_1m_tokens: 0.0,
                context_window: 4096,
                max_output_tokens: 2048,
            },
        );

        db.insert(
            "local".to_string(),
            ModelPricing {
                model_name: "local".to_string(),
                input_cost_per_1m_tokens: 0.0,
                output_cost_per_1m_tokens: 0.0,
                context_window: 8192,
                max_output_tokens: 4096,
            },
        );
    }

    /// Add or update model pricing
    pub fn set_pricing(&self, pricing: ModelPricing) {
        let mut db = self.pricing_db.write().unwrap();
        db.insert(pricing.model_name.clone(), pricing);
    }

    /// Get pricing for a model
    pub fn get_pricing(&self, model_name: &str) -> Option<ModelPricing> {
        self.pricing_db.read().unwrap().get(model_name).cloned()
    }

    /// Calculate cost for token usage
    pub fn calculate_cost(
        &self,
        model_name: &str,
        input_tokens: usize,
        output_tokens: usize,
    ) -> Result<CostEstimate> {
        let pricing = self
            .get_pricing(model_name)
            .ok_or_else(|| crate::ZoeyError::Other(format!("Unknown model: {}", model_name)))?;

        let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_cost_per_1m_tokens;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_cost_per_1m_tokens;
        let total_cost = input_cost + output_cost;
        let total_tokens = input_tokens + output_tokens;

        let cost_per_token = if total_tokens > 0 {
            total_cost / total_tokens as f64
        } else {
            0.0
        };

        Ok(CostEstimate {
            input_tokens,
            output_tokens,
            total_tokens,
            estimated_cost_usd: total_cost,
            model_used: model_name.to_string(),
            pricing,
            breakdown: CostBreakdown {
                input_cost,
                output_cost,
                total_cost,
                cost_per_token,
            },
        })
    }

    /// Find cheaper alternative model
    pub fn find_cheaper_model(&self, current_model: &str, min_context: usize) -> Option<String> {
        let db = self.pricing_db.read().unwrap();

        let current_pricing = db.get(current_model)?;
        let current_avg_cost = (current_pricing.input_cost_per_1m_tokens
            + current_pricing.output_cost_per_1m_tokens)
            / 2.0;

        // Find models with lower average cost and sufficient context
        let mut cheaper_models: Vec<_> = db
            .values()
            .filter(|p| {
                let avg_cost = (p.input_cost_per_1m_tokens + p.output_cost_per_1m_tokens) / 2.0;
                avg_cost < current_avg_cost && p.context_window >= min_context
            })
            .collect();

        // Sort by cost (cheapest first)
        cheaper_models.sort_by(|a, b| {
            let avg_a = (a.input_cost_per_1m_tokens + a.output_cost_per_1m_tokens) / 2.0;
            let avg_b = (b.input_cost_per_1m_tokens + b.output_cost_per_1m_tokens) / 2.0;
            avg_a.partial_cmp(&avg_b).unwrap()
        });

        cheaper_models.first().map(|p| p.model_name.clone())
    }

    /// Get all available models sorted by cost
    pub fn get_models_by_cost(&self) -> Vec<ModelPricing> {
        let db = self.pricing_db.read().unwrap();
        let mut models: Vec<_> = db.values().cloned().collect();

        models.sort_by(|a, b| {
            let avg_a = (a.input_cost_per_1m_tokens + a.output_cost_per_1m_tokens) / 2.0;
            let avg_b = (b.input_cost_per_1m_tokens + b.output_cost_per_1m_tokens) / 2.0;
            avg_a.partial_cmp(&avg_b).unwrap()
        });

        models
    }

    /// Recommend model based on budget and requirements
    pub fn recommend_model(
        &self,
        budget_usd: f64,
        estimated_tokens: usize,
        min_context: usize,
    ) -> Option<String> {
        let db = self.pricing_db.read().unwrap();

        // Assume 50/50 split between input and output
        let estimated_input = estimated_tokens / 2;
        let estimated_output = estimated_tokens / 2;

        // Find models that fit budget and context requirements
        let mut suitable: Vec<_> = db
            .values()
            .filter(|p| {
                let cost = p.calculate_cost(estimated_input, estimated_output);
                cost <= budget_usd && p.context_window >= min_context
            })
            .collect();

        if suitable.is_empty() {
            return None;
        }

        // Sort by capability (higher cost usually = better model)
        suitable.sort_by(|a, b| {
            let avg_a = (a.input_cost_per_1m_tokens + a.output_cost_per_1m_tokens) / 2.0;
            let avg_b = (b.input_cost_per_1m_tokens + b.output_cost_per_1m_tokens) / 2.0;
            avg_b.partial_cmp(&avg_a).unwrap() // Descending (best first)
        });

        suitable.first().map(|p| p.model_name.clone())
    }
}

impl Default for CostCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_calculation() {
        let calculator = CostCalculator::new();

        // Test GPT-4 cost
        let estimate = calculator.calculate_cost("gpt-4", 1000, 500).unwrap();

        assert_eq!(estimate.input_tokens, 1000);
        assert_eq!(estimate.output_tokens, 500);
        assert!(estimate.estimated_cost_usd > 0.0);

        // Input: 1000 tokens * $30 / 1M = $0.03
        // Output: 500 tokens * $60 / 1M = $0.03
        // Total: $0.06
        assert!((estimate.estimated_cost_usd - 0.06).abs() < 0.001);
    }

    #[test]
    fn test_cheaper_model() {
        let calculator = CostCalculator::new();

        // Find cheaper than GPT-4
        let cheaper = calculator.find_cheaper_model("gpt-4", 8000);
        assert!(cheaper.is_some());

        let cheaper_model = cheaper.unwrap();
        assert_ne!(cheaper_model, "gpt-4");

        // Verify it's actually cheaper
        let gpt4_pricing = calculator.get_pricing("gpt-4").unwrap();
        let cheaper_pricing = calculator.get_pricing(&cheaper_model).unwrap();

        let gpt4_avg =
            (gpt4_pricing.input_cost_per_1m_tokens + gpt4_pricing.output_cost_per_1m_tokens) / 2.0;
        let cheaper_avg = (cheaper_pricing.input_cost_per_1m_tokens
            + cheaper_pricing.output_cost_per_1m_tokens)
            / 2.0;

        assert!(cheaper_avg < gpt4_avg);
    }

    #[test]
    fn test_model_recommendation() {
        let calculator = CostCalculator::new();

        // Small budget should recommend cheaper model
        let model = calculator.recommend_model(0.001, 1000, 4000);
        assert!(model.is_some());

        let model_name = model.unwrap();
        let pricing = calculator.get_pricing(&model_name).unwrap();

        // Should be a relatively cheap model
        assert!(pricing.input_cost_per_1m_tokens < 5.0);
    }

    #[test]
    fn test_local_models_free() {
        let calculator = CostCalculator::new();

        let estimate = calculator.calculate_cost("local", 10000, 5000).unwrap();

        assert_eq!(estimate.estimated_cost_usd, 0.0);
    }

    #[test]
    fn test_models_sorted_by_cost() {
        let calculator = CostCalculator::new();
        let models = calculator.get_models_by_cost();

        assert!(!models.is_empty());

        // Verify sorted (cheapest first)
        for i in 0..models.len() - 1 {
            let avg_current =
                (models[i].input_cost_per_1m_tokens + models[i].output_cost_per_1m_tokens) / 2.0;
            let avg_next = (models[i + 1].input_cost_per_1m_tokens
                + models[i + 1].output_cost_per_1m_tokens)
                / 2.0;

            assert!(avg_current <= avg_next);
        }
    }

    #[test]
    fn test_custom_pricing() {
        let calculator = CostCalculator::new();

        let custom = ModelPricing {
            model_name: "custom-model".to_string(),
            input_cost_per_1m_tokens: 1.0,
            output_cost_per_1m_tokens: 2.0,
            context_window: 4096,
            max_output_tokens: 2048,
        };

        calculator.set_pricing(custom.clone());

        let retrieved = calculator.get_pricing("custom-model").unwrap();
        assert_eq!(retrieved.model_name, "custom-model");
        assert_eq!(retrieved.input_cost_per_1m_tokens, 1.0);
    }
}
