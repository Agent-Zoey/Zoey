//! Knowledge gap analysis for planning

use crate::types::*;
use crate::Result;
use serde::{Deserialize, Serialize};

/// Priority level for knowledge gaps
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Priority {
    /// Low priority - nice to know
    Low,
    /// Medium priority - helpful for better response
    Medium,
    /// High priority - critical for accurate response
    High,
    /// Critical priority - cannot proceed without
    Critical,
}

/// Strategy for resolving a knowledge gap
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResolutionStrategy {
    /// Search agent's memory
    SearchMemory,
    /// Ask user for clarification
    AskUser,
    /// Make informed assumption
    Assume,
    /// Look up from external source
    ExternalLookup,
    /// Derive from context
    DeriveFromContext,
    /// Not resolvable
    Unresolvable,
}

/// A piece of knowledge we have
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownFact {
    /// What we know
    pub fact: String,
    /// Source of the knowledge
    pub source: KnowledgeSource,
    /// Confidence in this fact (0.0 - 1.0)
    pub confidence: f32,
    /// When this was learned/updated
    pub timestamp: i64,
}

/// Source of knowledge
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeSource {
    /// From conversation memory
    Memory,
    /// From current state/context
    Context,
    /// From agent's character/settings
    Character,
    /// From previous messages
    RecentMessages,
    /// Derived/inferred
    Inferred,
}

/// A gap in our knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGap {
    /// Description of what we don't know
    pub description: String,
    /// Priority level
    pub priority: Priority,
    /// Can this be resolved?
    pub resolvable: bool,
    /// How to resolve it
    pub resolution_strategy: Option<ResolutionStrategy>,
    /// Impact on response quality if not resolved
    pub impact: String,
}

/// An assumption we're making
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assumption {
    /// What we're assuming
    pub assumption: String,
    /// Confidence in this assumption (0.0 - 1.0)
    pub confidence: f32,
    /// Risk if assumption is wrong
    pub risk_level: RiskLevel,
}

/// Risk level for assumptions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    /// Low risk if wrong
    Low,
    /// Medium risk if wrong
    Medium,
    /// High risk if wrong
    High,
    /// Critical risk if wrong
    Critical,
}

/// Complete knowledge state analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeState {
    /// Facts we know
    pub known_facts: Vec<KnownFact>,
    /// Gaps in knowledge
    pub unknown_gaps: Vec<KnowledgeGap>,
    /// Assumptions we're making
    pub assumptions: Vec<Assumption>,
    /// Overall confidence score (0.0 - 1.0)
    pub confidence_score: f32,
    /// Summary of knowledge state
    pub summary: String,
}

/// Knowledge analyzer
pub struct KnowledgeAnalyzer;

impl KnowledgeAnalyzer {
    /// Create a new knowledge analyzer
    pub fn new() -> Self {
        Self
    }

    /// Analyze knowledge state for a message
    pub async fn analyze(&self, message: &Memory, state: &State) -> Result<KnowledgeState> {
        let mut known_facts = Vec::new();
        let mut unknown_gaps = Vec::new();

        // Extract entities and concepts from message
        let entities = self.extract_entities(&message.content.text);

        // Check what we know from state
        known_facts.extend(self.extract_facts_from_state(state));

        // Check what we know from recent messages
        if let Some(recent) = state.data.get("recentMessages") {
            known_facts.extend(self.extract_facts_from_recent(recent));
        }

        // Identify knowledge gaps
        for entity in &entities {
            if !self.is_entity_known(entity, &known_facts) {
                let gap = self.create_knowledge_gap(entity, &message.content.text);
                unknown_gaps.push(gap);
            }
        }

        // Analyze contextual requirements
        let contextual_gaps = self.analyze_contextual_requirements(&message.content.text, state);
        unknown_gaps.extend(contextual_gaps);

        // Generate assumptions if needed
        let assumptions = self.generate_assumptions(&unknown_gaps, state);

        // Calculate confidence score
        let confidence_score = self.calculate_confidence(&known_facts, &unknown_gaps);

        // Generate summary
        let summary = self.generate_summary(&known_facts, &unknown_gaps, &assumptions);

        Ok(KnowledgeState {
            known_facts,
            unknown_gaps,
            assumptions,
            confidence_score,
            summary,
        })
    }

