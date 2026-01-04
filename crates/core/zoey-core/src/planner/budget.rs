//! Budget management for agent operations

use crate::planner::cost::CostEstimate;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// Action to take when budget is exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BudgetAction {
    /// Warn but continue
    Warn,
    /// Switch to smaller/cheaper model
    SwitchToSmaller,
    /// Block execution
    Block,
    /// Require user approval
    RequireApproval,
}

/// Budget configuration and tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBudget {
    /// Total budget in USD
    pub total_budget_usd: f64,
    /// Amount spent so far
    pub spent_usd: f64,
    /// Amount reserved for in-flight requests
    pub reserved_usd: f64,
    /// Available budget
    pub available_usd: f64,
    /// Per-request limit (optional)
    pub per_request_limit_usd: Option<f64>,
    /// Action to take when budget exceeded
    pub budget_exceeded_action: BudgetAction,
    /// Budget period in days (for auto-reset)
    pub period_days: Option<u32>,
    /// Period start timestamp
    pub period_start: i64,
}

impl AgentBudget {
    /// Create a new budget
    pub fn new(total_budget_usd: f64, exceeded_action: BudgetAction) -> Self {
        Self {
            total_budget_usd,
            spent_usd: 0.0,
            reserved_usd: 0.0,
            available_usd: total_budget_usd,
            per_request_limit_usd: None,
            budget_exceeded_action: exceeded_action,
            period_days: None,
            period_start: chrono::Utc::now().timestamp(),
        }
    }

    /// Create with per-request limit
    pub fn with_request_limit(
        total_budget_usd: f64,
        per_request_limit_usd: f64,
        exceeded_action: BudgetAction,
    ) -> Self {
        Self {
            total_budget_usd,
            spent_usd: 0.0,
            reserved_usd: 0.0,
            available_usd: total_budget_usd,
            per_request_limit_usd: Some(per_request_limit_usd),
            budget_exceeded_action: exceeded_action,
            period_days: None,
            period_start: chrono::Utc::now().timestamp(),
        }
    }

    /// Create with periodic reset
    pub fn with_period(
        total_budget_usd: f64,
        period_days: u32,
        exceeded_action: BudgetAction,
    ) -> Self {
        Self {
            total_budget_usd,
            spent_usd: 0.0,
            reserved_usd: 0.0,
            available_usd: total_budget_usd,
            per_request_limit_usd: None,
            budget_exceeded_action: exceeded_action,
            period_days: Some(period_days),
            period_start: chrono::Utc::now().timestamp(),
        }
    }

    /// Check if budget allows a cost
    pub fn can_afford(&self, cost_usd: f64) -> bool {
        // Check total budget
        if cost_usd > self.available_usd {
            return false;
        }

        // Check per-request limit
        if let Some(limit) = self.per_request_limit_usd {
            if cost_usd > limit {
                return false;
            }
        }

        true
    }

    /// Reserve budget for a request
    pub fn reserve(&mut self, cost_usd: f64) -> bool {
        if self.can_afford(cost_usd) {
            self.reserved_usd += cost_usd;
            self.available_usd -= cost_usd;
            true
        } else {
            false
        }
    }

    /// Commit a reserved amount (move to spent)
    pub fn commit(&mut self, cost_usd: f64) {
        self.reserved_usd -= cost_usd;
        self.spent_usd += cost_usd;
    }

    /// Release a reservation (return to available)
    pub fn release(&mut self, cost_usd: f64) {
        self.reserved_usd -= cost_usd;
        self.available_usd += cost_usd;
    }

    /// Get remaining budget
    pub fn remaining(&self) -> f64 {
        self.available_usd
    }

    /// Get utilization percentage (0.0 - 1.0+)
    pub fn utilization(&self) -> f64 {
        if self.total_budget_usd == 0.0 {
            0.0
        } else {
            (self.spent_usd + self.reserved_usd) / self.total_budget_usd
        }
    }

    /// Check if period has expired and reset if needed
    pub fn check_and_reset_period(&mut self) -> bool {
        if let Some(period_days) = self.period_days {
            let now = chrono::Utc::now().timestamp();
            let period_seconds = period_days as i64 * 86400;

            if now - self.period_start >= period_seconds {
                self.reset();
                self.period_start = now;
                return true;
            }
        }
        false
    }

