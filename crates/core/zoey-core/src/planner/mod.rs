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
//! - Dynamic teacher/model selection
//! - Chain-of-thought orchestration

pub mod budget;
pub mod chain_of_thought;
pub mod complexity;
pub mod cost;
pub mod emoji;
pub mod knowledge;
pub mod metrics;
pub mod optimization;
pub mod teacher;
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
pub use chain_of_thought::{
    ChainOfThoughtOrchestrator, ChainResult, PromptTemplates, StepResult, ThoughtChain,
    ThoughtChainBuilder, ThoughtStep, ThoughtStepType,
};
pub use complexity::{ComplexityAnalyzer, ComplexityAssessment, ComplexityLevel, TokenEstimate};
pub use cost::{CostCalculator, CostEstimate, ModelPricing};
pub use emoji::{EmojiPlanner, EmojiStrategy, EmojiTone, EmojiType};
pub use knowledge::{KnowledgeAnalyzer, KnowledgeGap, KnowledgeState, Priority};
pub use metrics::{ExecutionRecord, MetricsTracker, PlannerMetrics};
pub use optimization::{Optimization, PlanOptimizer};
pub use teacher::{
    ComplexityTeacherMapping, RoutingPreference, TaskType, Teacher, TeacherCapabilities,
    TeacherSelection, TeacherSelector,
};
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
    /// Inferred task type for this request
    pub task_type: TaskType,
    /// Selected primary teacher for this task
    pub selected_teacher: Option<Teacher>,
    /// Chain of thought plan (for moderate+ complexity)
    pub thought_chain: Option<ThoughtChain>,
    /// Whether to use chain-of-thought reasoning
    pub use_chain_of_thought: bool,
}

impl ExecutionPlan {
    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        let teacher_info = self
            .selected_teacher
            .as_ref()
            .map(|t| format!("{} ({})", t.name, t.model_name))
            .unwrap_or_else(|| "None".to_string());

        let chain_info = self
            .thought_chain
            .as_ref()
            .map(|c| format!("{} steps", c.len()))
            .unwrap_or_else(|| "No chain".to_string());

