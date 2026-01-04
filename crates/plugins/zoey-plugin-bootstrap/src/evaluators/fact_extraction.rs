//! Fact extraction evaluator - extracts and stores facts from conversations

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Types of facts that can be extracted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FactType {
    /// Personal information (name, age, etc.)
    Personal,
    /// Location information (where they live, work, etc.)
    Location,
    /// Occupation/career information
    Occupation,
    /// Preferences (likes, dislikes, interests)
    Preference,
    /// Relationships (family, friends, colleagues)
    Relationship,
    /// Skills and abilities
    Skill,
    /// Goals and aspirations
    Goal,
    /// Events and experiences
    Event,
    /// Opinions and beliefs
    Opinion,
    /// Other factual information
    Other,
}

/// Extracted fact with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFact {
    /// Type of fact
    pub fact_type: FactType,
    /// The actual fact content
    pub content: String,
    /// Entity the fact is about
    pub entity_id: uuid::Uuid,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Source message
    pub source_message_id: uuid::Uuid,
    /// When the fact was extracted
    pub extracted_at: i64,
    /// Structured data if available
    pub metadata: Option<serde_json::Value>,
}

/// Fact extraction evaluator
pub struct FactExtractionEvaluator;

#[async_trait]
impl Evaluator for FactExtractionEvaluator {
    fn name(&self) -> &str {
        "fact_extraction"
    }

    fn description(&self) -> &str {
        "Extracts factual information from conversations and stores them in memory"
    }

    fn examples(&self) -> Vec<EvaluationExample> {
        vec![EvaluationExample {
            prompt: "Extract facts from conversation".to_string(),
            messages: vec![ActionExample {
                name: "User".to_string(),
                text: "I live in San Francisco and work as a software engineer".to_string(),
            }],
            outcome: "Extracted: location=San Francisco, occupation=software engineer".to_string(),
        }]
    }

    fn always_run(&self) -> bool {
        true // Always extract facts
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }

    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _did_respond: bool,
        _responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        tracing::debug!(
            "Fact extraction: Analyzing message {} for factual information",
            message.id
        );

        // Quick keyword scan to determine if message likely contains facts
        if !self.has_potential_facts(&message.content.text) {
            tracing::debug!("No potential facts detected in message");
            return Ok(());
        }

        // Extract facts using LLM
        let facts = self
            .extract_facts_with_llm(runtime.clone(), message)
            .await?;

        if facts.is_empty() {
            tracing::debug!("No facts extracted from message");
            return Ok(());
        }

        tracing::info!(
            "Extracted {} fact(s) from message {}",
            facts.len(),
            message.id
        );

        // Store facts in database
        self.store_facts(runtime, &facts, message).await?;

        // Log extracted facts for visibility
        for fact in &facts {
            tracing::info!(
                "  → {:?}: {} (confidence: {:.2})",
                fact.fact_type,
                fact.content,
                fact.confidence
            );
        }

        Ok(())
    }
}

impl FactExtractionEvaluator {
    /// Quick heuristic to check if message might contain facts
    fn has_potential_facts(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase();

        // Skip very short messages
        if text.len() < 10 {
            return false;
        }

        // Skip questions (they usually don't contain facts)
        if text.starts_with("what")
            || text.starts_with("how")
            || text.starts_with("why")
            || text.starts_with("when")
            || text.starts_with("where")
            || text.starts_with("who")
        {
            return false;
        }

        // Check for fact indicators
        let fact_indicators = [
            "i am",
            "i'm",
            "my",
            "live in",
            "work as",
            "from",
            "like",
            "love",
            "enjoy",
            "prefer",
            "hate",
            "dislike",
            "studied",
            "graduated",
            "degree",
            "family",
            "friend",
            "born",
            "grew up",
            "moved to",
            "want to",
            "plan to",
            "believe",
            "think that",
            "in my opinion",
            "skilled in",
            "experienced in",
            "know how to",
            "can do",
            "able to",
        ];

        fact_indicators
            .iter()
            .any(|indicator| text_lower.contains(indicator))
    }

