//! Chain of Thought Orchestration
//!
//! This module provides multi-step reasoning across different models:
//! - Creates execution chains based on task complexity
//! - Orchestrates thinking, drafting, and refining steps
//! - Passes context between models in the chain
//! - Enables capabilities beyond any single model

use crate::planner::complexity::{ComplexityAssessment, ComplexityLevel};
use crate::planner::teacher::{TaskType, Teacher, TeacherSelector};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Type of step in the chain of thought
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThoughtStepType {
    /// Analyze and break down the problem
    Analyze,
    /// Gather relevant information/context
    Research,
    /// Apply logical reasoning to the problem
    Reason,
    /// Generate initial response/draft
    Draft,
    /// Self-review and critique the draft
    Critique,
    /// Refine and improve based on critique
    Refine,
    /// Synthesize multiple inputs into coherent output
    Synthesize,
}

impl std::fmt::Display for ThoughtStepType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThoughtStepType::Analyze => write!(f, "Analyze"),
            ThoughtStepType::Research => write!(f, "Research"),
            ThoughtStepType::Reason => write!(f, "Reason"),
            ThoughtStepType::Draft => write!(f, "Draft"),
            ThoughtStepType::Critique => write!(f, "Critique"),
            ThoughtStepType::Refine => write!(f, "Refine"),
            ThoughtStepType::Synthesize => write!(f, "Synthesize"),
        }
    }
}

/// Model tier for complexity-based model selection
/// Higher tiers use stronger/larger models
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    /// Fastest models (phi3:mini, qwen2.5:3b) - for trivial tasks
    Fast,
    /// Light models (llama3.2:3b) - for simple tasks
    Light,
    /// Balanced models (llama3.2:3b, qwen2.5:7b) - for moderate tasks
    Balanced,
    /// Strong models (llama3.1:8b) - for complex tasks
    Strong,
    /// Maximum strength (codellama:13b, largest available) - for very complex tasks
    Maximum,
}

/// Role a step plays in the chain (affects model selection)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepRole {
    /// Analytical/reasoning tasks
    Reasoning,
    /// Text generation tasks
    Generation,
    /// Critical review tasks
    Critique,
    /// Code-related tasks
    Coding,
}

/// A single step in the chain of thought
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtStep {
    /// Unique identifier for this step
    pub id: Uuid,
    /// Type of step
    pub step_type: ThoughtStepType,
    /// Teacher/model to use for this step
    pub teacher_id: Option<Uuid>,
    /// Model name (resolved at execution time if teacher_id is None)
    pub model_name: Option<String>,
    /// Prompt template for this step (uses {input}, {context}, {previous_output} placeholders)
    pub prompt_template: String,
    /// Indices of steps this depends on (must complete before this step)
    pub depends_on: Vec<usize>,
    /// Maximum tokens for this step's output
    pub max_tokens: usize,
    /// Temperature for generation
    pub temperature: f32,
    /// Whether this step's output should be included in final response
    pub include_in_output: bool,
}

impl ThoughtStep {
    /// Create a new thought step
    pub fn new(step_type: ThoughtStepType, prompt_template: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            step_type,
            teacher_id: None,
            model_name: None,
            prompt_template: prompt_template.into(),
            depends_on: Vec::new(),
            max_tokens: 1024,
            temperature: 0.7,
            include_in_output: false,
        }
    }

    /// Builder: set teacher
    pub fn with_teacher(mut self, teacher_id: Uuid) -> Self {
        self.teacher_id = Some(teacher_id);
        self
    }

    /// Builder: set model name
    pub fn with_model(mut self, model_name: impl Into<String>) -> Self {
        self.model_name = Some(model_name.into());
        self
    }

    /// Builder: set dependencies
    pub fn depends_on(mut self, indices: Vec<usize>) -> Self {
        self.depends_on = indices;
        self
    }

    /// Builder: set max tokens
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Builder: set temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Builder: include in output
    pub fn include_in_output(mut self) -> Self {
        self.include_in_output = true;
        self
    }
}