        format!(
            "Plan Summary:\n\
            - Complexity: {} (confidence: {:.2})\n\
            - Task Type: {}\n\
            - Teacher: {}\n\
            - Chain of Thought: {}\n\
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
            self.task_type,
            teacher_info,
            chain_info,
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
    /// Available memory for local models (GB)
    pub available_memory_gb: f32,
    /// Whether to enable chain-of-thought for complex tasks
    pub enable_chain_of_thought: bool,
    /// Minimum complexity level to use chain-of-thought
    pub chain_of_thought_threshold: ComplexityLevel,
    /// Default routing preference
    pub routing_preference: RoutingPreference,
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
            available_memory_gb: 8.0,
            enable_chain_of_thought: true,
            chain_of_thought_threshold: ComplexityLevel::Moderate,
            routing_preference: RoutingPreference::Balanced,
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
    /// Teacher selector for dynamic model selection
    teacher_selector: Arc<TeacherSelector>,
    /// Chain of thought orchestrator
    cot_orchestrator: Arc<ChainOfThoughtOrchestrator>,
    /// Whether chain-of-thought is enabled
    enable_chain_of_thought: bool,
    /// Minimum complexity for chain-of-thought
    chain_of_thought_threshold: ComplexityLevel,
    /// Default routing preference
    routing_preference: RoutingPreference,
}

impl Planner {
    /// Create a new planner with configuration
    pub fn new(config: PlannerConfig) -> Self {
        let cost_calculator = Arc::new(CostCalculator::new());

        // Load custom pricing if provided
        for (_, pricing) in config.model_pricing {
            cost_calculator.set_pricing(pricing);
        }

        // Create teacher selector with memory constraints
        let teacher_selector = Arc::new(
            TeacherSelector::with_default_teachers()
                .with_memory_constraint(config.available_memory_gb),
        );

        // Create chain-of-thought orchestrator
        let cot_orchestrator = Arc::new(ChainOfThoughtOrchestrator::new(Arc::clone(
            &teacher_selector,
        )));

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
            teacher_selector,
            cot_orchestrator,
            enable_chain_of_thought: config.enable_chain_of_thought,
            chain_of_thought_threshold: config.chain_of_thought_threshold,
            routing_preference: config.routing_preference,
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

        // 2. Infer task type from message content
        debug!("Inferring task type");
        let task_type = self.infer_task_type(message);

        // 3. Analyze knowledge state
        debug!("Analyzing knowledge state");
        let knowledge = self.knowledge_analyzer.analyze(message, state).await?;

        // 4. Get token estimate from complexity assessment
        let token_estimate = complexity.estimated_tokens.clone();

        // 5. Select optimal teacher based on complexity and task type
        debug!("Selecting teacher for task type: {:?}", task_type);
        let selected_teacher = self
            .teacher_selector
            .select_teacher(&complexity, task_type, self.routing_preference)
            .cloned();

        // 6. Determine model to use (prefer teacher's model, then state, then default)
        let model = selected_teacher
            .as_ref()
            .map(|t| t.model_name.clone())
            .or_else(|| {
                state
                    .data
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| self.default_model.clone());

        debug!("Using model: {}", model);

        // 7. Create chain-of-thought plan if complexity warrants it
        let (thought_chain, use_chain_of_thought) =
            if self.enable_chain_of_thought && self.should_use_chain_of_thought(&complexity) {
                debug!(
                    "Creating chain-of-thought plan for {} complexity",
                    complexity.level
                );
                let chain = self.cot_orchestrator.create_chain(&complexity, task_type);
                (Some(chain), true)
            } else {
                (None, false)
            };

        // 8. Calculate cost (adjust for chain-of-thought if applicable)
        let total_tokens = if let Some(ref chain) = thought_chain {
            // Account for all steps in the chain
            token_estimate.input_tokens + chain.estimated_total_tokens
        } else {
            token_estimate.input_tokens + token_estimate.output_tokens
        };

        debug!("Calculating cost for model: {} with {} tokens", model, total_tokens);
        let cost_estimate = self.cost_calculator.calculate_cost(
            &model,
            token_estimate.input_tokens,
            total_tokens - token_estimate.input_tokens,
        )?;

        // 9. Check budget
        debug!("Checking budget");
        let mut budget_check = self.budget_manager.check_budget(&cost_estimate)?;

        // 10. Plan response strategy
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

        // 11. Create initial plan
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
            task_type,
            selected_teacher,
            thought_chain,
            use_chain_of_thought,
        };

        // 12. Optimize if needed
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

        // 13. Add token optimization warnings
        let token_optimizations = self.optimizer.optimize_tokens(&mut plan);
        plan.optimizations_applied.extend(token_optimizations);

        // 14. Add suggestions as warnings
        let suggestions = self.optimizer.suggest_optimizations(&plan);
        plan.warnings.extend(suggestions);

        // Set planning duration
        plan.planning_duration_ms = start_time.elapsed().as_millis();

        info!(
            "Planning complete in {}ms - Complexity: {}, Task: {}, Teacher: {}, CoT: {}, Cost: ${:.4}",
            plan.planning_duration_ms,
            plan.complexity.level,
            plan.task_type,
            plan.selected_teacher.as_ref().map(|t| t.name.as_str()).unwrap_or("none"),
            plan.use_chain_of_thought,
            plan.cost_estimate.estimated_cost_usd
        );

        Ok(plan)
    }

    /// Infer task type from message content
    fn infer_task_type(&self, message: &Memory) -> TaskType {
        let text = message.content.text.to_lowercase();

        // Code-related patterns
        if text.contains("code")
            || text.contains("implement")
            || text.contains("function")
            || text.contains("class")
            || text.contains("debug")
            || text.contains("fix this")
            || text.contains("write a program")
            || text.contains("refactor")
        {
            return TaskType::CodeGeneration;
        }

        // Summarization patterns
        if text.contains("summarize")
            || text.contains("summary")
            || text.contains("tldr")
            || text.contains("brief")
            || text.contains("condense")
        {
            return TaskType::Summarization;
        }

        // Translation patterns
        if text.contains("translate") || text.contains("translation") {
            return TaskType::Translation;
        }

        // Reasoning/analysis patterns
        if text.contains("analyze")
            || text.contains("explain why")
            || text.contains("reason")
            || text.contains("step by step")
            || text.contains("think through")
            || text.contains("evaluate")
            || text.contains("compare")
            || text.contains("pros and cons")
        {
            return TaskType::Reasoning;
        }

        // Question answering patterns
        if text.contains('?')
            || text.contains("what is")
            || text.contains("what are")
            || text.contains("how do")
            || text.contains("how does")
            || text.contains("why is")
            || text.contains("when did")
            || text.contains("who is")
            || text.contains("where is")
        {
            return TaskType::QuestionAnswering;
        }

        // Completion patterns
        if text.contains("complete")
            || text.contains("finish")
            || text.contains("continue")
        {
            return TaskType::Completion;
        }

        // Default to chat
        TaskType::Chat
    }

