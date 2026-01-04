//! Planner metrics tracking

use crate::planner::complexity::ComplexityLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Metrics for planning operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerMetrics {
    /// Total plans created
    pub total_plans_created: usize,

    /// Total tokens used
    pub total_tokens_used: usize,

    /// Total cost in USD
    pub total_cost_usd: f64,

    /// Average accuracy of estimates (compared to actual)
    pub avg_estimation_accuracy: f32,

    /// Budget violations
    pub budget_violations: usize,

    /// Model switches performed
    pub model_switches: usize,

    /// Plans by complexity
    pub plans_by_complexity: HashMap<String, usize>,

    /// Average planning time (ms)
    pub avg_planning_time_ms: f64,

    /// Last updated timestamp
    pub last_updated: i64,
}

impl PlannerMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self {
            total_plans_created: 0,
            total_tokens_used: 0,
            total_cost_usd: 0.0,
            avg_estimation_accuracy: 0.0,
            budget_violations: 0,
            model_switches: 0,
            plans_by_complexity: HashMap::new(),
            avg_planning_time_ms: 0.0,
            last_updated: chrono::Utc::now().timestamp(),
        }
    }

    /// Record a new plan
    pub fn record_plan(
        &mut self,
        complexity: ComplexityLevel,
        tokens_used: usize,
        cost: f64,
        planning_time_ms: u128,
    ) {
        self.total_plans_created += 1;
        self.total_tokens_used += tokens_used;
        self.total_cost_usd += cost;

        // Update complexity counts
        *self
            .plans_by_complexity
            .entry(complexity.to_string())
            .or_insert(0) += 1;

        // Update average planning time
        let total_time = self.avg_planning_time_ms * (self.total_plans_created - 1) as f64;
        self.avg_planning_time_ms =
            (total_time + planning_time_ms as f64) / self.total_plans_created as f64;

        self.last_updated = chrono::Utc::now().timestamp();
    }

    /// Record estimation accuracy
    pub fn record_accuracy(&mut self, estimated: usize, actual: usize) {
        let accuracy = if actual > 0 {
            1.0 - ((estimated as f32 - actual as f32).abs() / actual as f32)
        } else {
            1.0
        };

        // Update running average
        let total = self.avg_estimation_accuracy * (self.total_plans_created - 1) as f32;
        self.avg_estimation_accuracy = (total + accuracy) / self.total_plans_created as f32;
    }

    /// Record budget violation
    pub fn record_budget_violation(&mut self) {
        self.budget_violations += 1;
    }

    /// Record model switch
    pub fn record_model_switch(&mut self) {
        self.model_switches += 1;
    }

    /// Get average cost per plan
    pub fn avg_cost_per_plan(&self) -> f64 {
        if self.total_plans_created == 0 {
            0.0
        } else {
            self.total_cost_usd / self.total_plans_created as f64
        }
    }

    /// Get average tokens per plan
    pub fn avg_tokens_per_plan(&self) -> f64 {
        if self.total_plans_created == 0 {
            0.0
        } else {
            self.total_tokens_used as f64 / self.total_plans_created as f64
        }
    }
}

impl Default for PlannerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Execution record for historical analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    /// Timestamp
    pub timestamp: i64,

    /// Complexity level
    pub complexity: ComplexityLevel,

    /// Estimated tokens
    pub estimated_tokens: usize,

    /// Actual tokens used
    pub actual_tokens: usize,

    /// Estimated cost
    pub estimated_cost: f64,

    /// Actual cost
    pub actual_cost: f64,

    /// Model used
    pub model_used: String,

    /// Planning time (ms)
    pub planning_time_ms: u128,

    /// Execution time (ms)
    pub execution_time_ms: u128,

    /// Whether budget was exceeded
    pub budget_exceeded: bool,

    /// Optimizations applied
    pub optimizations: Vec<String>,
}

impl ExecutionRecord {
    /// Calculate estimation accuracy
    pub fn estimation_accuracy(&self) -> f32 {
        if self.actual_tokens > 0 {
            1.0 - ((self.estimated_tokens as f32 - self.actual_tokens as f32).abs()
                / self.actual_tokens as f32)
        } else {
            1.0
        }
    }

    /// Calculate cost accuracy
    pub fn cost_accuracy(&self) -> f32 {
        if self.actual_cost > 0.0 {
            1.0 - ((self.estimated_cost - self.actual_cost).abs() as f32 / self.actual_cost as f32)
        } else {
            1.0
        }
    }
}

/// Metrics tracker
pub struct MetricsTracker {
    metrics: Arc<RwLock<PlannerMetrics>>,
    history: Arc<RwLock<Vec<ExecutionRecord>>>,
    max_history: usize,
}

