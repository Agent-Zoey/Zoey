/*!
# Entity Types and Extraction

Defines entity types and extraction logic.
*/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Entity types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Date,
    Medication,
    Disease,
    Procedure,
    Symptom,
    Device,
    Concept,
    Other,
}

/// An entity in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Uuid,
    pub entity_type: EntityType,
    pub name: String,
    pub aliases: Vec<String>,
    pub attributes: HashMap<String, String>,
    pub confidence: f64,
}

impl Entity {
    /// Create a new entity
    pub fn new(entity_type: impl Into<String>, name: impl Into<String>) -> Self {
        let entity_type_str = entity_type.into();
        let entity_type = match entity_type_str.to_lowercase().as_str() {
            "person" => EntityType::Person,
            "organization" | "org" => EntityType::Organization,
            "location" | "place" => EntityType::Location,
            "date" | "time" => EntityType::Date,
            "medication" | "drug" => EntityType::Medication,
            "disease" | "condition" => EntityType::Disease,
            "procedure" | "treatment" => EntityType::Procedure,
            "symptom" => EntityType::Symptom,
            "device" | "equipment" => EntityType::Device,
            "concept" => EntityType::Concept,
            _ => EntityType::Other,
        };

        Self {
            id: Uuid::new_v4(),
            entity_type,
            name: name.into(),
            aliases: Vec::new(),
            attributes: HashMap::new(),
            confidence: 1.0,
        }
    }

    /// Add an alias
    pub fn add_alias(&mut self, alias: impl Into<String>) {
        self.aliases.push(alias.into());
    }

    /// Add an attribute
    pub fn add_attribute(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.attributes.insert(key.into(), value.into());
    }

    /// Set confidence
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

/// Entity extractor using pattern matching and heuristics
pub struct EntityExtractor {
    /// Patterns for entity recognition
    patterns: HashMap<EntityType, Vec<String>>,
}

impl EntityExtractor {
    /// Create a new entity extractor
    pub fn new() -> Self {
        Self {
            patterns: Self::default_patterns(),
        }
    }

    /// Extract entities from text
    pub fn extract_entities(&self, text: &str) -> anyhow::Result<Vec<Entity>> {
        let mut entities = Vec::new();

        // Simple pattern-based extraction
        // In production, you'd use NLP libraries or ML models

        // Extract capitalized phrases (potential proper nouns)
        for word in text.split_whitespace() {
            if word.chars().next().map_or(false, |c| c.is_uppercase()) && word.len() > 2 {
                // Heuristic: capitalized words might be entities
                let entity =
                    Entity::new("Concept", word.trim_matches(|c: char| !c.is_alphanumeric()));
                entities.push(entity);
            }
        }

        // Remove duplicates by name
        entities.sort_by(|a, b| a.name.cmp(&b.name));
        entities.dedup_by(|a, b| a.name == b.name);

        Ok(entities)
    }

    fn default_patterns() -> HashMap<EntityType, Vec<String>> {
        let mut patterns = HashMap::new();

        patterns.insert(
            EntityType::Person,
            vec!["Dr.", "Mr.", "Mrs.", "Ms."]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );

        patterns.insert(
            EntityType::Organization,
            vec!["Inc.", "LLC", "Corp.", "Company"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );

        patterns
    }
}

impl Default for EntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_creation() {
        let entity = Entity::new("Person", "John Doe");
        assert_eq!(entity.entity_type, EntityType::Person);
        assert_eq!(entity.name, "John Doe");
    }

    #[test]
    fn test_entity_with_confidence() {
        let entity = Entity::new("Medication", "Aspirin").with_confidence(0.95);
        assert_eq!(entity.confidence, 0.95);
    }

    #[test]
    fn test_extractor() {
        let extractor = EntityExtractor::new();
        let text = "Alice went to Boston to see Microsoft.";
        let entities = extractor.extract_entities(text).unwrap();
        assert!(!entities.is_empty());
    }
}
