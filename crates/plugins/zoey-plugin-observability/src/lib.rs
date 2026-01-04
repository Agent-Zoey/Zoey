/*!
# Explainability Plugin for LauraAI

This plugin provides comprehensive explainability features for AI agents:

- **Reasoning Chains**: Track and display logical reasoning steps
- **Source Attribution**: Cite which documents/memories influenced decisions
- **Confidence Scores**: Quantify uncertainty with bounds
- **Alternative Explanations**: Show other considered options
- **Counterfactual Reasoning**: Explain "what if" scenarios
- **Tamper-Evident Logs**: Compliance-ready audit trails

## Example Usage

```rust
use zoey_plugin_explainability::{
    ExplainabilityEngine, ReasoningChain, ReasoningStepType,
    ConfidenceScore, ExplainabilityContext
};

let mut engine = ExplainabilityEngine::new();

// Track reasoning
let mut chain = ReasoningChain::new("patient_diagnosis".to_string());
chain.add_step(ReasoningStepType::Observation, "Analyzed symptoms: fever, cough, fatigue", 1.0);
chain.add_step(ReasoningStepType::KnowledgeRetrieval, "Checked against knowledge base: 5 matching conditions", 0.95);
chain.add_step(ReasoningStepType::Elimination, "Ruled out: common cold (no nasal congestion)", 0.9);
chain.add_step(ReasoningStepType::Hypothesis, "Primary hypothesis: Influenza", 0.85);

// Create explainability context
let mut context = ExplainabilityContext::new(chain);
context.set_confidence(ConfidenceScore::new(0.85));

// Generate explanation
let explanation = context.to_explanation();
println!("{}", explanation);

// Record in audit log
engine.record(&context).unwrap();
```
*/

pub mod audit_log;
pub mod confidence;
pub mod counterfactual;
pub mod plugin;
pub mod reasoning_chain;
pub mod source_attribution;

pub use audit_log::{AuditEntry, AuditLogManager, TamperEvidentLog};
pub use confidence::{ConfidenceLevel, ConfidenceScore, UncertaintyBounds};
pub use counterfactual::{CounterfactualReasoning, CounterfactualScenario};
pub use plugin::ExplainabilityPlugin;
pub use reasoning_chain::{ReasoningChain, ReasoningStep, ReasoningStepType};
pub use source_attribution::{AttributionScore, Source, SourceAttribution};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Complete explainability context for a decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainabilityContext {
    /// Unique identifier
    pub id: Uuid,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// The reasoning chain
    pub reasoning_chain: ReasoningChain,

    /// Source attributions
    pub sources: Vec<SourceAttribution>,

    /// Confidence score
    pub confidence: ConfidenceScore,

    /// Alternative explanations considered
    pub alternatives: Vec<AlternativeExplanation>,

    /// Counterfactual scenarios
    pub counterfactuals: Vec<CounterfactualScenario>,

    /// Audit log entry
    pub audit_entry: Option<AuditEntry>,
}

/// An alternative explanation that was considered
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeExplanation {
    /// Description of the alternative
    pub description: String,

    /// Why it was considered
    pub rationale: String,

    /// Why it was not chosen
    pub rejection_reason: String,

    /// Confidence score if this path was taken
    pub hypothetical_confidence: f64,
}