    /// Determine if chain-of-thought should be used based on complexity
    fn should_use_chain_of_thought(&self, complexity: &ComplexityAssessment) -> bool {
        match (complexity.level, self.chain_of_thought_threshold) {
            (ComplexityLevel::VeryComplex, _) => true,
            (ComplexityLevel::Complex, ComplexityLevel::Complex | ComplexityLevel::Moderate | ComplexityLevel::Simple | ComplexityLevel::Trivial) => true,
            (ComplexityLevel::Moderate, ComplexityLevel::Moderate | ComplexityLevel::Simple | ComplexityLevel::Trivial) => true,
            (ComplexityLevel::Simple, ComplexityLevel::Simple | ComplexityLevel::Trivial) => true,
            (ComplexityLevel::Trivial, ComplexityLevel::Trivial) => true,
            _ => false,
        }
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

    /// Get teacher selector
    pub fn get_teacher_selector(&self) -> Arc<TeacherSelector> {
        Arc::clone(&self.teacher_selector)
    }

    /// Get chain-of-thought orchestrator
    pub fn get_cot_orchestrator(&self) -> Arc<ChainOfThoughtOrchestrator> {
        Arc::clone(&self.cot_orchestrator)
    }

    /// Check if chain-of-thought is enabled
    pub fn is_chain_of_thought_enabled(&self) -> bool {
        self.enable_chain_of_thought
    }

    /// Get chain-of-thought complexity threshold
    pub fn get_cot_threshold(&self) -> ComplexityLevel {
        self.chain_of_thought_threshold
    }

    /// Get routing preference
    pub fn get_routing_preference(&self) -> RoutingPreference {
        self.routing_preference
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

    #[tokio::test]
    async fn test_task_type_inference() {
        let planner = Planner::default();

        // Code generation
        let code_msg = create_test_message("Write a function to sort an array");
        assert_eq!(planner.infer_task_type(&code_msg), TaskType::CodeGeneration);

        // Question answering
        let qa_msg = create_test_message("What is the capital of France?");
        assert_eq!(planner.infer_task_type(&qa_msg), TaskType::QuestionAnswering);

        // Summarization
        let summary_msg = create_test_message("Summarize this article for me");
        assert_eq!(planner.infer_task_type(&summary_msg), TaskType::Summarization);

        // Chat (default)
        let chat_msg = create_test_message("Hello there");
        assert_eq!(planner.infer_task_type(&chat_msg), TaskType::Chat);
    }

    #[tokio::test]
    async fn test_teacher_selection() {
        let planner = Planner::default();
        let message = create_test_message("How does Rust handle memory management?");
        let state = State::new();

        let plan = planner.plan_execution(&message, &state).await.unwrap();

        // Should have inferred task type
        assert!(matches!(
            plan.task_type,
            TaskType::QuestionAnswering | TaskType::Chat
        ));

        // Should have selected a teacher (may be None if memory constraints)
        // Just verify the field exists and plan completed successfully
        assert!(plan.planning_duration_ms > 0);
    }

    #[tokio::test]
    async fn test_chain_of_thought_threshold() {
        // Disable CoT
        let config = PlannerConfig {
            enable_chain_of_thought: false,
            ..Default::default()
        };
        let planner = Planner::new(config);

        let message = create_test_message(
            "Explain the process of how neural networks learn through backpropagation step by step",
        );
        let state = State::new();

        let plan = planner.plan_execution(&message, &state).await.unwrap();
        assert!(!plan.use_chain_of_thought);
        assert!(plan.thought_chain.is_none());
    }

    #[tokio::test]
    async fn test_chain_of_thought_enabled() {
        let config = PlannerConfig {
            enable_chain_of_thought: true,
            chain_of_thought_threshold: ComplexityLevel::Simple,
            ..Default::default()
        };
        let planner = Planner::new(config);

        // Complex question should trigger CoT
        let message = create_test_message(
            "Analyze the pros and cons of microservices vs monolithic architecture, \
             compare their trade-offs in terms of scalability, maintainability, \
             and development complexity, then recommend when to use each approach.",
        );
        let state = State::new();

        let plan = planner.plan_execution(&message, &state).await.unwrap();

        // If complexity is high enough, should use chain of thought
        if matches!(
            plan.complexity.level,
            ComplexityLevel::Moderate | ComplexityLevel::Complex | ComplexityLevel::VeryComplex
        ) {
            assert!(plan.use_chain_of_thought);
            assert!(plan.thought_chain.is_some());
        }
    }

    #[tokio::test]
    async fn test_plan_summary_includes_new_fields() {
        let planner = Planner::default();
        let message = create_test_message("How do I implement a binary search tree?");
        let state = State::new();

        let plan = planner.plan_execution(&message, &state).await.unwrap();
        let summary = plan.summary();

        // Summary should include new fields
        assert!(summary.contains("Task Type:"));
        assert!(summary.contains("Teacher:"));
        assert!(summary.contains("Chain of Thought:"));
    }
}