/// A complete chain of thought execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtChain {
    /// Unique identifier
    pub id: Uuid,
    /// Steps in the chain (ordered)
    pub steps: Vec<ThoughtStep>,
    /// Task type this chain is designed for
    pub task_type: TaskType,
    /// Complexity level this chain handles
    pub complexity_level: ComplexityLevel,
    /// Description of what this chain does
    pub description: String,
    /// Estimated total tokens for the chain
    pub estimated_total_tokens: usize,
}

impl ThoughtChain {
    /// Create a new thought chain
    pub fn new(task_type: TaskType, complexity_level: ComplexityLevel) -> Self {
        Self {
            id: Uuid::new_v4(),
            steps: Vec::new(),
            task_type,
            complexity_level,
            description: String::new(),
            estimated_total_tokens: 0,
        }
    }

    /// Add a step to the chain
    pub fn add_step(&mut self, step: ThoughtStep) {
        self.estimated_total_tokens += step.max_tokens;
        self.steps.push(step);
    }

    /// Builder: set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Get the number of steps
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Check if chain is empty
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Get steps that can run in parallel (no dependencies)
    pub fn get_parallel_steps(&self, completed: &[usize]) -> Vec<usize> {
        self.steps
            .iter()
            .enumerate()
            .filter(|(idx, step)| {
                // Not already completed
                !completed.contains(idx) &&
                // All dependencies are completed
                step.depends_on.iter().all(|dep| completed.contains(dep))
            })
            .map(|(idx, _)| idx)
            .collect()
    }
}

/// Result of executing a single step
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    /// Step index
    pub step_index: usize,
    /// Step type
    pub step_type: ThoughtStepType,
    /// Model used
    pub model_used: String,
    /// Output from this step
    pub output: String,
    /// Tokens used
    pub tokens_used: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Whether the step succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Result of executing a complete chain
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainResult {
    /// Chain ID
    pub chain_id: Uuid,
    /// Results from each step
    pub step_results: Vec<StepResult>,
    /// Final output (combined from steps marked include_in_output)
    pub final_output: String,
    /// Total tokens used across all steps
    pub total_tokens_used: usize,
    /// Total execution time
    pub total_execution_time_ms: u64,
    /// Whether the chain succeeded
    pub success: bool,
}

/// Prompt templates for different step types
pub struct PromptTemplates;

impl PromptTemplates {
    /// Template for analysis step
    pub fn analyze() -> &'static str {
        r#"Analyze the following request and break it down into key components:

Request: {input}

Context: {context}

Please identify:
1. The main objective
2. Key constraints or requirements
3. Information needed to complete this task
4. Potential challenges or considerations

Provide a structured analysis."#
    }

    /// Template for reasoning step
    pub fn reason() -> &'static str {
        r#"Based on the following analysis, apply logical reasoning to determine the best approach:

Analysis: {previous_output}

Original Request: {input}

Think step by step:
1. What are the most important factors to consider?
2. What are the possible approaches?
3. What are the trade-offs of each approach?
4. What is the recommended approach and why?

Provide your reasoning."#
    }

    /// Template for draft step
    pub fn draft() -> &'static str {
        r#"Based on the reasoning provided, generate a response to the original request:

Reasoning: {previous_output}

Original Request: {input}

Generate a complete, well-structured response that addresses the request."#
    }

    /// Template for critique step
    pub fn critique() -> &'static str {
        r#"Review the following draft response and provide constructive critique:

Draft Response: {previous_output}

Original Request: {input}

Evaluate:
1. Does it fully address the request?
2. Is it accurate and well-reasoned?
3. Is it clear and well-organized?
4. What could be improved?

Provide specific suggestions for improvement."#
    }

    /// Template for refine step
    pub fn refine() -> &'static str {
        r#"Improve the draft based on the critique provided:

Original Draft: {previous_output}

Critique: {context}

Original Request: {input}

Generate an improved version that addresses the critique while maintaining the strengths of the original."#
    }

    /// Template for synthesis step
    pub fn synthesize() -> &'static str {
        r#"Synthesize the following information into a coherent response:

Information gathered:
{previous_output}

Original Request: {input}

Create a unified, well-structured response that incorporates all relevant information."#
    }
}

/// Chain of Thought Orchestrator
///
/// Creates and manages execution chains based on task complexity
pub struct ChainOfThoughtOrchestrator {
    /// Teacher selector for choosing models
    teacher_selector: Arc<TeacherSelector>,
}