    /// Extract entities and concepts from text
    fn extract_entities(&self, text: &str) -> Vec<String> {
        let mut entities = Vec::new();

        // Simple entity extraction (could be enhanced with NER)
        let words: Vec<&str> = text.split_whitespace().collect();

        for window in words.windows(2) {
            // Capitalized words (potential proper nouns)
            if window[0]
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
            {
                entities.push(window[0].to_string());
            }

            // Two-word entities
            if window[0]
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
                && window[1]
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
            {
                entities.push(format!("{} {}", window[0], window[1]));
            }
        }

        // Technical terms and keywords
        let technical_patterns = [
            "algorithm",
            "function",
            "code",
            "system",
            "database",
            "api",
            "service",
            "module",
            "component",
            "framework",
        ];

        for pattern in &technical_patterns {
            if text.to_lowercase().contains(pattern) {
                entities.push(pattern.to_string());
            }
        }

        // Ensure common programming languages are recognized
        let lower = text.to_lowercase();
        for lang in ["rust", "python", "java", "javascript", "go", "c++", "c"].iter() {
            if lower.contains(lang) {
                let name = match *lang {
                    "javascript" => "JavaScript".to_string(),
                    "c++" => "C++".to_string(),
                    _ => {
                        let mut s = lang.to_string();
                        if let Some(first) = s.chars().next() {
                            s.replace_range(0..1, &first.to_uppercase().to_string());
                        }
                        s
                    }
                };
                entities.push(name);
            }
        }

        // Deduplicate
        entities.sort();
        entities.dedup();

        entities
    }

    /// Extract known facts from state
    fn extract_facts_from_state(&self, state: &State) -> Vec<KnownFact> {
        let mut facts = Vec::new();
        let now = chrono::Utc::now().timestamp();

        // Agent name
        if let Some(name) = state.data.get("agentName") {
            if let Some(name_str) = name.as_str() {
                facts.push(KnownFact {
                    fact: format!("Agent name is {}", name_str),
                    source: KnowledgeSource::Context,
                    confidence: 1.0,
                    timestamp: now,
                });
            }
        }

        // User name
        if let Some(user) = state.data.get("userName") {
            if let Some(user_str) = user.as_str() {
                facts.push(KnownFact {
                    fact: format!("User name is {}", user_str),
                    source: KnowledgeSource::Context,
                    confidence: 1.0,
                    timestamp: now,
                });
            }
        }

        // Current goals
        if let Some(goals) = state.data.get("goals") {
            if let Some(goals_arr) = goals.as_array() {
                for goal in goals_arr {
                    if let Some(goal_str) = goal.as_str() {
                        facts.push(KnownFact {
                            fact: format!("Current goal: {}", goal_str),
                            source: KnowledgeSource::Context,
                            confidence: 0.9,
                            timestamp: now,
                        });
                    }
                }
            }
        }

        facts
    }

    /// Extract facts from recent messages
    fn extract_facts_from_recent(&self, recent: &serde_json::Value) -> Vec<KnownFact> {
        let mut facts = Vec::new();
        let now = chrono::Utc::now().timestamp();

        if let Some(messages) = recent.as_array() {
            for msg in messages.iter().take(5) {
                if let Some(content) = msg.get("content").and_then(|c| c.get("text")) {
                    if let Some(text) = content.as_str() {
                        // Extract key information from recent messages
                        facts.push(KnownFact {
                            fact: format!(
                                "Recent context: {}",
                                text.chars().take(100).collect::<String>()
                            ),
                            source: KnowledgeSource::RecentMessages,
                            confidence: 0.8,
                            timestamp: now,
                        });
                    }
                }
            }
        }

        facts
    }

    /// Check if an entity is known
    fn is_entity_known(&self, entity: &str, known_facts: &[KnownFact]) -> bool {
        let entity_lower = entity.to_lowercase();
        known_facts
            .iter()
            .any(|fact| fact.fact.to_lowercase().contains(&entity_lower))
    }

    /// Create a knowledge gap for an unknown entity
    fn create_knowledge_gap(&self, entity: &str, context: &str) -> KnowledgeGap {
        // Determine priority based on context
        let priority = if context
            .to_lowercase()
            .contains(&format!("what is {}", entity.to_lowercase()))
            || context
                .to_lowercase()
                .contains(&format!("who is {}", entity.to_lowercase()))
        {
            Priority::Critical
        } else if context.to_lowercase().contains("explain") {
            Priority::High
        } else {
            Priority::Medium
        };

        KnowledgeGap {
            description: format!("Unknown entity: {}", entity),
            priority,
            resolvable: true,
            resolution_strategy: Some(ResolutionStrategy::SearchMemory),
            impact: format!("May affect understanding of {}", entity),
        }
    }

    /// Analyze contextual requirements
    fn analyze_contextual_requirements(&self, text: &str, state: &State) -> Vec<KnowledgeGap> {
        let mut gaps = Vec::new();
        let lower = text.to_lowercase();

        // References to "it", "this", "that" without clear antecedent
        if (lower.contains(" it ") || lower.contains("this") || lower.contains("that"))
            && !state.data.contains_key("recentMessages")
        {
            gaps.push(KnowledgeGap {
                description: "Unclear reference - missing context".to_string(),
                priority: Priority::High,
                resolvable: false,
                resolution_strategy: Some(ResolutionStrategy::AskUser),
                impact: "May misunderstand what user is referring to".to_string(),
            });
        }

        // Temporal references
        if lower.contains("previous") || lower.contains("earlier") || lower.contains("last time") {
            gaps.push(KnowledgeGap {
                description: "Reference to previous conversation or event".to_string(),
                priority: Priority::High,
                resolvable: true,
                resolution_strategy: Some(ResolutionStrategy::SearchMemory),
                impact: "Missing historical context".to_string(),
            });
        }

        // Technical specifications without details
        if (lower.contains("implement") || lower.contains("build"))
            && !lower.contains("how")
            && lower.split_whitespace().count() < 10
        {
            gaps.push(KnowledgeGap {
                description: "Insufficient implementation details".to_string(),
                priority: Priority::High,
                resolvable: true,
                resolution_strategy: Some(ResolutionStrategy::AskUser),
                impact: "May provide generic solution instead of specific one".to_string(),
            });
        }

        gaps
    }