impl MetricsTracker {
    /// Create a new metrics tracker
    pub fn new(max_history: usize) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(PlannerMetrics::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            max_history,
        }
    }

    /// Record a plan execution
    pub fn record_execution(&self, record: ExecutionRecord) {
        // Update metrics
        let mut metrics = self.metrics.write().unwrap();
        metrics.record_plan(
            record.complexity,
            record.actual_tokens,
            record.actual_cost,
            record.planning_time_ms,
        );
        metrics.record_accuracy(record.estimated_tokens, record.actual_tokens);

        if record.budget_exceeded {
            metrics.record_budget_violation();
        }

        if !record.optimizations.is_empty() {
            metrics.record_model_switch();
        }

        drop(metrics);

        // Add to history
        let mut history = self.history.write().unwrap();
        history.push(record);

        // Trim history if needed
        if history.len() > self.max_history {
            history.remove(0);
        }
    }

    /// Get current metrics
    pub fn get_metrics(&self) -> PlannerMetrics {
        self.metrics.read().unwrap().clone()
    }

    /// Get execution history
    pub fn get_history(&self) -> Vec<ExecutionRecord> {
        self.history.read().unwrap().clone()
    }

    /// Get recent records (last N)
    pub fn get_recent(&self, count: usize) -> Vec<ExecutionRecord> {
        let history = self.history.read().unwrap();
        let start = history.len().saturating_sub(count);
        history[start..].to_vec()
    }

    /// Get average accuracy for a complexity level
    pub fn get_accuracy_for_complexity(&self, complexity: ComplexityLevel) -> f32 {
        let history = self.history.read().unwrap();
        let records: Vec<_> = history
            .iter()
            .filter(|r| r.complexity == complexity)
            .collect();

        if records.is_empty() {
            return 0.0;
        }

        let total_accuracy: f32 = records.iter().map(|r| r.estimation_accuracy()).sum();
        total_accuracy / records.len() as f32
    }

    /// Clear all metrics and history
    pub fn clear(&self) {
        *self.metrics.write().unwrap() = PlannerMetrics::new();
        self.history.write().unwrap().clear();
    }
}

impl Default for MetricsTracker {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = PlannerMetrics::new();
        assert_eq!(metrics.total_plans_created, 0);
        assert_eq!(metrics.total_cost_usd, 0.0);
    }

    #[test]
    fn test_record_plan() {
        let mut metrics = PlannerMetrics::new();

        metrics.record_plan(ComplexityLevel::Simple, 1000, 0.05, 100);

        assert_eq!(metrics.total_plans_created, 1);
        assert_eq!(metrics.total_tokens_used, 1000);
        assert_eq!(metrics.total_cost_usd, 0.05);
        assert_eq!(metrics.avg_planning_time_ms, 100.0);
    }

    #[test]
    fn test_accuracy_recording() {
        let mut metrics = PlannerMetrics::new();

        metrics.record_plan(ComplexityLevel::Simple, 1000, 0.05, 100);
        metrics.record_accuracy(1000, 1000); // Perfect accuracy

        assert_eq!(metrics.avg_estimation_accuracy, 1.0);
    }

    #[test]
    fn test_metrics_tracker() {
        let tracker = MetricsTracker::new(10);

        let record = ExecutionRecord {
            timestamp: chrono::Utc::now().timestamp(),
            complexity: ComplexityLevel::Simple,
            estimated_tokens: 1000,
            actual_tokens: 950,
            estimated_cost: 0.05,
            actual_cost: 0.048,
            model_used: "gpt-4".to_string(),
            planning_time_ms: 100,
            execution_time_ms: 500,
            budget_exceeded: false,
            optimizations: vec![],
        };

        tracker.record_execution(record);

        let metrics = tracker.get_metrics();
        assert_eq!(metrics.total_plans_created, 1);
    }

    #[test]
    fn test_history_trimming() {
        let tracker = MetricsTracker::new(5);

        // Add 10 records
        for i in 0..10 {
            let record = ExecutionRecord {
                timestamp: chrono::Utc::now().timestamp(),
                complexity: ComplexityLevel::Simple,
                estimated_tokens: 1000 + i,
                actual_tokens: 950 + i,
                estimated_cost: 0.05,
                actual_cost: 0.048,
                model_used: "gpt-4".to_string(),
                planning_time_ms: 100,
                execution_time_ms: 500,
                budget_exceeded: false,
                optimizations: vec![],
            };
            tracker.record_execution(record);
        }

        let history = tracker.get_history();
        assert_eq!(history.len(), 5); // Should only keep 5
    }
}