    /// Extract facts using LLM
    async fn extract_facts_with_llm(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
    ) -> Result<Vec<ExtractedFact>> {
        use zoey_core::runtime_ref::downcast_runtime_ref;

        // Get the runtime reference
        let runtime_ref = match downcast_runtime_ref(&runtime) {
            Some(rt) => rt,
            None => {
                tracing::warn!("Failed to downcast runtime for fact extraction");
                return Ok(vec![]);
            }
        };

        // Try to upgrade to full runtime access
        let full_runtime = match runtime_ref.try_upgrade() {
            Some(rt) => rt,
            None => {
                tracing::warn!("Runtime no longer available");
                return Ok(vec![]);
            }
        };

        // Build prompt for fact extraction
        let prompt = self.build_extraction_prompt(&message.content.text);

        // Get model handlers
        let models = full_runtime.read().unwrap().get_models();

        if let Some(handlers) = models.get("TEXT_SMALL") {
            if let Some(first_handler) = handlers.first() {
                let handler_params = ModelHandlerParams {
                    runtime: runtime.clone(),
                    params: GenerateTextParams {
                        prompt,
                        model: None,
                        temperature: Some(0.3), // Lower temperature for more consistent extraction
                        max_tokens: Some(500),
                        top_p: None,
                        stop: None,
                        frequency_penalty: None,
                        presence_penalty: None,
                    },
                };

                match (first_handler.handler)(handler_params).await {
                    Ok(response) => {
                        return self.parse_llm_response(&response, message);
                    }
                    Err(e) => {
                        tracing::warn!("LLM fact extraction failed: {}", e);
                    }
                }
            }
        }

        // Fallback to keyword-based extraction
        tracing::debug!("Using fallback keyword-based extraction");
        Ok(self.keyword_based_extraction(message))
    }

    /// Build LLM prompt for fact extraction
    fn build_extraction_prompt(&self, text: &str) -> String {
        format!(
            r#"Extract factual information from the following message. Return ONLY a JSON array of facts.
Each fact should have: type, content, confidence (0.0-1.0).

Valid types: personal, location, occupation, preference, relationship, skill, goal, event, opinion, other

Message: "{}"

Return format (JSON only, no other text):
[
  {{"type": "location", "content": "lives in San Francisco", "confidence": 0.9}},
  {{"type": "occupation", "content": "works as a software engineer", "confidence": 0.95}}
]

If no facts found, return: []"#,
            text
        )
    }

    /// Parse LLM response into ExtractedFacts
    fn parse_llm_response(&self, response: &str, message: &Memory) -> Result<Vec<ExtractedFact>> {
        // Try to extract JSON from response (LLM might add extra text)
        let json_str = if let Some(start) = response.find('[') {
            if let Some(end) = response.rfind(']') {
                &response[start..=end]
            } else {
                response
            }
        } else {
            response
        };

        #[derive(Deserialize)]
        struct LLMFact {
            #[serde(rename = "type")]
            fact_type: String,
            content: String,
            confidence: f32,
        }

        match serde_json::from_str::<Vec<LLMFact>>(json_str) {
            Ok(llm_facts) => {
                let mut facts = Vec::new();

                for llm_fact in llm_facts {
                    let fact_type = match llm_fact.fact_type.to_lowercase().as_str() {
                        "personal" => FactType::Personal,
                        "location" => FactType::Location,
                        "occupation" => FactType::Occupation,
                        "preference" => FactType::Preference,
                        "relationship" => FactType::Relationship,
                        "skill" => FactType::Skill,
                        "goal" => FactType::Goal,
                        "event" => FactType::Event,
                        "opinion" => FactType::Opinion,
                        _ => FactType::Other,
                    };

                    facts.push(ExtractedFact {
                        fact_type,
                        content: llm_fact.content,
                        entity_id: message.entity_id,
                        confidence: llm_fact.confidence.clamp(0.0, 1.0),
                        source_message_id: message.id,
                        extracted_at: chrono::Utc::now().timestamp(),
                        metadata: None,
                    });
                }

                Ok(facts)
            }
            Err(e) => {
                tracing::warn!("Failed to parse LLM fact extraction response: {}", e);
                tracing::debug!("Response was: {}", response);
                Ok(vec![])
            }
        }
    }

