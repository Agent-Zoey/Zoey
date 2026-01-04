/*!
# Counterfactual Reasoning Module

Enables "what-if" analysis to explain how different conditions would change outcomes.
*/

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A counterfactual scenario exploring alternative conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterfactualScenario {
    /// Unique identifier
    pub id: Uuid,

    /// The hypothetical condition ("If X were different...")
    pub condition: String,

    /// The predicted outcome ("...then Y would happen")
    pub outcome: String,

    /// Confidence in this counterfactual (0.0-1.0)
    pub confidence: f64,

    /// How different from actual (0.0-1.0, where 1.0 is completely different)
    pub divergence: f64,

    /// Type of counterfactual
    pub scenario_type: ScenarioType,

    /// Supporting reasoning
    pub reasoning: Vec<String>,
}

/// Type of counterfactual scenario
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenarioType {
    /// Changing input data
    InputVariation,

    /// Changing parameters
    ParameterVariation,

    /// Changing context
    ContextVariation,

    /// Changing available knowledge
    KnowledgeVariation,

    /// Removing a constraint
    ConstraintRemoval,

    /// Adding a constraint
    ConstraintAddition,

    /// Time-based variation (earlier/later)
    TemporalVariation,
}

impl CounterfactualScenario {
    /// Create a new counterfactual scenario
    pub fn new(condition: impl Into<String>, outcome: impl Into<String>, confidence: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            condition: condition.into(),
            outcome: outcome.into(),
            confidence: confidence.clamp(0.0, 1.0),
            divergence: 0.5, // Default moderate divergence
            scenario_type: ScenarioType::InputVariation,
            reasoning: Vec::new(),
        }
    }

    /// Set scenario type
    pub fn with_type(mut self, scenario_type: ScenarioType) -> Self {
        self.scenario_type = scenario_type;
        self
    }

    /// Set divergence
    pub fn with_divergence(mut self, divergence: f64) -> Self {
        self.divergence = divergence.clamp(0.0, 1.0);
        self
    }

    /// Add reasoning step
    pub fn add_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.reasoning.push(reasoning.into());
        self
    }

    /// Format as human-readable text
    pub fn to_text(&self) -> String {
        let mut text = format!("**If {}**, then {}\n", self.condition, self.outcome);
        text.push_str(&format!("Confidence: {:.1}%\n", self.confidence * 100.0));

        if !self.reasoning.is_empty() {
            text.push_str("Reasoning:\n");
            for (idx, reason) in self.reasoning.iter().enumerate() {
                text.push_str(&format!("  {}. {}\n", idx + 1, reason));
            }
        }

        text
    }
}

/// Generator for counterfactual scenarios
pub struct CounterfactualGenerator {
    scenarios: Vec<CounterfactualScenario>,
}

impl CounterfactualGenerator {
    /// Create a new generator
    pub fn new() -> Self {
        Self {
            scenarios: Vec::new(),
        }
    }

    /// Add a scenario
    pub fn add(&mut self, scenario: CounterfactualScenario) {
        self.scenarios.push(scenario);
    }

    /// Generate simple input variation
    pub fn generate_input_variation(
        &mut self,
        condition: impl Into<String>,
        outcome: impl Into<String>,
        confidence: f64,
    ) {
        let scenario = CounterfactualScenario::new(condition, outcome, confidence)
            .with_type(ScenarioType::InputVariation);
        self.scenarios.push(scenario);
    }

    /// Generate parameter variation
    pub fn generate_parameter_variation(
        &mut self,
        parameter: impl Into<String>,
        value: impl Into<String>,
        outcome: impl Into<String>,
        confidence: f64,
    ) {
        let condition = format!("{} was {}", parameter.into(), value.into());
        let scenario = CounterfactualScenario::new(condition, outcome, confidence)
            .with_type(ScenarioType::ParameterVariation);
        self.scenarios.push(scenario);
    }

    /// Generate temporal variation
    pub fn generate_temporal_variation(
        &mut self,
        time_shift: impl Into<String>,
        outcome: impl Into<String>,
        confidence: f64,
    ) {
        let condition = format!("this had happened {}", time_shift.into());
        let scenario = CounterfactualScenario::new(condition, outcome, confidence)
            .with_type(ScenarioType::TemporalVariation);
        self.scenarios.push(scenario);
    }

