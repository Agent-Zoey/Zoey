//! Agent and Character types

use super::primitives::{Metadata, UUID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Character definition for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    /// Optional unique ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<UUID>,

    /// Character name
    pub name: String,

    /// Optional username
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// Biography/description
    #[serde(default)]
    pub bio: Vec<String>,

    /// Character lore/background
    #[serde(default)]
    pub lore: Vec<String>,

    /// Knowledge base
    #[serde(default)]
    pub knowledge: Vec<String>,

    /// Message examples for training
    #[serde(default)]
    pub message_examples: Vec<Vec<MessageExample>>,

    /// Post examples
    #[serde(default)]
    pub post_examples: Vec<String>,

    /// Topics of interest
    #[serde(default)]
    pub topics: Vec<String>,

    /// Character style guide
    #[serde(default)]
    pub style: CharacterStyle,

    /// Adjectives describing the character
    #[serde(default)]
    pub adjectives: Vec<String>,

    /// Settings/configuration
    #[serde(default)]
    pub settings: Metadata,

    /// Custom templates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub templates: Option<CharacterTemplates>,

    /// Plugins to load
    #[serde(default)]
    pub plugins: Vec<String>,

    /// Client configurations
    #[serde(default)]
    pub clients: Vec<String>,

    /// Model provider (e.g., "openai", "anthropic")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
}

/// Message example for character training
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageExample {
    /// User/entity name
    pub name: String,

    /// Message text
    pub text: String,
}

/// Character style configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterStyle {
    /// Writing style guidelines
    #[serde(default)]
    pub all: Vec<String>,

    /// Chat-specific style
    #[serde(default)]
    pub chat: Vec<String>,

    /// Post-specific style
    #[serde(default)]
    pub post: Vec<String>,
}

/// Custom templates for the character
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterTemplates {
    /// Message handler template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_handler_template: Option<String>,

    /// Post creation template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_creation_template: Option<String>,

    /// Additional custom templates
    #[serde(flatten)]
    pub custom: HashMap<String, String>,
}

/// Agent database record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// Agent ID
    pub id: UUID,

    /// Agent name
    pub name: String,

    /// Character configuration (JSON)
    pub character: serde_json::Value,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,

    /// Update timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_character_default() {
        let character = Character {
            name: "TestBot".to_string(),
            bio: vec!["A helpful assistant".to_string()],
            ..Default::default()
        };

        assert_eq!(character.name, "TestBot");
        assert_eq!(character.bio.len(), 1);
    }

    #[test]
    fn test_character_serialization() {
        let character = Character {
            name: "TestBot".to_string(),
            username: Some("testbot".to_string()),
            bio: vec!["A helpful assistant".to_string()],
            ..Default::default()
        };

        let json = serde_json::to_string(&character).unwrap();
        assert!(json.contains("TestBot"));
    }
}

impl Default for Character {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            username: None,
            bio: Vec::new(),
            lore: Vec::new(),
            knowledge: Vec::new(),
            message_examples: Vec::new(),
            post_examples: Vec::new(),
            topics: Vec::new(),
            style: CharacterStyle::default(),
            adjectives: Vec::new(),
            settings: HashMap::new(),
            templates: None,
            plugins: Vec::new(),
            clients: Vec::new(),
            model_provider: None,
        }
    }
}