impl ChainOfThoughtOrchestrator {
    /// Create a new orchestrator
    pub fn new(teacher_selector: Arc<TeacherSelector>) -> Self {
        Self { teacher_selector }
    }

    /// Create an execution chain for a task
    ///
    /// NEW ARCHITECTURE: Fixed 3-step chain (Analyze → Draft → Refine)
    /// Complexity determines MODEL STRENGTH, not step count.
    ///
    /// - Trivial/Simple: Use fastest available models (phi3:mini, qwen2.5:3b)
    /// - Moderate: Use balanced models (llama3.2:3b)
    /// - Complex: Use strongest available models (llama3.1:8b, codellama:13b)
    /// - VeryComplex: Use strongest models with higher token budgets
    pub fn create_chain(
        &self,
        complexity: &ComplexityAssessment,
        task_type: TaskType,
    ) -> ThoughtChain {
        // Always use the standard 3-step chain
        // Complexity controls model selection and token budgets
        self.create_standard_chain(complexity, task_type)
    }

    /// Standard 3-step chain: Analyze → Draft → Refine
    /// Model strength and token budgets scale with complexity
    fn create_standard_chain(
        &self,
        complexity: &ComplexityAssessment,
        task_type: TaskType,
    ) -> ThoughtChain {
        let mut chain = ThoughtChain::new(task_type, complexity.level)
            .with_description(format!(
                "Standard chain with {} model strength",
                self.complexity_to_strength_label(complexity.level)
            ));

        // Get model tier based on complexity
        let model_tier = self.get_model_tier(complexity.level);

        // Get token budgets based on complexity
        let (analyze_tokens, draft_tokens, refine_tokens) =
            self.get_token_budgets(complexity.level);

        // Step 1: Analyze - understand the request
        let mut analyze = ThoughtStep::new(ThoughtStepType::Analyze, PromptTemplates::analyze())
            .with_max_tokens(analyze_tokens)
            .with_temperature(0.4); // Lower temp for analysis

        if let Some(teacher) = self.select_model_for_tier(model_tier, StepRole::Reasoning) {
            analyze = analyze.with_teacher(teacher.id);
        }

        // Step 2: Draft - generate response
        let mut draft = ThoughtStep::new(ThoughtStepType::Draft, PromptTemplates::draft())
            .depends_on(vec![0])
            .with_max_tokens(draft_tokens)
            .with_temperature(0.7); // Standard temp for generation

        if let Some(teacher) = self.select_model_for_tier(model_tier, StepRole::Generation) {
            draft = draft.with_teacher(teacher.id);
        }

        // Step 3: Refine - improve the draft
        let mut refine = ThoughtStep::new(ThoughtStepType::Refine, PromptTemplates::refine())
            .depends_on(vec![1])
            .with_max_tokens(refine_tokens)
            .with_temperature(0.5) // Balanced temp for refinement
            .include_in_output();

        if let Some(teacher) = self.select_model_for_tier(model_tier, StepRole::Generation) {
            refine = refine.with_teacher(teacher.id);
        }

        chain.add_step(analyze);
        chain.add_step(draft);
        chain.add_step(refine);
        chain
    }