    /// Fallback keyword-based extraction
    fn keyword_based_extraction(&self, message: &Memory) -> Vec<ExtractedFact> {
        let text = &message.content.text.to_lowercase();
        let mut facts = Vec::new();
        let timestamp = chrono::Utc::now().timestamp();

        // Location extraction
        if let Some(location) = self.extract_location(text) {
            facts.push(ExtractedFact {
                fact_type: FactType::Location,
                content: location,
                entity_id: message.entity_id,
                confidence: 0.7,
                source_message_id: message.id,
                extracted_at: timestamp,
                metadata: None,
            });
        }

        // Occupation extraction
        if let Some(occupation) = self.extract_occupation(text) {
            facts.push(ExtractedFact {
                fact_type: FactType::Occupation,
                content: occupation,
                entity_id: message.entity_id,
                confidence: 0.7,
                source_message_id: message.id,
                extracted_at: timestamp,
                metadata: None,
            });
        }

        // Preference extraction
        if let Some(preference) = self.extract_preference(text) {
            facts.push(ExtractedFact {
                fact_type: FactType::Preference,
                content: preference,
                entity_id: message.entity_id,
                confidence: 0.6,
                source_message_id: message.id,
                extracted_at: timestamp,
                metadata: None,
            });
        }

        facts
    }

    fn extract_location(&self, text: &str) -> Option<String> {
        if let Some(pos) = text.find("live in ") {
            let rest = &text[pos + 8..];
            let end = rest
                .find(&[' ', ',', '.', '!', '?'][..])
                .unwrap_or(rest.len());
            return Some(format!("lives in {}", &rest[..end]));
        }
        if let Some(pos) = text.find("from ") {
            let rest = &text[pos + 5..];
            let end = rest
                .find(&[' ', ',', '.', '!', '?'][..])
                .unwrap_or(rest.len());
            return Some(format!("from {}", &rest[..end]));
        }
        None
    }

    fn extract_occupation(&self, text: &str) -> Option<String> {
        if let Some(pos) = text.find("work as ") {
            let rest = &text[pos + 8..];
            let end = rest.find(&[',', '.', '!', '?'][..]).unwrap_or(rest.len());
            return Some(format!("works as {}", rest[..end].trim()));
        }
        if let Some(pos) = text.find("i'm a ") {
            let rest = &text[pos + 6..];
            let end = rest.find(&[',', '.', '!', '?'][..]).unwrap_or(rest.len());
            let occupation = rest[..end].trim();
            if !occupation.is_empty() {
                return Some(format!("is a {}", occupation));
            }
        }
        None
    }

    fn extract_preference(&self, text: &str) -> Option<String> {
        for verb in &["like", "love", "enjoy", "prefer"] {
            if let Some(pos) = text.find(&format!("{} ", verb)) {
                let rest = &text[pos + verb.len() + 1..];
                let end = rest.find(&[',', '.', '!', '?'][..]).unwrap_or(rest.len());
                let preference = rest[..end].trim();
                if !preference.is_empty() && preference.len() < 50 {
                    return Some(format!("{}s {}", verb, preference));
                }
            }
        }
        None
    }

