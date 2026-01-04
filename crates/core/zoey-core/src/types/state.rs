//! State types for conversation state management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// State for conversation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// String values from providers (for template substitution)
    pub values: HashMap<String, String>,

    /// Structured data from providers (for programmatic access)
    pub data: HashMap<String, serde_json::Value>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            data: HashMap::new(),
        }
    }
}

impl State {
    /// Create a new empty state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a string value
    pub fn set_value(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), value.into());
    }

    /// Get a string value
    pub fn get_value(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }

    /// Set structured data
    pub fn set_data(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.data.insert(key.into(), value);
    }

    /// Get structured data
    pub fn get_data(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    /// Merge another state into this one
    pub fn merge(&mut self, other: State) {
        self.values.extend(other.values);
        self.data.extend(other.data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_values() {
        let mut state = State::new();
        state.set_value("key1", "value1");

        assert_eq!(state.get_value("key1"), Some(&"value1".to_string()));
        assert_eq!(state.get_value("key2"), None);
    }

    #[test]
    fn test_state_data() {
        let mut state = State::new();
        state.set_data("key1", serde_json::json!({"nested": "value"}));

        assert!(state.get_data("key1").is_some());
        assert!(state.get_data("key2").is_none());
    }

    #[test]
    fn test_state_merge() {
        let mut state1 = State::new();
        state1.set_value("key1", "value1");

        let mut state2 = State::new();
        state2.set_value("key2", "value2");

        state1.merge(state2);

        assert_eq!(state1.get_value("key1"), Some(&"value1".to_string()));
        assert_eq!(state1.get_value("key2"), Some(&"value2".to_string()));
    }
}