    /// Reset budget (for new period)
    pub fn reset(&mut self) {
        self.spent_usd = 0.0;
        self.reserved_usd = 0.0;
        self.available_usd = self.total_budget_usd;
    }

    /// Increase total budget
    pub fn add_budget(&mut self, amount_usd: f64) {
        self.total_budget_usd += amount_usd;
        self.available_usd += amount_usd;
    }
}

/// Result of a budget check
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetCheckResult {
    /// Whether the request is approved
    pub approved: bool,
    /// Reason for decision
    pub reason: String,
    /// Action to take
    pub action: BudgetAction,
    /// Available budget
    pub available_budget: f64,
    /// Required budget
    pub required_budget: f64,
    /// Budget utilization (0.0 - 1.0+)
    pub utilization: f64,
}

/// Budget manager
pub struct BudgetManager {
    /// Current budget
    budget: Arc<RwLock<AgentBudget>>,
}

impl BudgetManager {
    /// Create a new budget manager
    pub fn new(budget: AgentBudget) -> Self {
        Self {
            budget: Arc::new(RwLock::new(budget)),
        }
    }

    /// Check if cost estimate fits within budget
    pub fn check_budget(&self, estimate: &CostEstimate) -> Result<BudgetCheckResult> {
        let mut budget = self.budget.write().unwrap();

        // Check and reset period if needed
        budget.check_and_reset_period();

        let cost = estimate.estimated_cost_usd;
        let can_afford = budget.can_afford(cost);

        let result = if can_afford {
            BudgetCheckResult {
                approved: true,
                reason: format!("Within budget (${:.4} available)", budget.available_usd),
                action: budget.budget_exceeded_action,
                available_budget: budget.available_usd,
                required_budget: cost,
                utilization: budget.utilization(),
            }
        } else {
            let reason = if let Some(limit) = budget.per_request_limit_usd {
                if cost > limit {
                    format!("Exceeds per-request limit (${:.4} > ${:.4})", cost, limit)
                } else {
                    format!(
                        "Insufficient budget (${:.4} available, ${:.4} required)",
                        budget.available_usd, cost
                    )
                }
            } else {
                format!(
                    "Insufficient budget (${:.4} available, ${:.4} required)",
                    budget.available_usd, cost
                )
            };

            BudgetCheckResult {
                approved: false,
                reason,
                action: budget.budget_exceeded_action,
                available_budget: budget.available_usd,
                required_budget: cost,
                utilization: budget.utilization(),
            }
        };

        Ok(result)
    }

    /// Reserve budget for an operation
    pub fn reserve(&self, cost_usd: f64) -> Result<bool> {
        let mut budget = self.budget.write().unwrap();
        Ok(budget.reserve(cost_usd))
    }

    /// Commit a reservation
    pub fn commit(&self, cost_usd: f64) -> Result<()> {
        let mut budget = self.budget.write().unwrap();
        budget.commit(cost_usd);
        Ok(())
    }

    /// Release a reservation
    pub fn release(&self, cost_usd: f64) -> Result<()> {
        let mut budget = self.budget.write().unwrap();
        budget.release(cost_usd);
        Ok(())
    }

    /// Get current budget state
    pub fn get_budget(&self) -> AgentBudget {
        self.budget.read().unwrap().clone()
    }

    /// Get remaining budget
    pub fn get_remaining(&self) -> f64 {
        self.budget.read().unwrap().remaining()
    }

    /// Get utilization percentage
    pub fn get_utilization(&self) -> f64 {
        self.budget.read().unwrap().utilization()
    }

    /// Add budget
    pub fn add_budget(&self, amount_usd: f64) -> Result<()> {
        let mut budget = self.budget.write().unwrap();
        budget.add_budget(amount_usd);
        Ok(())
    }