    /// Store extracted facts in database
    async fn store_facts(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        facts: &[ExtractedFact],
        message: &Memory,
    ) -> Result<()> {
        use zoey_core::runtime_ref::downcast_runtime_ref;

        let runtime_ref = match downcast_runtime_ref(&runtime) {
            Some(rt) => rt,
            None => {
                tracing::warn!("Failed to downcast runtime for storing facts");
                return Ok(());
            }
        };

        // Try to upgrade to full runtime access
        let full_runtime = match runtime_ref.try_upgrade() {
            Some(rt) => rt,
            None => {
                tracing::warn!("Runtime no longer available");
                return Ok(());
            }
        };

        // Get database adapter
        let adapter = full_runtime.read().unwrap().get_adapter();

        if let Some(adapter) = adapter {
            for fact in facts {
                // Store fact as a memory with special metadata
                let mut content_metadata = HashMap::new();
                content_metadata.insert("type".to_string(), serde_json::json!("extracted_fact"));
                content_metadata.insert(
                    "fact_type".to_string(),
                    serde_json::json!(format!("{:?}", fact.fact_type)),
                );
                content_metadata
                    .insert("confidence".to_string(), serde_json::json!(fact.confidence));
                content_metadata.insert(
                    "source_message_id".to_string(),
                    serde_json::json!(fact.source_message_id.to_string()),
                );

                let fact_memory = Memory {
                    id: uuid::Uuid::new_v4(),
                    entity_id: fact.entity_id,
                    agent_id: message.agent_id,
                    room_id: message.room_id,
                    content: Content {
                        text: fact.content.clone(),
                        metadata: content_metadata,
                        ..Default::default()
                    },
                    embedding: None,
                    metadata: Some(MemoryMetadata {
                        memory_type: Some("extracted_fact".to_string()),
                        entity_name: None,
                        data: if let Some(ref meta) = fact.metadata {
                            let mut map = HashMap::new();
                            map.insert("fact_data".to_string(), meta.clone());
                            map
                        } else {
                            HashMap::new()
                        },
                    }),
                    created_at: fact.extracted_at,
                    unique: Some(false),
                    similarity: None,
                };

                match adapter.create_memory(&fact_memory, "facts").await {
                    Ok(id) => {
                        tracing::debug!("Stored fact with ID: {}", id);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to store fact: {}", e);
                    }
                }
            }
        } else {
            tracing::debug!("No database adapter available, facts not persisted");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fact_extraction_evaluator() {
        let evaluator = FactExtractionEvaluator;
        assert_eq!(evaluator.name(), "fact_extraction");
        assert!(evaluator.always_run());
    }

    #[test]
    fn test_has_potential_facts() {
        let evaluator = FactExtractionEvaluator;

        // Should detect facts
        assert!(evaluator.has_potential_facts("I live in New York"));
        assert!(evaluator.has_potential_facts("I work as a software engineer"));
        assert!(evaluator.has_potential_facts("I love programming"));
        assert!(evaluator.has_potential_facts("I'm from California"));
        assert!(evaluator.has_potential_facts("My favorite color is blue"));

        // Should NOT detect facts
        assert!(!evaluator.has_potential_facts("What is your name?"));
        assert!(!evaluator.has_potential_facts("How are you?"));
        assert!(!evaluator.has_potential_facts("hi"));
        assert!(!evaluator.has_potential_facts("ok"));
    }

    #[test]
    fn test_keyword_based_extraction() {
        let evaluator = FactExtractionEvaluator;

        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "I live in San Francisco and work as a designer. I love hiking.".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        let facts = evaluator.keyword_based_extraction(&message);

        assert!(!facts.is_empty());

        // Check for location fact
        let has_location = facts.iter().any(|f| f.fact_type == FactType::Location);
        assert!(has_location);

        // Check for occupation fact
        let has_occupation = facts.iter().any(|f| f.fact_type == FactType::Occupation);
        assert!(has_occupation);

        // Check for preference fact
        let has_preference = facts.iter().any(|f| f.fact_type == FactType::Preference);
        assert!(has_preference);
    }

    #[test]
    fn test_extract_location() {
        let evaluator = FactExtractionEvaluator;

        let loc1 = evaluator.extract_location("i live in tokyo");
        assert_eq!(loc1, Some("lives in tokyo".to_string()));

        let loc2 = evaluator.extract_location("i'm from paris");
        assert_eq!(loc2, Some("from paris".to_string()));

        let loc3 = evaluator.extract_location("hello there");
        assert_eq!(loc3, None);
    }

    #[test]
    fn test_extract_occupation() {
        let evaluator = FactExtractionEvaluator;

        let occ1 = evaluator.extract_occupation("i work as a software engineer");
        assert_eq!(occ1, Some("works as a software engineer".to_string()));

        let occ2 = evaluator.extract_occupation("i'm a designer");
        assert_eq!(occ2, Some("is a designer".to_string()));

        let occ3 = evaluator.extract_occupation("hello world");
        assert_eq!(occ3, None);
    }

    #[test]
    fn test_extract_preference() {
        let evaluator = FactExtractionEvaluator;

        let pref1 = evaluator.extract_preference("i like pizza");
        assert_eq!(pref1, Some("likes pizza".to_string()));

        let pref2 = evaluator.extract_preference("i love programming");
        assert_eq!(pref2, Some("loves programming".to_string()));

        let pref3 = evaluator.extract_preference("hello there");
        assert_eq!(pref3, None);
    }

    #[test]
    fn test_parse_llm_response() {
        let evaluator = FactExtractionEvaluator;

        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "Test message".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        let response = r#"[
            {"type": "location", "content": "lives in Tokyo", "confidence": 0.9},
            {"type": "occupation", "content": "works as engineer", "confidence": 0.85}
        ]"#;

        let facts = evaluator.parse_llm_response(response, &message).unwrap();
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].fact_type, FactType::Location);
        assert_eq!(facts[1].fact_type, FactType::Occupation);
    }

    #[test]
    fn test_parse_llm_response_with_extra_text() {
        let evaluator = FactExtractionEvaluator;

        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "Test message".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        // LLM might add extra text before/after JSON
        let response = r#"Here are the extracted facts:
        [
            {"type": "preference", "content": "likes coffee", "confidence": 0.8}
        ]
        These facts were extracted from the message."#;

        let facts = evaluator.parse_llm_response(response, &message).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_type, FactType::Preference);
    }
}