    /// Map complexity to human-readable strength label
    fn complexity_to_strength_label(&self, level: ComplexityLevel) -> &'static str {
        match level {
            ComplexityLevel::Trivial => "minimal",
            ComplexityLevel::Simple => "light",
            ComplexityLevel::Moderate => "balanced",
            ComplexityLevel::Complex => "strong",
            ComplexityLevel::VeryComplex => "maximum",
        }
    }

    /// Get model tier (0-4) based on complexity
    /// Higher tier = stronger models
    fn get_model_tier(&self, level: ComplexityLevel) -> ModelTier {
        match level {
            ComplexityLevel::Trivial => ModelTier::Fast,      // phi3:mini, qwen2.5:3b
            ComplexityLevel::Simple => ModelTier::Light,      // llama3.2:3b
            ComplexityLevel::Moderate => ModelTier::Balanced, // llama3.2:3b, qwen2.5:7b
            ComplexityLevel::Complex => ModelTier::Strong,    // llama3.1:8b
            ComplexityLevel::VeryComplex => ModelTier::Maximum, // codellama:13b, largest available
        }
    }

    /// Get token budgets based on complexity
    /// Returns (analyze_tokens, draft_tokens, refine_tokens)
    fn get_token_budgets(&self, level: ComplexityLevel) -> (usize, usize, usize) {
        match level {
            ComplexityLevel::Trivial => (128, 256, 256),
            ComplexityLevel::Simple => (256, 512, 512),
            ComplexityLevel::Moderate => (512, 768, 1024),
            ComplexityLevel::Complex => (768, 1024, 1536),
            ComplexityLevel::VeryComplex => (1024, 2048, 2048),
        }
    }

    /// Select a model for a given tier and role, respecting memory constraints
    fn select_model_for_tier(&self, tier: ModelTier, role: StepRole) -> Option<&Teacher> {
        // Build a complexity assessment that matches the tier
        let complexity = match tier {
            ModelTier::Fast => ComplexityAssessment {
                level: ComplexityLevel::Trivial,
                reasoning: String::new(),
                estimated_steps: 1,
                estimated_tokens: crate::planner::complexity::TokenEstimate {
                    input_tokens: 100,
                    output_tokens: 200,
                    total_tokens: 300,
                    confidence: 0.9,
                },
                confidence: 0.9,
                factors: crate::planner::complexity::ComplexityFactors {
                    length_score: 0.1,
                    question_score: 0.1,
                    domain_score: 0.1,
                    context_score: 0.1,
                    reasoning_score: 0.1,
                },
            },
            ModelTier::Light => ComplexityAssessment {
                level: ComplexityLevel::Simple,
                reasoning: String::new(),
                estimated_steps: 2,
                estimated_tokens: crate::planner::complexity::TokenEstimate {
                    input_tokens: 200,
                    output_tokens: 400,
                    total_tokens: 600,
                    confidence: 0.85,
                },
                confidence: 0.85,
                factors: crate::planner::complexity::ComplexityFactors {
                    length_score: 0.2,
                    question_score: 0.2,
                    domain_score: 0.2,
                    context_score: 0.2,
                    reasoning_score: 0.2,
                },
            },
            ModelTier::Balanced => ComplexityAssessment {
                level: ComplexityLevel::Moderate,
                reasoning: String::new(),
                estimated_steps: 3,
                estimated_tokens: crate::planner::complexity::TokenEstimate {
                    input_tokens: 400,
                    output_tokens: 800,
                    total_tokens: 1200,
                    confidence: 0.8,
                },
                confidence: 0.8,
                factors: crate::planner::complexity::ComplexityFactors {
                    length_score: 0.4,
                    question_score: 0.4,
                    domain_score: 0.4,
                    context_score: 0.4,
                    reasoning_score: 0.4,
                },
            },
            ModelTier::Strong => ComplexityAssessment {
                level: ComplexityLevel::Complex,
                reasoning: String::new(),
                estimated_steps: 3,
                estimated_tokens: crate::planner::complexity::TokenEstimate {
                    input_tokens: 600,
                    output_tokens: 1200,
                    total_tokens: 1800,
                    confidence: 0.75,
                },
                confidence: 0.75,
                factors: crate::planner::complexity::ComplexityFactors {
                    length_score: 0.6,
                    question_score: 0.6,
                    domain_score: 0.6,
                    context_score: 0.6,
                    reasoning_score: 0.6,
                },
            },
            ModelTier::Maximum => ComplexityAssessment {
                level: ComplexityLevel::VeryComplex,
                reasoning: String::new(),
                estimated_steps: 3,
                estimated_tokens: crate::planner::complexity::TokenEstimate {
                    input_tokens: 1000,
                    output_tokens: 2000,
                    total_tokens: 3000,
                    confidence: 0.7,
                },
                confidence: 0.7,
                factors: crate::planner::complexity::ComplexityFactors {
                    length_score: 0.8,
                    question_score: 0.8,
                    domain_score: 0.8,
                    context_score: 0.8,
                    reasoning_score: 0.8,
                },
            },
        };

        // Select based on role
        match role {
            StepRole::Reasoning => self.teacher_selector.select_for_reasoning(&complexity),
            StepRole::Generation => self.teacher_selector.select_for_generation(&complexity),
            StepRole::Critique => self.teacher_selector.select_for_critique(&complexity),
            StepRole::Coding => self.teacher_selector.select_for_coding(&complexity),
        }
    }

    // ============ LEGACY METHODS (kept for backward compatibility) ============

    /// Single-step chain for trivial tasks (LEGACY - now uses standard chain)
    #[deprecated(note = "Use create_chain() which uses standard 3-step chain with model strength scaling")]
    fn chain_for_trivial(&self, task_type: TaskType) -> ThoughtChain {
        self.create_standard_chain(
            &ComplexityAssessment {
                level: ComplexityLevel::Trivial,
                reasoning: String::new(),
                estimated_steps: 1,
                estimated_tokens: crate::planner::complexity::TokenEstimate {
                    input_tokens: 50,
                    output_tokens: 100,
                    total_tokens: 150,
                    confidence: 0.9,
                },
                confidence: 0.9,
                factors: crate::planner::complexity::ComplexityFactors {
                    length_score: 0.1,
                    question_score: 0.1,
                    domain_score: 0.1,
                    context_score: 0.1,
                    reasoning_score: 0.1,
                },
            },
            task_type,
        )
    }

    /// Two-step chain for simple tasks (LEGACY - now uses standard chain)
    #[deprecated(note = "Use create_chain() which uses standard 3-step chain with model strength scaling")]
    fn chain_for_simple(&self, task_type: TaskType) -> ThoughtChain {
        self.create_standard_chain(
            &ComplexityAssessment {
                level: ComplexityLevel::Simple,
                reasoning: String::new(),
                estimated_steps: 2,
                estimated_tokens: crate::planner::complexity::TokenEstimate {
                    input_tokens: 100,
                    output_tokens: 200,
                    total_tokens: 300,
                    confidence: 0.85,
                },
                confidence: 0.85,
                factors: crate::planner::complexity::ComplexityFactors {
                    length_score: 0.2,
                    question_score: 0.2,
                    domain_score: 0.2,
                    context_score: 0.2,
                    reasoning_score: 0.2,
                },
            },
            task_type,
        )
    }

    /// Three-step chain for moderate tasks (LEGACY - now uses standard chain)
    #[deprecated(note = "Use create_chain() which uses standard 3-step chain with model strength scaling")]
    fn chain_for_moderate(&self, task_type: TaskType) -> ThoughtChain {
        self.create_standard_chain(
            &ComplexityAssessment {
                level: ComplexityLevel::Moderate,
                reasoning: String::new(),
                estimated_steps: 3,
                estimated_tokens: crate::planner::complexity::TokenEstimate {
                    input_tokens: 200,
                    output_tokens: 400,
                    total_tokens: 600,
                    confidence: 0.8,
                },
                confidence: 0.8,
                factors: crate::planner::complexity::ComplexityFactors {
                    length_score: 0.4,
                    question_score: 0.4,
                    domain_score: 0.4,
                    context_score: 0.4,
                    reasoning_score: 0.4,
                },
            },
            task_type,
        )
    }

    /// Five-step chain for complex tasks (LEGACY - now uses standard chain)
    #[deprecated(note = "Use create_chain() which uses standard 3-step chain with model strength scaling")]
    fn chain_for_complex(
        &self,
        task_type: TaskType,
        complexity: &ComplexityAssessment,
    ) -> ThoughtChain {
        self.create_standard_chain(complexity, task_type)
    }

    /// Full chain for very complex tasks (LEGACY - now uses standard chain)
    #[deprecated(note = "Use create_chain() which uses standard 3-step chain with model strength scaling")]
    fn chain_for_very_complex(
        &self,
        task_type: TaskType,
        complexity: &ComplexityAssessment,
    ) -> ThoughtChain {
        self.create_standard_chain(complexity, task_type)
    }

    /// Execute a chain of thought using the provided LLM caller
    ///
    /// # Arguments
    /// * `chain` - The thought chain to execute
    /// * `initial_input` - The original user input/request
    /// * `initial_context` - Optional initial context (e.g., conversation history)
    /// * `llm_caller` - Async function that calls the LLM: (prompt, model_name, max_tokens, temperature) -> Result<String>
    ///
    /// # Returns
    /// The final `ChainResult` containing all step results and the final output
    pub async fn execute_chain<F, Fut>(
        &self,
        chain: &ThoughtChain,
        initial_input: &str,
        initial_context: Option<&str>,
        llm_caller: F,
    ) -> crate::Result<ChainResult>
    where
        F: Fn(String, Option<String>, usize, f32) -> Fut,
        Fut: std::future::Future<Output = crate::Result<(String, usize)>>,
    {
        use std::time::Instant;
        use tracing::{debug, info};

        let start_time = Instant::now();
        let mut step_results: Vec<StepResult> = Vec::with_capacity(chain.steps.len());
        let mut step_outputs: Vec<String> = vec![String::new(); chain.steps.len()];
        let mut completed: Vec<usize> = Vec::new();

        info!(
            "Starting chain-of-thought execution: {} steps, complexity: {}",
            chain.len(),
            chain.complexity_level
        );

        // Execute steps in dependency order
        while completed.len() < chain.steps.len() {
            let runnable = chain.get_parallel_steps(&completed);

            if runnable.is_empty() {
                // No more steps can run - check if we're done or stuck
                if completed.len() < chain.steps.len() {
                    return Err(crate::ZoeyError::Other(
                        "Chain execution stuck: circular dependency or missing step".to_string(),
                    ));
                }
                break;
            }

            // Execute runnable steps (could be parallelized in the future)
            for step_idx in runnable {
                let step = &chain.steps[step_idx];
                let step_start = Instant::now();

                debug!(
                    "Executing step {}: {} ({})",
                    step_idx, step.step_type, step.id
                );

                // Build the prompt by substituting placeholders
                let prompt = self.build_step_prompt(
                    step,
                    initial_input,
                    initial_context,
                    &step_outputs,
                );

                // Resolve the model to use for this step
                let model_name = self.resolve_step_model(step);

                // Call the LLM
                let result = llm_caller(
                    prompt,
                    model_name.clone(),
                    step.max_tokens,
                    step.temperature,
                )
                .await;

                match result {
                    Ok((output, tokens_used)) => {
                        let execution_time = step_start.elapsed().as_millis() as u64;

                        info!(
                            "Step {} ({}) completed in {}ms, {} tokens",
                            step_idx, step.step_type, execution_time, tokens_used
                        );

                        step_outputs[step_idx] = output.clone();

                        step_results.push(StepResult {
                            step_index: step_idx,
                            step_type: step.step_type,
                            model_used: model_name.unwrap_or_else(|| "default".to_string()),
                            output,
                            tokens_used,
                            execution_time_ms: execution_time,
                            success: true,
                            error: None,
                        });
                    }
                    Err(e) => {
                        let execution_time = step_start.elapsed().as_millis() as u64;

                        tracing::warn!(
                            "Step {} ({}) failed after {}ms: {}",
                            step_idx, step.step_type, execution_time, e
                        );

                        step_results.push(StepResult {
                            step_index: step_idx,
                            step_type: step.step_type,
                            model_used: model_name.unwrap_or_else(|| "default".to_string()),
                            output: String::new(),
                            tokens_used: 0,
                            execution_time_ms: execution_time,
                            success: false,
                            error: Some(e.to_string()),
                        });

                        // For now, fail the whole chain on any step failure
                        // Could be made configurable to continue on non-critical steps
                        let total_tokens: usize = step_results.iter().map(|r| r.tokens_used).sum();
                        return Ok(ChainResult {
                            chain_id: chain.id,
                            step_results,
                            final_output: String::new(),
                            total_tokens_used: total_tokens,
                            total_execution_time_ms: start_time.elapsed().as_millis() as u64,
                            success: false,
                        });
                    }
                }

                completed.push(step_idx);
            }
        }

        // Combine outputs from steps marked as include_in_output
        let final_output = chain
            .steps
            .iter()
            .enumerate()
            .filter(|(_, step)| step.include_in_output)
            .map(|(idx, _)| step_outputs[idx].clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        let total_tokens: usize = step_results.iter().map(|r| r.tokens_used).sum();
        let total_time = start_time.elapsed().as_millis() as u64;

        info!(
            "Chain-of-thought completed: {} steps, {} tokens, {}ms",
            step_results.len(),
            total_tokens,
            total_time
        );

        Ok(ChainResult {
            chain_id: chain.id,
            step_results,
            final_output,
            total_tokens_used: total_tokens,
            total_execution_time_ms: total_time,
            success: true,
        })
    }

    /// Build the prompt for a step by substituting placeholders
    fn build_step_prompt(
        &self,
        step: &ThoughtStep,
        initial_input: &str,
        initial_context: Option<&str>,
        step_outputs: &[String],
    ) -> String {
        let mut prompt = step.prompt_template.clone();

        // Substitute {input} - the original user request
        prompt = prompt.replace("{input}", initial_input);

        // Substitute {context} - initial context or output from specific dependency
        let context = if step.depends_on.len() > 1 {
            // If multiple dependencies, use the first one's output as context
            // (the second one goes to previous_output)
            step.depends_on
                .first()
                .and_then(|&idx| step_outputs.get(idx))
                .map(|s| s.as_str())
                .unwrap_or("")
        } else {
            initial_context.unwrap_or("")
        };
        prompt = prompt.replace("{context}", context);

        // Substitute {previous_output} - output from the most recent dependency
        let previous_output = step
            .depends_on
            .last()
            .and_then(|&idx| step_outputs.get(idx))
            .map(|s| s.as_str())
            .unwrap_or("");
        prompt = prompt.replace("{previous_output}", previous_output);

        prompt
    }

    /// Get the teacher selector
    pub fn get_teacher_selector(&self) -> &Arc<TeacherSelector> {
        &self.teacher_selector
    }

    /// Resolve a step's model - returns the model name to use
    pub fn resolve_step_model(&self, step: &ThoughtStep) -> Option<String> {
        // If step has explicit model name, use it
        if let Some(ref model_name) = step.model_name {
            return Some(model_name.clone());
        }

        // If step has teacher ID, get teacher's model
        if let Some(teacher_id) = step.teacher_id {
            if let Some(teacher) = self.teacher_selector.get_teacher(&teacher_id) {
                return Some(teacher.model_name.clone());
            }
        }

        // Return None to indicate default should be used
        None
    }
}

