/*!
# Relationships Between Entities

Defines relationship types and detection logic.
*/

use serde::{Deserialize, Serialize};

/// Relationship types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationType {
    IsA,
    PartOf,
    HasProperty,
    Causes,
    Treats,
    DiagnosedWith,
    WorksFor,
    LocatedIn,
    Interacts,
    RelatedTo,
    Custom,
}

/// A relationship between two entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub rel_type: RelationType,
    pub confidence: f64,
    pub properties: std::collections::HashMap<String, String>,
}

impl Relationship {
    /// Create a new relationship
    pub fn new(rel_type: RelationType, confidence: f64) -> Self {
        Self {
            rel_type,
            confidence: confidence.clamp(0.0, 1.0),
            properties: std::collections::HashMap::new(),
        }
    }

    /// Add a property
    pub fn add_property(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.properties.insert(key.into(), value.into());
    }
}

/// Relationship detector
pub struct RelationshipDetector;

impl RelationshipDetector {
    /// Detect relationships in text
    pub fn detect_relationships(_text: &str) -> Vec<(String, String, RelationType)> {
        // Placeholder implementation
        // In production, use NLP dependency parsing
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relationship_creation() {
        let rel = Relationship::new(RelationType::Causes, 0.9);
        assert_eq!(rel.rel_type, RelationType::Causes);
        assert_eq!(rel.confidence, 0.9);
    }
}