    /// Reset budget
    pub fn reset(&self) -> Result<()> {
        let mut budget = self.budget.write().unwrap();
        budget.reset();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::cost::ModelPricing;

    fn create_test_estimate(cost: f64) -> CostEstimate {
        CostEstimate {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            estimated_cost_usd: cost,
            model_used: "test-model".to_string(),
            pricing: ModelPricing {
                model_name: "test-model".to_string(),
                input_cost_per_1m_tokens: 1.0,
                output_cost_per_1m_tokens: 2.0,
                context_window: 8192,
                max_output_tokens: 4096,
            },
            breakdown: crate::planner::cost::CostBreakdown {
                input_cost: cost * 0.4,
                output_cost: cost * 0.6,
                total_cost: cost,
                cost_per_token: cost / 1500.0,
            },
        }
    }

    #[test]
    fn test_budget_creation() {
        let budget = AgentBudget::new(10.0, BudgetAction::Warn);
        assert_eq!(budget.total_budget_usd, 10.0);
        assert_eq!(budget.available_usd, 10.0);
        assert_eq!(budget.spent_usd, 0.0);
    }

    #[test]
    fn test_budget_reservation() {
        let mut budget = AgentBudget::new(10.0, BudgetAction::Warn);

        assert!(budget.reserve(5.0));
        assert_eq!(budget.reserved_usd, 5.0);
        assert_eq!(budget.available_usd, 5.0);

        // Can't reserve more than available
        assert!(!budget.reserve(6.0));
    }

    #[test]
    fn test_budget_commit() {
        let mut budget = AgentBudget::new(10.0, BudgetAction::Warn);

        budget.reserve(5.0);
        budget.commit(5.0);

        assert_eq!(budget.spent_usd, 5.0);
        assert_eq!(budget.reserved_usd, 0.0);
        assert_eq!(budget.available_usd, 5.0);
    }

    #[test]
    fn test_budget_release() {
        let mut budget = AgentBudget::new(10.0, BudgetAction::Warn);

        budget.reserve(5.0);
        budget.release(5.0);

        assert_eq!(budget.reserved_usd, 0.0);
        assert_eq!(budget.available_usd, 10.0);
    }

    #[test]
    fn test_per_request_limit() {
        let budget = AgentBudget::with_request_limit(10.0, 2.0, BudgetAction::Block);

        // Within limit
        assert!(budget.can_afford(1.5));

        // Exceeds per-request limit
        assert!(!budget.can_afford(3.0));
    }

    #[test]
    fn test_budget_manager() {
        let budget = AgentBudget::new(10.0, BudgetAction::Warn);
        let manager = BudgetManager::new(budget);

        let estimate = create_test_estimate(5.0);
        let result = manager.check_budget(&estimate).unwrap();

        assert!(result.approved);
        assert_eq!(result.available_budget, 10.0);
    }

    #[test]
    fn test_budget_exceeded() {
        let budget = AgentBudget::new(10.0, BudgetAction::Block);
        let manager = BudgetManager::new(budget);

        let estimate = create_test_estimate(15.0);
        let result = manager.check_budget(&estimate).unwrap();

        assert!(!result.approved);
        assert_eq!(result.action, BudgetAction::Block);
    }

    #[test]
    fn test_utilization() {
        let mut budget = AgentBudget::new(10.0, BudgetAction::Warn);

        budget.reserve(5.0);
        assert_eq!(budget.utilization(), 0.5);

        budget.commit(5.0);
        assert_eq!(budget.utilization(), 0.5);

        budget.reserve(3.0);
        assert_eq!(budget.utilization(), 0.8);
    }

    #[test]
    fn test_budget_reset() {
        let mut budget = AgentBudget::new(10.0, BudgetAction::Warn);

        budget.reserve(5.0);
        budget.commit(5.0);

        budget.reset();

        assert_eq!(budget.spent_usd, 0.0);
        assert_eq!(budget.reserved_usd, 0.0);
        assert_eq!(budget.available_usd, 10.0);
    }

    #[test]
    fn test_add_budget() {
        let mut budget = AgentBudget::new(10.0, BudgetAction::Warn);

        budget.add_budget(5.0);

        assert_eq!(budget.total_budget_usd, 15.0);
        assert_eq!(budget.available_usd, 15.0);
    }
}
