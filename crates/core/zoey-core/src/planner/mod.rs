//! Planning phase for agent operations
//!
//! This module provides comprehensive planning capabilities including:
//! - Task complexity assessment
//! - Knowledge gap analysis
//! - Token and cost estimation
//! - Budget management
//! - Response strategy planning
//! - Emoji usage planning
//! - Plan optimization
//! - Metrics tracking

pub mod budget;
pub mod complexity;
pub mod cost;
pub mod emoji;
pub mod knowledge;
pub mod metrics;
pub mod optimization;
pub mod tokens;

use crate::types::*;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, instrument};

// Re-export commonly used types
pub use budget::{AgentBudget, BudgetAction, BudgetCheckResult, BudgetManager};
pub use complexity::{ComplexityAnalyzer, ComplexityAssessment, ComplexityLevel, TokenEstimate};
pub use cost::{CostCalculator, CostEstimate, ModelPricing};
pub use emoji::{EmojiPlanner, EmojiStrategy, EmojiTone, EmojiType};
pub use knowledge::{KnowledgeAnalyzer, KnowledgeGap, KnowledgeState, Priority};
pub use metrics::{ExecutionRecord, MetricsTracker, PlannerMetrics};
pub use optimization::{Optimization, PlanOptimizer};
pub use tokens::{TokenBudget, TokenCounter, TokenTracker};

/// Response tone
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResponseTone {
    Professional,
    Friendly,
    Technical,
    Casual,
    Formal,
    Empathetic,
}

/// Response type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResponseType {
    Answer,
    Explanation,
    Instruction,
    Acknowledgment,
    Question,
    Clarification,
}

/// Response strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseStrategy {
    /// Whether to respond
    pub should_respond: bool,
    /// Type of response
    pub response_type: ResponseType,
    /// Tone of response
    pub tone: ResponseTone,
    /// Emoji strategy
    pub emoji_strategy: EmojiStrategy,
    /// Maximum tokens for response
    pub max_tokens: usize,
    /// Model to use
    pub model_selection: String,
}

/// Complete execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlan {
    /// Complexity assessment
    pub complexity: ComplexityAssessment,
    /// Knowledge state
    pub knowledge: KnowledgeState,
    /// Token estimate
    pub token_estimate: TokenEstimate,
    /// Cost estimate
    pub cost_estimate: CostEstimate,
    /// Budget check result
    pub budget_check: BudgetCheckResult,
    /// Response strategy
    pub response_strategy: ResponseStrategy,
    /// Optimizations applied
    pub optimizations_applied: Vec<Optimization>,
    /// Warnings
    pub warnings: Vec<String>,
    /// Whether user approval is required
    pub requires_approval: bool,
    /// Planning timestamp
    pub planned_at: i64,
    /// Planning duration (ms)
    pub planning_duration_ms: u128,
}

impl ExecutionPlan {
    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        format!(
            "Plan Summary:\n\
            - Complexity: {} (confidence: {:.2})\n\
            - Knowledge: {} | {}\n\
            - Tokens: {} (input: {}, output: {})\n\
            - Cost: ${:.4} using {}\n\
            - Budget: ${:.4} available | {:.1}% utilized\n\
            - Response: {:?} tone, {:?} type\n\
            - Emojis: {} (max: {})\n\
            - Optimizations: {}\n\
            - Warnings: {}",
            self.complexity.level,
            self.complexity.confidence,
            self.knowledge.known_facts.len(),
            self.knowledge.summary,
            self.token_estimate.total_tokens,
            self.token_estimate.input_tokens,
            self.token_estimate.output_tokens,
            self.cost_estimate.estimated_cost_usd,
            self.cost_estimate.model_used,
            self.budget_check.available_budget,
            self.budget_check.utilization * 100.0,
            self.response_strategy.tone,
            self.response_strategy.response_type,
            if self.response_strategy.emoji_strategy.should_use_emojis {
                "Yes"
            } else {
                "No"
            },
            self.response_strategy.emoji_strategy.max_emojis,
            self.optimizations_applied.len(),
            self.warnings.len()
        )
    }
}

/// Planner configuration
#[derive(Debug, Clone)]
pub struct PlannerConfig {
    /// Budget configuration
    pub budget: AgentBudget,
    /// Model pricing database
    pub model_pricing: HashMap<String, ModelPricing>,
    /// Default model to use
    pub default_model: String,
    /// Maximum history for metrics
    pub max_history: usize,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        let calculator = CostCalculator::new();
        let mut pricing = HashMap::new();

        // Load default pricing
        for model in calculator.get_models_by_cost() {
            pricing.insert(model.model_name.clone(), model);
        }

        Self {
            budget: AgentBudget::new(100.0, BudgetAction::Warn),
            model_pricing: pricing,
            default_model: "gpt-4".to_string(),
            max_history: 1000,
        }
    }
}

