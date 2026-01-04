/*!
# Reasoning Chain Module

Tracks the logical steps in an AI agent's reasoning process.
*/

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// A complete chain of reasoning steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningChain {
    /// Unique identifier
    pub id: Uuid,

    /// Name/description of what's being reasoned about
    pub subject: String,

    /// The sequence of reasoning steps
    pub steps: Vec<ReasoningStep>,

    /// When the reasoning started
    pub started_at: DateTime<Utc>,

    /// When the reasoning completed
    pub completed_at: Option<DateTime<Utc>>,

    /// Overall complexity score
    pub complexity: f64,
}

/// Type of reasoning step
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningStepType {
    /// Initial observation or data gathering
    Observation,

    /// Hypothesis formation
    Hypothesis,

    /// Testing or validation
    Validation,

    /// Ruling out alternatives
    Elimination,

    /// Drawing conclusions
    Conclusion,

    /// Uncertainty acknowledgment
    Uncertainty,

    /// Reference to external knowledge
    KnowledgeRetrieval,

    /// Logical inference
    Inference,

    /// Assumption made
    Assumption,
}

/// A single step in the reasoning process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Unique identifier
    pub id: Uuid,

    /// Type of reasoning step
    pub step_type: ReasoningStepType,

    /// Description of this step
    pub description: String,

    /// Confidence in this step (0.0-1.0)
    pub confidence: f64,

    /// Dependencies on previous steps
    pub depends_on: Vec<Uuid>,

    /// Supporting evidence
    pub evidence: Vec<String>,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Optional metadata
    pub metadata: serde_json::Value,
}

impl ReasoningChain {
    /// Create a new reasoning chain
    pub fn new(subject: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            subject,
            steps: Vec::new(),
            started_at: Utc::now(),
            completed_at: None,
            complexity: 0.0,
        }
    }

    /// Add a reasoning step
    pub fn add_step(
        &mut self,
        step_type: ReasoningStepType,
        description: impl Into<String>,
        confidence: f64,
    ) -> Uuid {
        let step = ReasoningStep {
            id: Uuid::new_v4(),
            step_type,
            description: description.into(),
            confidence,
            depends_on: Vec::new(),
            evidence: Vec::new(),
            timestamp: Utc::now(),
            metadata: serde_json::Value::Null,
        };

        let id = step.id;
        self.steps.push(step);
        self.update_complexity();
        id
    }

    /// Add a step with dependencies
    pub fn add_step_with_deps(
        &mut self,
        step_type: ReasoningStepType,
        description: impl Into<String>,
        confidence: f64,
        depends_on: Vec<Uuid>,
    ) -> Uuid {
        let step = ReasoningStep {
            id: Uuid::new_v4(),
            step_type,
            description: description.into(),
            confidence,
            depends_on,
            evidence: Vec::new(),
            timestamp: Utc::now(),
            metadata: serde_json::Value::Null,
        };

        let id = step.id;
        self.steps.push(step);
        self.update_complexity();
        id
    }

    /// Add evidence to a specific step
    pub fn add_evidence(&mut self, step_id: Uuid, evidence: impl Into<String>) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.id == step_id) {
            step.evidence.push(evidence.into());
        }
    }

    /// Mark the reasoning as complete
    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
    }

    /// Calculate overall confidence
    pub fn overall_confidence(&self) -> f64 {
        if self.steps.is_empty() {
            return 0.0;
        }

        // Weighted average, giving more weight to conclusions
        let mut total_weight = 0.0;
        let mut weighted_sum = 0.0;

        for step in &self.steps {
            let weight = match step.step_type {
                ReasoningStepType::Conclusion => 2.0,
                ReasoningStepType::Validation => 1.5,
                ReasoningStepType::Inference => 1.5,
                ReasoningStepType::Uncertainty => 0.5,
                _ => 1.0,
            };

            weighted_sum += step.confidence * weight;
            total_weight += weight;
        }

        if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            0.0
        }
    }

    /// Update complexity score based on chain structure
    fn update_complexity(&mut self) {
        // Complexity factors:
        // 1. Number of steps
        // 2. Number of dependencies
        // 3. Number of uncertainty acknowledgments
        // 4. Depth of reasoning tree

        let step_complexity = self.steps.len() as f64 * 0.1;

        let dep_complexity: f64 = self
            .steps
            .iter()
            .map(|s| s.depends_on.len() as f64 * 0.05)
            .sum();

        let uncertainty_count = self
            .steps
            .iter()
            .filter(|s| s.step_type == ReasoningStepType::Uncertainty)
            .count() as f64
            * 0.2;

        self.complexity = step_complexity + dep_complexity + uncertainty_count;
    }

    /// Get the reasoning tree depth
    pub fn depth(&self) -> usize {
        if self.steps.is_empty() {
            return 0;
        }

        // Calculate maximum depth via dependencies
        let mut max_depth = 1;
        for step in &self.steps {
            let step_depth = self.calculate_step_depth(&step.id);
            max_depth = max_depth.max(step_depth);
        }

        max_depth
    }

    /// Calculate depth of a specific step
    fn calculate_step_depth(&self, step_id: &Uuid) -> usize {
        let step = match self.steps.iter().find(|s| &s.id == step_id) {
            Some(s) => s,
            None => return 0,
        };

        if step.depends_on.is_empty() {
            return 1;
        }

        let max_parent_depth = step
            .depends_on
            .iter()
            .map(|dep_id| self.calculate_step_depth(dep_id))
            .max()
            .unwrap_or(0);

        max_parent_depth + 1
    }

    /// Get steps by type
    pub fn steps_by_type(&self, step_type: ReasoningStepType) -> Vec<&ReasoningStep> {
        self.steps
            .iter()
            .filter(|s| s.step_type == step_type)
            .collect()
    }
}

