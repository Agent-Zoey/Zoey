//! Plan optimization strategies

use crate::planner::*;
use crate::Result;
use serde::{Deserialize, Serialize};

/// Optimization applied to a plan
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Optimization {
    /// Switched to a cheaper model
    ModelDowngrade,
    /// Reduced context window
    ReducedContext,
    /// Reduced max output tokens
    ReducedOutput,
    /// Simplified prompt
    SimplifiedPrompt,
    /// Cached response used
    CachedResponse,
    /// Batched with other requests
    BatchedRequest,
}

/// Plan optimizer
pub struct PlanOptimizer;

impl PlanOptimizer {
    /// Create a new plan optimizer
    pub fn new() -> Self {
        Self
    }

    /// Optimize a plan to fit budget constraints
    pub async fn optimize(
        &self,
        mut plan: ExecutionPlan,
        budget_check: &budget::BudgetCheckResult,
        cost_calculator: &cost::CostCalculator,
    ) -> Result<ExecutionPlan> {
        if budget_check.approved {
            // Already within budget, no optimization needed
            return Ok(plan);
        }

        let mut optimizations = Vec::new();

        // Try optimization strategies in order of preference
        match budget_check.action {
            budget::BudgetAction::Warn => {
                // Just warn, don't optimize
                plan.warnings.push(format!(
                    "Budget warning: {} (${:.4} available, ${:.4} required)",
                    budget_check.reason,
                    budget_check.available_budget,
                    budget_check.required_budget
                ));
            }

            budget::BudgetAction::SwitchToSmaller => {
                // Try switching to a cheaper model
                if let Some(cheaper_model) = cost_calculator.find_cheaper_model(
                    &plan.cost_estimate.model_used,
                    plan.token_estimate.input_tokens,
                ) {
                    // Recalculate cost with cheaper model
                    let new_cost = cost_calculator.calculate_cost(
                        &cheaper_model,
                        plan.cost_estimate.input_tokens,
                        plan.cost_estimate.output_tokens,
                    )?;

                    if new_cost.estimated_cost_usd <= budget_check.available_budget {
                        plan.cost_estimate = new_cost;
                        plan.response_strategy.model_selection = cheaper_model;
                        optimizations.push(Optimization::ModelDowngrade);
                    }
                }

                // If still over budget, try reducing output tokens
                if plan.cost_estimate.estimated_cost_usd > budget_check.available_budget {
                    let reduction_factor =
                        budget_check.available_budget / plan.cost_estimate.estimated_cost_usd;

                    let new_output_tokens =
                        (plan.cost_estimate.output_tokens as f64 * reduction_factor * 0.9) as usize;

                    if new_output_tokens > 50 {
                        // Recalculate
                        let new_cost = cost_calculator.calculate_cost(
                            &plan.cost_estimate.model_used,
                            plan.cost_estimate.input_tokens,
                            new_output_tokens,
                        )?;

                        plan.cost_estimate = new_cost;
                        plan.response_strategy.max_tokens = new_output_tokens;
                        optimizations.push(Optimization::ReducedOutput);
                    }
                }
            }

            budget::BudgetAction::Block => {
                // Cannot proceed, return error
                return Err(crate::ZoeyError::Other(format!(
                    "Budget exceeded and action is BLOCK: {}",
                    budget_check.reason
                )));
            }

            budget::BudgetAction::RequireApproval => {
                plan.warnings
                    .push(format!("User approval required: {}", budget_check.reason));
                plan.requires_approval = true;
            }
        }

        plan.optimizations_applied.extend(optimizations);

        Ok(plan)
    }

    /// Optimize token usage
    pub fn optimize_tokens(&self, plan: &mut ExecutionPlan) -> Vec<Optimization> {
        let optimizations = Vec::new();

        // If estimated tokens are very high, suggest reductions
        if plan.token_estimate.total_tokens > 100000 {
            plan.warnings.push(
                "High token usage detected. Consider reducing context or output length."
                    .to_string(),
            );
        }

        // Check if output tokens seem excessive for complexity
        let expected_output = match plan.complexity.level {
            complexity::ComplexityLevel::Trivial => 100,
            complexity::ComplexityLevel::Simple => 300,
            complexity::ComplexityLevel::Moderate => 600,
            complexity::ComplexityLevel::Complex => 1000,
            complexity::ComplexityLevel::VeryComplex => 2000,
        };

        if plan.token_estimate.output_tokens > expected_output * 2 {
            plan.warnings.push(format!(
                "Output tokens ({}) seem high for {} complexity. Expected ~{}.",
                plan.token_estimate.output_tokens, plan.complexity.level, expected_output
            ));
        }

        optimizations
    }

    /// Suggest optimizations based on historical data
    pub fn suggest_optimizations(&self, plan: &ExecutionPlan) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Model selection suggestions
        if plan.cost_estimate.estimated_cost_usd > 0.10 {
            suggestions.push(
                "Consider using a smaller model for cost savings (e.g., GPT-3.5 instead of GPT-4)"
                    .to_string(),
            );
        }

        // Token optimization suggestions
        if plan.token_estimate.input_tokens > 10000 {
            suggestions.push(
                "High input tokens detected. Consider summarizing context or using RAG."
                    .to_string(),
            );
        }

        // Complexity-based suggestions
        if matches!(
            plan.complexity.level,
            complexity::ComplexityLevel::Trivial | complexity::ComplexityLevel::Simple
        ) && plan.cost_estimate.model_used.contains("gpt-4")
        {
            suggestions.push(
                "Simple task detected. A smaller model like GPT-3.5 may be sufficient.".to_string(),
            );
        }

        // Knowledge gap suggestions
        if !plan.knowledge.unknown_gaps.is_empty() {
            let critical_gaps = plan
                .knowledge
                .unknown_gaps
                .iter()
                .filter(|g| g.priority == knowledge::Priority::Critical)
                .count();

            if critical_gaps > 0 {
                suggestions.push(format!(
                    "{} critical knowledge gaps detected. Consider gathering more context first.",
                    critical_gaps
                ));
            }
        }

        suggestions
    }
}

impl Default for PlanOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_model_downgrade() {
        // This would require creating a full ExecutionPlan
        // In practice, you'd test this with real data
        let optimizer = PlanOptimizer::new();
        assert!(true); // Placeholder
    }

    #[test]
    fn test_suggestions() {
        let optimizer = PlanOptimizer::new();
        // Would need a real plan to test suggestions
        assert!(true); // Placeholder
    }
}