/// Main planner service
pub struct Planner {
    /// Complexity analyzer
    complexity_analyzer: ComplexityAnalyzer,
    /// Knowledge analyzer
    knowledge_analyzer: KnowledgeAnalyzer,
    /// Cost calculator
    cost_calculator: Arc<CostCalculator>,
    /// Budget manager
    budget_manager: Arc<BudgetManager>,
    /// Plan optimizer
    optimizer: PlanOptimizer,
    /// Emoji planner
    emoji_planner: EmojiPlanner,
    /// Token tracker
    token_tracker: Arc<TokenTracker>,
    /// Metrics tracker
    metrics_tracker: Arc<MetricsTracker>,
    /// Default model
    default_model: String,
}

impl Planner {
    /// Create a new planner with configuration
    pub fn new(config: PlannerConfig) -> Self {
        let cost_calculator = Arc::new(CostCalculator::new());

        // Load custom pricing if provided
        for (_, pricing) in config.model_pricing {
            cost_calculator.set_pricing(pricing);
        }

        Self {
            complexity_analyzer: ComplexityAnalyzer::new(),
            knowledge_analyzer: KnowledgeAnalyzer::new(),
            cost_calculator,
            budget_manager: Arc::new(BudgetManager::new(config.budget)),
            optimizer: PlanOptimizer::new(),
            emoji_planner: EmojiPlanner::new(),
            token_tracker: Arc::new(TokenTracker::new()),
            metrics_tracker: Arc::new(MetricsTracker::new(config.max_history)),
            default_model: config.default_model,
        }
    }

    /// Main planning method - creates execution plan
    #[instrument(skip(self, message, state), level = "info")]
    pub async fn plan_execution(&self, message: &Memory, state: &State) -> Result<ExecutionPlan> {
        let start_time = Instant::now();

        info!("Starting execution planning");

        // 1. Assess complexity
        debug!("Assessing complexity");
        let complexity = self.complexity_analyzer.assess(message, state).await?;

        // 2. Analyze knowledge state
        debug!("Analyzing knowledge state");
        let knowledge = self.knowledge_analyzer.analyze(message, state).await?;

        // 3. Get token estimate from complexity assessment
        let token_estimate = complexity.estimated_tokens.clone();

        // 4. Determine model to use
        let model = state
            .data
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.default_model)
            .to_string();

        // 5. Calculate cost
        debug!("Calculating cost for model: {}", model);
        let cost_estimate = self.cost_calculator.calculate_cost(
            &model,
            token_estimate.input_tokens,
            token_estimate.output_tokens,
        )?;

        // 6. Check budget
        debug!("Checking budget");
        let mut budget_check = self.budget_manager.check_budget(&cost_estimate)?;

        // 7. Plan response strategy
        debug!("Planning response strategy");
        let emoji_strategy = self.emoji_planner.plan_emoji_usage(message, state).await?;

        let response_strategy = ResponseStrategy {
            should_respond: true,
            response_type: self.determine_response_type(message),
            tone: self.determine_tone(message, state),
            emoji_strategy,
            max_tokens: token_estimate.output_tokens,
            model_selection: model.clone(),
        };

        // 8. Create initial plan
        let mut plan = ExecutionPlan {
            complexity,
            knowledge,
            token_estimate,
            cost_estimate,
            budget_check: budget_check.clone(),
            response_strategy,
            optimizations_applied: vec![],
            warnings: vec![],
            requires_approval: false,
            planned_at: chrono::Utc::now().timestamp(),
            planning_duration_ms: 0, // Will be set at the end
        };

        // 9. Optimize if needed
        if !budget_check.approved {
            debug!("Budget not approved, optimizing plan");
            plan = self
                .optimizer
                .optimize(plan, &budget_check, &self.cost_calculator)
                .await?;

            // Re-check budget after optimization
            budget_check = self.budget_manager.check_budget(&plan.cost_estimate)?;
            plan.budget_check = budget_check;
        }

        // 10. Add token optimization warnings
        let token_optimizations = self.optimizer.optimize_tokens(&mut plan);
        plan.optimizations_applied.extend(token_optimizations);

        // 11. Add suggestions as warnings
        let suggestions = self.optimizer.suggest_optimizations(&plan);
        plan.warnings.extend(suggestions);

        // Set planning duration
        plan.planning_duration_ms = start_time.elapsed().as_millis();

        info!(
            "Planning complete in {}ms - Complexity: {}, Cost: ${:.4}",
            plan.planning_duration_ms, plan.complexity.level, plan.cost_estimate.estimated_cost_usd
        );