impl ExplainabilityContext {
    /// Create a new explainability context
    pub fn new(reasoning_chain: ReasoningChain) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            reasoning_chain,
            sources: Vec::new(),
            confidence: ConfidenceScore::default(),
            alternatives: Vec::new(),
            counterfactuals: Vec::new(),
            audit_entry: None,
        }
    }

    /// Add a source attribution
    pub fn add_source(&mut self, source: SourceAttribution) {
        self.sources.push(source);
    }

    /// Set confidence score
    pub fn set_confidence(&mut self, confidence: ConfidenceScore) {
        self.confidence = confidence;
    }

    /// Add an alternative explanation
    pub fn add_alternative(&mut self, alternative: AlternativeExplanation) {
        self.alternatives.push(alternative);
    }

    /// Add a counterfactual scenario
    pub fn add_counterfactual(&mut self, counterfactual: CounterfactualScenario) {
        self.counterfactuals.push(counterfactual);
    }

    /// Generate a human-readable explanation
    pub fn to_explanation(&self) -> String {
        let mut explanation = String::new();

        // Reasoning chain
        explanation.push_str("## Reasoning Process\n\n");
        explanation.push_str(&self.reasoning_chain.to_string());
        explanation.push_str("\n\n");

        // Confidence
        explanation.push_str("## Confidence Assessment\n\n");
        explanation.push_str(&format!(
            "**Overall Confidence**: {:.1}% ({:?})\n",
            self.confidence.value * 100.0,
            self.confidence.level
        ));
        explanation.push_str(&format!(
            "**Uncertainty Range**: {:.1}% - {:.1}%\n\n",
            self.confidence.bounds.lower * 100.0,
            self.confidence.bounds.upper * 100.0
        ));

        // Sources
        if !self.sources.is_empty() {
            explanation.push_str("## Sources Referenced\n\n");
            for (idx, source) in self.sources.iter().enumerate() {
                explanation.push_str(&format!(
                    "{}. {} (relevance: {:.1}%)\n",
                    idx + 1,
                    source.source.title,
                    source.attribution_score * 100.0
                ));
            }
            explanation.push_str("\n");
        }

        // Alternatives
        if !self.alternatives.is_empty() {
            explanation.push_str("## Alternative Explanations Considered\n\n");
            for (idx, alt) in self.alternatives.iter().enumerate() {
                explanation.push_str(&format!(
                    "{}. **{}**\n   - Why considered: {}\n   - Why rejected: {}\n\n",
                    idx + 1,
                    alt.description,
                    alt.rationale,
                    alt.rejection_reason
                ));
            }
        }

        // Counterfactuals
        if !self.counterfactuals.is_empty() {
            explanation.push_str("## What-If Scenarios\n\n");
            for (idx, cf) in self.counterfactuals.iter().enumerate() {
                explanation.push_str(&format!(
                    "{}. **If {}**, then {}\n",
                    idx + 1,
                    cf.condition,
                    cf.outcome
                ));
            }
            explanation.push_str("\n");
        }

        explanation
    }
}

/// The main explainability engine
pub struct ExplainabilityEngine {
    /// Tamper-evident audit log
    audit_log: TamperEvidentLog,
}

impl ExplainabilityEngine {
    /// Create a new explainability engine
    pub fn new() -> Self {
        Self {
            audit_log: TamperEvidentLog::new(),
        }
    }

    /// Record an explainability context in the audit log
    pub fn record(&mut self, context: &ExplainabilityContext) -> anyhow::Result<()> {
        let entry = AuditEntry {
            id: context.id,
            timestamp: context.timestamp,
            context_json: serde_json::to_string(context)?,
            hash: String::new(), // Will be set by audit log
        };

        self.audit_log.append(entry)?;
        Ok(())
    }

    /// Verify audit log integrity
    pub fn verify_audit_log(&self) -> anyhow::Result<bool> {
        self.audit_log.verify()
    }

    /// Export audit log for compliance
    pub fn export_audit_log(&self) -> anyhow::Result<Vec<AuditEntry>> {
        Ok(self.audit_log.entries().to_vec())
    }
}

impl Default for ExplainabilityEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explainability_context_creation() {
        let chain = ReasoningChain::new("test".to_string());
        let context = ExplainabilityContext::new(chain);

        assert_eq!(context.sources.len(), 0);
        assert_eq!(context.alternatives.len(), 0);
    }

    #[test]
    fn test_explanation_generation() {
        let mut chain = ReasoningChain::new("diagnosis".to_string());
        chain.add_step(
            ReasoningStepType::Observation,
            "Patient presents with fever",
            1.0,
        );

        let mut context = ExplainabilityContext::new(chain);
        context.set_confidence(ConfidenceScore::new(0.85));

        let explanation = context.to_explanation();
        assert!(explanation.contains("Reasoning Process"));
        assert!(explanation.contains("Confidence Assessment"));
    }
}