    /// Generate reasonable assumptions
    fn generate_assumptions(&self, gaps: &[KnowledgeGap], state: &State) -> Vec<Assumption> {
        let mut assumptions = Vec::new();

        // For each gap, consider if we can make a reasonable assumption
        for gap in gaps {
            if gap.priority <= Priority::Medium && gap.resolvable {
                // Make assumptions for low-medium priority gaps
                let assumption = match gap.resolution_strategy {
                    Some(ResolutionStrategy::DeriveFromContext) => Assumption {
                        assumption: format!("Assuming typical context for: {}", gap.description),
                        confidence: 0.6,
                        risk_level: RiskLevel::Low,
                    },
                    Some(ResolutionStrategy::Assume) => Assumption {
                        assumption: format!(
                            "Assuming standard interpretation: {}",
                            gap.description
                        ),
                        confidence: 0.5,
                        risk_level: RiskLevel::Medium,
                    },
                    _ => continue,
                };
                assumptions.push(assumption);
            }
        }

        // Assume user wants help if asking questions
        if state.data.get("intent").and_then(|i| i.as_str()) == Some("question") {
            assumptions.push(Assumption {
                assumption: "User wants informative, helpful response".to_string(),
                confidence: 0.9,
                risk_level: RiskLevel::Low,
            });
        }

        assumptions
    }

    /// Calculate overall confidence score
    fn calculate_confidence(&self, known_facts: &[KnownFact], gaps: &[KnowledgeGap]) -> f32 {
        if known_facts.is_empty() && gaps.is_empty() {
            return 0.5; // Neutral when no information
        }

        // Weight by priority of gaps
        let gap_penalty: f32 = gaps
            .iter()
            .map(|g| match g.priority {
                Priority::Critical => 0.3,
                Priority::High => 0.2,
                Priority::Medium => 0.1,
                Priority::Low => 0.05,
            })
            .sum();

        // Boost from known facts
        let fact_boost = (known_facts.len() as f32 * 0.1).min(0.4);

        // Calculate confidence
        let confidence = 0.5 + fact_boost - gap_penalty;

        confidence.max(0.1).min(1.0)
    }

    /// Generate summary
    fn generate_summary(
        &self,
        known_facts: &[KnownFact],
        gaps: &[KnowledgeGap],
        assumptions: &[Assumption],
    ) -> String {
        let critical_gaps = gaps
            .iter()
            .filter(|g| g.priority == Priority::Critical)
            .count();
        let high_gaps = gaps.iter().filter(|g| g.priority == Priority::High).count();

        format!(
            "Known: {} facts | Unknown: {} gaps ({} critical, {} high) | Assumptions: {}",
            known_facts.len(),
            gaps.len(),
            critical_gaps,
            high_gaps,
            assumptions.len()
        )
    }
}

impl Default for KnowledgeAnalyzer {
    fn default() -> Self {
        Self::new()
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
    async fn test_knowledge_analysis_simple() {
        let analyzer = KnowledgeAnalyzer::new();
        let message = create_test_message("Hello, how are you?");
        let state = State::new();

        let knowledge = analyzer.analyze(&message, &state).await.unwrap();
        assert!(knowledge.confidence_score > 0.0);
    }

    #[tokio::test]
    async fn test_entity_extraction() {
        let analyzer = KnowledgeAnalyzer::new();
        let entities = analyzer.extract_entities("Tell me about Rust programming and Python");

        assert!(entities.contains(&"Rust".to_string()));
        assert!(entities.contains(&"Python".to_string()));
    }

    #[tokio::test]
    async fn test_contextual_gaps() {
        let analyzer = KnowledgeAnalyzer::new();
        let message = create_test_message("Can you continue from where we left off last time?");
        let state = State::new();

        let knowledge = analyzer.analyze(&message, &state).await.unwrap();
        assert!(!knowledge.unknown_gaps.is_empty());
        assert!(knowledge
            .unknown_gaps
            .iter()
            .any(|g| g.priority >= Priority::High));
    }

    #[tokio::test]
    async fn test_known_facts_from_state() {
        let analyzer = KnowledgeAnalyzer::new();
        let message = create_test_message("Hello");
        let mut state = State::new();
        state.data.insert(
            "agentName".to_string(),
            serde_json::Value::String("TestAgent".to_string()),
        );

        let knowledge = analyzer.analyze(&message, &state).await.unwrap();
        assert!(!knowledge.known_facts.is_empty());
        assert!(knowledge
            .known_facts
            .iter()
            .any(|f| f.fact.contains("TestAgent")));
    }
}