        Ok(plan)
    }

    /// Record actual execution results for metrics
    pub fn record_execution(
        &self,
        plan: &ExecutionPlan,
        actual_usage: &TokenUsage,
        actual_cost: f64,
        execution_time_ms: u128,
        session_id: uuid::Uuid,
    ) -> Result<()> {
        // Record in token tracker
        self.token_tracker
            .record_usage(session_id, actual_usage, actual_cost)?;

        // Record in metrics tracker
        let record = ExecutionRecord {
            timestamp: chrono::Utc::now().timestamp(),
            complexity: plan.complexity.level,
            estimated_tokens: plan.token_estimate.total_tokens,
            actual_tokens: actual_usage.total_tokens,
            estimated_cost: plan.cost_estimate.estimated_cost_usd,
            actual_cost,
            model_used: plan.cost_estimate.model_used.clone(),
            planning_time_ms: plan.planning_duration_ms,
            execution_time_ms,
            budget_exceeded: !plan.budget_check.approved,
            optimizations: plan
                .optimizations_applied
                .iter()
                .map(|o| format!("{:?}", o))
                .collect(),
        };

        self.metrics_tracker.record_execution(record);

        // Commit budget
        self.budget_manager.commit(actual_cost)?;

        Ok(())
    }

    /// Get planner metrics
    pub fn get_metrics(&self) -> PlannerMetrics {
        self.metrics_tracker.get_metrics()
    }

    /// Get remaining budget
    pub fn get_remaining_budget(&self) -> f64 {
        self.budget_manager.get_remaining()
    }

    /// Get budget utilization
    pub fn get_budget_utilization(&self) -> f64 {
        self.budget_manager.get_utilization()
    }

    /// Get token tracker
    pub fn get_token_tracker(&self) -> Arc<TokenTracker> {
        Arc::clone(&self.token_tracker)
    }

    /// Get cost calculator
    pub fn get_cost_calculator(&self) -> Arc<CostCalculator> {
        Arc::clone(&self.cost_calculator)
    }

    /// Get budget manager
    pub fn get_budget_manager(&self) -> Arc<BudgetManager> {
        Arc::clone(&self.budget_manager)
    }

    /// Determine response type from message
    fn determine_response_type(&self, message: &Memory) -> ResponseType {
        let text = message.content.text.to_lowercase();

        if text.contains('?') {
            ResponseType::Answer
        } else if text.contains("how") || text.contains("why") || text.contains("explain") {
            ResponseType::Explanation
        } else if text.contains("show me") || text.contains("tell me") || text.contains("help me") {
            ResponseType::Instruction
        } else if text.len() < 20 {
            ResponseType::Acknowledgment
        } else {
            ResponseType::Answer
        }
    }

    /// Determine response tone
    fn determine_tone(&self, message: &Memory, state: &State) -> ResponseTone {
        let text = message.content.text.to_lowercase();

        // Check character settings
        if let Some(settings) = state.data.get("characterSettings") {
            if let Some(tone) = settings.get("preferredTone") {
                if let Some(tone_str) = tone.as_str() {
                    match tone_str {
                        "professional" => return ResponseTone::Professional,
                        "friendly" => return ResponseTone::Friendly,
                        "technical" => return ResponseTone::Technical,
                        "casual" => return ResponseTone::Casual,
                        "formal" => return ResponseTone::Formal,
                        _ => {}
                    }
                }
            }
        }

        // Detect from content
        if text.contains("code") || text.contains("algorithm") || text.contains("function") {
            ResponseTone::Technical
        } else if text.contains("please") || text.contains("thank") {
            ResponseTone::Professional
        } else if text.len() < 30 && (text.contains("hi") || text.contains("hey")) {
            ResponseTone::Friendly
        } else {
            ResponseTone::Professional
        }
    }
}

impl Default for Planner {
    fn default() -> Self {
        Self::new(PlannerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_message(text: &str) -> Memory {
        Memory {
            id: Uuid::new_v4(),
            entity_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: Content {
                text: text.to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        }
    }

    #[tokio::test]
    async fn test_planner_creation() {
        let planner = Planner::default();
        let metrics = planner.get_metrics();
        assert_eq!(metrics.total_plans_created, 0);
    }

    #[tokio::test]
    async fn test_plan_execution() {
        let planner = Planner::default();
        let message = create_test_message("How does Rust handle memory management?");
        let state = State::new();

        let plan = planner.plan_execution(&message, &state).await.unwrap();

        assert!(matches!(
            plan.complexity.level,
            ComplexityLevel::Trivial
                | ComplexityLevel::Simple
                | ComplexityLevel::Moderate
                | ComplexityLevel::Complex
        ));
        assert!(plan.cost_estimate.estimated_cost_usd > 0.0);
        assert!(plan.token_estimate.total_tokens > 0);
    }

    #[tokio::test]
    async fn test_budget_management() {
        let config = PlannerConfig {
            budget: AgentBudget::new(0.001, BudgetAction::Block), // Very small budget
            ..Default::default()
        };

        let planner = Planner::new(config);
        let message = create_test_message("Explain quantum computing in detail.");
        let state = State::new();

        // This should fail due to budget
        let result = planner.plan_execution(&message, &state).await;

        // Should either fail or optimize heavily
        match result {
            Ok(plan) => {
                // If it succeeded, it must have optimized
                assert!(!plan.optimizations_applied.is_empty());
            }
            Err(_) => {
                // Or it blocked due to budget
                assert!(true);
            }
        }
    }

    #[tokio::test]
    async fn test_execution_recording() {
        let planner = Planner::default();
        let message = create_test_message("Hello!");
        let state = State::new();

        let plan = planner.plan_execution(&message, &state).await.unwrap();

        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };

        let session_id = Uuid::new_v4();
        planner
            .record_execution(&plan, &usage, 0.001, 500, session_id)
            .unwrap();

        let metrics = planner.get_metrics();
        assert_eq!(metrics.total_plans_created, 1);
    }
}