    /// Generate constraint variation
    pub fn generate_constraint_removal(
        &mut self,
        constraint: impl Into<String>,
        outcome: impl Into<String>,
        confidence: f64,
    ) {
        let condition = format!("the constraint '{}' was removed", constraint.into());
        let scenario = CounterfactualScenario::new(condition, outcome, confidence)
            .with_type(ScenarioType::ConstraintRemoval);
        self.scenarios.push(scenario);
    }

    /// Get all scenarios
    pub fn scenarios(&self) -> &[CounterfactualScenario] {
        &self.scenarios
    }

    /// Get scenarios by type
    pub fn scenarios_by_type(&self, scenario_type: ScenarioType) -> Vec<&CounterfactualScenario> {
        self.scenarios
            .iter()
            .filter(|s| s.scenario_type == scenario_type)
            .collect()
    }

    /// Get most impactful scenarios (high divergence + high confidence)
    pub fn most_impactful(&self, n: usize) -> Vec<&CounterfactualScenario> {
        let mut scenarios: Vec<&CounterfactualScenario> = self.scenarios.iter().collect();

        // Sort by impact score (divergence * confidence)
        scenarios.sort_by(|a, b| {
            let score_a = a.divergence * a.confidence;
            let score_b = b.divergence * b.confidence;
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        scenarios.into_iter().take(n).collect()
    }
}

impl Default for CounterfactualGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Counterfactual reasoning engine
pub struct CounterfactualReasoning {
    generator: CounterfactualGenerator,
}

impl CounterfactualReasoning {
    /// Create a new counterfactual reasoning engine
    pub fn new() -> Self {
        Self {
            generator: CounterfactualGenerator::new(),
        }
    }

    /// Analyze a decision and generate counterfactuals
    pub fn analyze(
        &mut self,
        _decision_context: &serde_json::Value,
    ) -> anyhow::Result<Vec<CounterfactualScenario>> {
        // In a real implementation, this would analyze the decision context
        // and automatically generate relevant counterfactual scenarios

        // For now, return the manually added scenarios
        Ok(self.generator.scenarios().to_vec())
    }

    /// Add a manual counterfactual
    pub fn add_scenario(&mut self, scenario: CounterfactualScenario) {
        self.generator.add(scenario);
    }

    /// Get most impactful counterfactuals
    pub fn most_impactful(&self, n: usize) -> Vec<&CounterfactualScenario> {
        self.generator.most_impactful(n)
    }
}

impl Default for CounterfactualReasoning {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counterfactual_creation() {
        let scenario = CounterfactualScenario::new(
            "patient had received treatment 24 hours earlier",
            "recovery time would have been reduced by 30%",
            0.8,
        )
        .with_type(ScenarioType::TemporalVariation)
        .add_reasoning("Earlier treatment reduces complications");

        assert_eq!(scenario.confidence, 0.8);
        assert_eq!(scenario.scenario_type, ScenarioType::TemporalVariation);
        assert_eq!(scenario.reasoning.len(), 1);
    }

    #[test]
    fn test_generator() {
        let mut gen = CounterfactualGenerator::new();

        gen.generate_input_variation(
            "symptom severity was lower",
            "different diagnosis would be likely",
            0.7,
        );

        gen.generate_parameter_variation(
            "temperature threshold",
            "38.5°C instead of 39°C",
            "alert would not have triggered",
            0.9,
        );

        assert_eq!(gen.scenarios().len(), 2);
    }

    #[test]
    fn test_most_impactful() {
        let mut gen = CounterfactualGenerator::new();

        gen.add(
            CounterfactualScenario::new("A", "X", 0.8).with_divergence(0.9), // High impact
        );

        gen.add(
            CounterfactualScenario::new("B", "Y", 0.5).with_divergence(0.3), // Low impact
        );

        let impactful = gen.most_impactful(1);
        assert_eq!(impactful.len(), 1);
        assert_eq!(impactful[0].condition, "A");
    }
}
