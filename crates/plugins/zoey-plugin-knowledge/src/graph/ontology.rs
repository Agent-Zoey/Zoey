/*!
# Domain Ontology

Defines domain-specific concepts and hierarchies.
*/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Domain ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ontology {
    /// Domain name
    pub domain: String,

    /// Concept hierarchies (is-a relationships)
    pub hierarchies: HashMap<String, Vec<String>>,

    /// Domain-specific rules
    pub rules: Vec<OntologyRule>,
}

/// An ontology rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyRule {
    pub name: String,
    pub condition: String,
    pub conclusion: String,
}

impl Ontology {
    /// Create a new ontology
    pub fn new() -> Self {
        Self {
            domain: String::from("general"),
            hierarchies: HashMap::new(),
            rules: Vec::new(),
        }
    }

    /// Add a concept hierarchy
    pub fn add_hierarchy(&mut self, parent: impl Into<String>, children: Vec<String>) {
        self.hierarchies.insert(parent.into(), children);
    }

    /// Add a rule
    pub fn add_rule(&mut self, rule: OntologyRule) {
        self.rules.push(rule);
    }

    /// Check if concept is a subconcept of another
    pub fn is_subconcept_of(&self, child: &str, parent: &str) -> bool {
        if let Some(children) = self.hierarchies.get(parent) {
            children.iter().any(|c| c == child)
        } else {
            false
        }
    }
}

impl Default for Ontology {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ontology() {
        let mut ontology = Ontology::new();
        ontology.add_hierarchy("Animal", vec!["Dog".to_string(), "Cat".to_string()]);

        assert!(ontology.is_subconcept_of("Dog", "Animal"));
        assert!(!ontology.is_subconcept_of("Car", "Animal"));
    }
}
