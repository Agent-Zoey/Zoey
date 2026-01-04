//! Action formatting utilities

use crate::types::{Action, ActionExample};
use rand::seq::SliceRandom;
use std::sync::Arc;

/// Composes a set of example conversations based on provided actions and a specified count.
/// It randomly selects examples from the provided actions and formats them with generated names.
///
/// # Arguments
/// * `actions_data` - An array of `Action` trait objects from which to draw examples.
/// * `count` - The number of examples to generate.
///
/// # Returns
/// A string containing formatted examples of conversations.
pub fn compose_action_examples(actions: &[Arc<dyn Action>], count: usize) -> String {
    // Handle edge cases
    if actions.is_empty() || count == 0 {
        return String::new();
    }

    // Filter out actions without examples
    let actions_with_examples: Vec<_> = actions
        .iter()
        .filter(|action| !action.examples().is_empty())
        .collect();

    // If no actions have examples, return empty string
    if actions_with_examples.is_empty() {
        return String::new();
    }

    // Create a working copy of the examples
    let mut examples_copy: Vec<Vec<Vec<ActionExample>>> = actions_with_examples
        .iter()
        .map(|action| action.examples())
        .collect();

    let mut selected_examples: Vec<Vec<ActionExample>> = Vec::new();
    let mut rng = rand::thread_rng();

    // Keep track of actions that still have examples
    let mut available_action_indices: Vec<usize> = examples_copy
        .iter()
        .enumerate()
        .filter(|(_, examples)| !examples.is_empty())
        .map(|(i, _)| i)
        .collect();

    // Select examples until we reach the count or run out of examples
    while selected_examples.len() < count && !available_action_indices.is_empty() {
        // Randomly select an action
        let random_index = *available_action_indices.choose(&mut rng).unwrap();
        let examples = &mut examples_copy[random_index];

        // Select a random example from this action
        let example_index = rand::random::<usize>() % examples.len();
        selected_examples.push(examples.remove(example_index));

        // Remove action if it has no more examples
        if examples.is_empty() {
            available_action_indices.retain(|&i| i != random_index);
        }
    }

    // Format the selected examples
    format_selected_examples(&selected_examples)
}

/// Formats selected example conversations with random names.
fn format_selected_examples(examples: &[Vec<ActionExample>]) -> String {
    use names::Generator;

    examples
        .iter()
        .map(|example| {
            // Generate random names for this example
            let mut generator = Generator::default();
            let random_names: Vec<String> = (0..5).map(|_| generator.next().unwrap()).collect();

            // Format the conversation
            let conversation: Vec<String> = example
                .iter()
                .map(|message| {
                    // Build the base message - only include the text, no action info
                    let mut message_text = format!("{}: {}", message.name, message.text);

                    // Replace name placeholders
                    for (i, name) in random_names.iter().enumerate() {
                        message_text =
                            message_text.replace(&format!("{{{{name{}}}}}", i + 1), name);
                    }

                    message_text
                })
                .collect();

            format!("\n{}", conversation.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats the names of the provided actions into a comma-separated string.
///
/// # Arguments
/// * `actions` - An array of `Action` trait objects from which to extract names.
///
/// # Returns
/// A comma-separated string of action names.
pub fn format_action_names(actions: &[Arc<dyn Action>]) -> String {
    if actions.is_empty() {
        return String::new();
    }

    let mut action_names: Vec<_> = actions.iter().map(|a| a.name()).collect();

    // Shuffle for variety
    let mut rng = rand::thread_rng();
    action_names.shuffle(&mut rng);

    action_names.join(", ")
}

/// Formats the provided actions into a detailed string listing each action's name and description.
///
/// # Arguments
/// * `actions` - An array of `Action` trait objects to format.
///
/// # Returns
/// A detailed string of actions, including names and descriptions.
pub fn format_actions(actions: &[Arc<dyn Action>]) -> String {
    if actions.is_empty() {
        return String::new();
    }

    let mut action_list: Vec<_> = actions
        .iter()
        .map(|action| {
            let description = action.description();
            let desc_text = if description.is_empty() {
                "No description available"
            } else {
                description
            };
            format!("- **{}**: {}", action.name(), desc_text)
        })
        .collect();

    // Shuffle for variety
    let mut rng = rand::thread_rng();
    action_list.shuffle(&mut rng);

    action_list.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Memory, State};
    use crate::Result;
    use async_trait::async_trait;

    struct MockAction {
        name: String,
        description: String,
        examples: Vec<Vec<ActionExample>>,
    }

    #[async_trait]
    impl Action for MockAction {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn examples(&self) -> Vec<Vec<ActionExample>> {
            self.examples.clone()
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
            _runtime: Arc<dyn std::any::Any + Send + Sync>,
            _message: &Memory,
            _state: &State,
            _options: Option<super::super::types::HandlerOptions>,
            _callback: Option<super::super::types::HandlerCallback>,
        ) -> Result<Option<super::super::types::ActionResult>> {
            Ok(None)
        }
    }

    #[test]
    fn test_format_action_names() {
        let actions: Vec<Arc<dyn Action>> = vec![
            Arc::new(MockAction {
                name: "test1".to_string(),
                description: "Test 1".to_string(),
                examples: vec![],
            }),
            Arc::new(MockAction {
                name: "test2".to_string(),
                description: "Test 2".to_string(),
                examples: vec![],
            }),
        ];

        let names = format_action_names(&actions);
        assert!(names.contains("test1"));
        assert!(names.contains("test2"));
        assert!(names.contains(", "));
    }

    #[test]
    fn test_format_actions() {
        let actions: Vec<Arc<dyn Action>> = vec![Arc::new(MockAction {
            name: "test1".to_string(),
            description: "Test action 1".to_string(),
            examples: vec![],
        })];

        let formatted = format_actions(&actions);
        assert!(formatted.contains("test1"));
        assert!(formatted.contains("Test action 1"));
        assert!(formatted.contains("**"));
    }

    #[test]
    fn test_empty_actions() {
        let actions: Vec<Arc<dyn Action>> = vec![];
        assert_eq!(format_action_names(&actions), "");
        assert_eq!(format_actions(&actions), "");
    }
}