/// Builder for creating custom thought chains
pub struct ThoughtChainBuilder {
    chain: ThoughtChain,
}

impl ThoughtChainBuilder {
    /// Create a new builder
    pub fn new(task_type: TaskType, complexity_level: ComplexityLevel) -> Self {
        Self {
            chain: ThoughtChain::new(task_type, complexity_level),
        }
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.chain.description = desc.into();
        self
    }

    /// Add a step
    pub fn step(mut self, step: ThoughtStep) -> Self {
        self.chain.add_step(step);
        self
    }

    /// Build the chain
    pub fn build(self) -> ThoughtChain {
        self.chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_complexity(level: ComplexityLevel) -> ComplexityAssessment {
        ComplexityAssessment {
            level,
            reasoning: "Test".to_string(),
            estimated_steps: 1,
            estimated_tokens: crate::planner::complexity::TokenEstimate {
                input_tokens: 100,
                output_tokens: 100,
                total_tokens: 200,
                confidence: 0.8,
            },
            confidence: 0.8,
            factors: crate::planner::complexity::ComplexityFactors {
                length_score: 0.3,
                question_score: 0.3,
                domain_score: 0.3,
                context_score: 0.3,
                reasoning_score: 0.3,
            },
        }
    }

    #[test]
    fn test_trivial_chain() {
        let selector = Arc::new(TeacherSelector::with_default_teachers());
        let orchestrator = ChainOfThoughtOrchestrator::new(selector);

        let complexity = create_test_complexity(ComplexityLevel::Trivial);
        let chain = orchestrator.create_chain(&complexity, TaskType::Chat);

        assert_eq!(chain.len(), 1);
        assert_eq!(chain.steps[0].step_type, ThoughtStepType::Draft);
    }

    #[test]
    fn test_simple_chain() {
        let selector = Arc::new(TeacherSelector::with_default_teachers());
        let orchestrator = ChainOfThoughtOrchestrator::new(selector);

        let complexity = create_test_complexity(ComplexityLevel::Simple);
        let chain = orchestrator.create_chain(&complexity, TaskType::QuestionAnswering);

        assert_eq!(chain.len(), 2);
        assert_eq!(chain.steps[0].step_type, ThoughtStepType::Analyze);
        assert_eq!(chain.steps[1].step_type, ThoughtStepType::Draft);
    }

    #[test]
    fn test_complex_chain() {
        let selector = Arc::new(TeacherSelector::with_default_teachers());
        let orchestrator = ChainOfThoughtOrchestrator::new(selector);

        let complexity = create_test_complexity(ComplexityLevel::Complex);
        let chain = orchestrator.create_chain(&complexity, TaskType::Reasoning);

        assert_eq!(chain.len(), 5);
        assert_eq!(chain.steps[0].step_type, ThoughtStepType::Analyze);
        assert_eq!(chain.steps[1].step_type, ThoughtStepType::Reason);
        assert_eq!(chain.steps[2].step_type, ThoughtStepType::Draft);
        assert_eq!(chain.steps[3].step_type, ThoughtStepType::Critique);
        assert_eq!(chain.steps[4].step_type, ThoughtStepType::Refine);
    }

    #[test]
    fn test_very_complex_chain() {
        let selector = Arc::new(TeacherSelector::with_default_teachers());
        let orchestrator = ChainOfThoughtOrchestrator::new(selector);

        let complexity = create_test_complexity(ComplexityLevel::VeryComplex);
        let chain = orchestrator.create_chain(&complexity, TaskType::CodeGeneration);

        assert_eq!(chain.len(), 6);
        // Should include research step
        assert!(chain
            .steps
            .iter()
            .any(|s| s.step_type == ThoughtStepType::Research));
    }

    #[test]
    fn test_chain_dependencies() {
        let selector = Arc::new(TeacherSelector::with_default_teachers());
        let orchestrator = ChainOfThoughtOrchestrator::new(selector);

        let complexity = create_test_complexity(ComplexityLevel::Complex);
        let chain = orchestrator.create_chain(&complexity, TaskType::Chat);

        // Verify dependency structure
        assert!(chain.steps[0].depends_on.is_empty()); // Analyze has no deps
        assert!(chain.steps[1].depends_on.contains(&0)); // Reason depends on Analyze
        assert!(chain.steps[2].depends_on.contains(&1)); // Draft depends on Reason
        assert!(chain.steps[3].depends_on.contains(&2)); // Critique depends on Draft
        assert!(chain.steps[4].depends_on.contains(&2)); // Refine depends on Draft
        assert!(chain.steps[4].depends_on.contains(&3)); // Refine depends on Critique
    }

    #[test]
    fn test_parallel_steps() {
        let mut chain = ThoughtChain::new(TaskType::Chat, ComplexityLevel::Complex);

        // Create steps with parallel structure
        chain.add_step(ThoughtStep::new(ThoughtStepType::Analyze, "A")); // 0
        chain.add_step(
            ThoughtStep::new(ThoughtStepType::Research, "B").depends_on(vec![0]),
        ); // 1
        chain.add_step(ThoughtStep::new(ThoughtStepType::Reason, "C").depends_on(vec![0])); // 2
        chain.add_step(
            ThoughtStep::new(ThoughtStepType::Draft, "D").depends_on(vec![1, 2]),
        ); // 3

        // Initially only step 0 can run
        let parallel = chain.get_parallel_steps(&[]);
        assert_eq!(parallel, vec![0]);

        // After step 0, steps 1 and 2 can run in parallel
        let parallel = chain.get_parallel_steps(&[0]);
        assert_eq!(parallel, vec![1, 2]);

        // After 0, 1, 2 - step 3 can run
        let parallel = chain.get_parallel_steps(&[0, 1, 2]);
        assert_eq!(parallel, vec![3]);
    }

    #[test]
    fn test_chain_builder() {
        let chain = ThoughtChainBuilder::new(TaskType::Chat, ComplexityLevel::Simple)
            .description("Test chain")
            .step(ThoughtStep::new(ThoughtStepType::Analyze, "Analyze"))
            .step(
                ThoughtStep::new(ThoughtStepType::Draft, "Draft")
                    .depends_on(vec![0])
                    .include_in_output(),
            )
            .build();

        assert_eq!(chain.len(), 2);
        assert_eq!(chain.description, "Test chain");
        assert!(chain.steps[1].include_in_output);
    }
}