impl fmt::Display for ReasoningChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Reasoning about: {}", self.subject)?;
        writeln!(f, "Steps: {}", self.steps.len())?;
        writeln!(f, "Complexity: {:.2}", self.complexity)?;
        writeln!(
            f,
            "Overall confidence: {:.1}%\n",
            self.overall_confidence() * 100.0
        )?;

        for (idx, step) in self.steps.iter().enumerate() {
            writeln!(
                f,
                "{}. [{:?}] {} (confidence: {:.1}%)",
                idx + 1,
                step.step_type,
                step.description,
                step.confidence * 100.0
            )?;

            if !step.evidence.is_empty() {
                for evidence in &step.evidence {
                    writeln!(f, "   Evidence: {}", evidence)?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reasoning_chain_creation() {
        let chain = ReasoningChain::new("test diagnosis".to_string());
        assert_eq!(chain.subject, "test diagnosis");
        assert_eq!(chain.steps.len(), 0);
    }

    #[test]
    fn test_add_steps() {
        let mut chain = ReasoningChain::new("diagnosis".to_string());

        let step1 = chain.add_step(ReasoningStepType::Observation, "Patient has fever", 1.0);

        let step2 = chain.add_step_with_deps(
            ReasoningStepType::Hypothesis,
            "Possible viral infection",
            0.7,
            vec![step1],
        );

        assert_eq!(chain.steps.len(), 2);
        assert_eq!(chain.steps[1].depends_on, vec![step1]);
    }

    #[test]
    fn test_overall_confidence() {
        let mut chain = ReasoningChain::new("test".to_string());

        chain.add_step(ReasoningStepType::Observation, "Step 1", 0.9);
        chain.add_step(ReasoningStepType::Hypothesis, "Step 2", 0.8);
        chain.add_step(ReasoningStepType::Conclusion, "Step 3", 0.85);

        let confidence = chain.overall_confidence();
        assert!(confidence > 0.8 && confidence < 0.9);
    }

    #[test]
    fn test_depth_calculation() {
        let mut chain = ReasoningChain::new("test".to_string());

        let step1 = chain.add_step(ReasoningStepType::Observation, "Base", 1.0);
        let step2 =
            chain.add_step_with_deps(ReasoningStepType::Hypothesis, "Level 1", 0.8, vec![step1]);
        chain.add_step_with_deps(ReasoningStepType::Conclusion, "Level 2", 0.7, vec![step2]);

        assert_eq!(chain.depth(), 3);
    }
}
