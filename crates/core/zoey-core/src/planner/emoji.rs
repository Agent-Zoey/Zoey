//! Emoji strategy planning

use crate::types::*;
use crate::Result;
use serde::{Deserialize, Serialize};

/// Type of emoji usage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EmojiType {
    /// Message reaction (👍, ❤️, 🎉)
    Reaction,
    /// Inline in text response
    InlineText,
    /// For emphasis on important points
    Emphasis,
    /// None - no emojis
    None,
}

/// Emoji tone/category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EmojiTone {
    /// Positive/happy (😊, ✅, 🎉)
    Positive,
    /// Helpful/informative (💡, 📚, ℹ️)
    Informative,
    /// Warning/caution (⚠️, ⚡)
    Warning,
    /// Technical/professional (🔧, 💻, 🛠️)
    Technical,
    /// Friendly/casual (👋, 🙂)
    Friendly,
    /// Thinking/processing (🤔, 💭)
    Thinking,
}

/// Emoji strategy for response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojiStrategy {
    /// Whether to use emojis
    pub should_use_emojis: bool,

    /// Maximum number of emojis
    pub max_emojis: usize,

    /// Types of emojis to use
    pub emoji_types: Vec<EmojiType>,

    /// Recommended tone
    pub recommended_tone: Option<EmojiTone>,

    /// Specific emoji suggestions
    pub suggestions: Vec<String>,

    /// Reasoning for decision
    pub reasoning: String,
}

/// Emoji planner
pub struct EmojiPlanner;

impl EmojiPlanner {
    /// Create a new emoji planner
    pub fn new() -> Self {
        Self
    }

    /// Plan emoji usage for a response
    pub async fn plan_emoji_usage(&self, message: &Memory, state: &State) -> Result<EmojiStrategy> {
        let text = &message.content.text;
        let lower = text.to_lowercase();

        // Determine if emojis are appropriate
        let should_use = self.should_use_emojis(&lower, state);

        if !should_use {
            return Ok(EmojiStrategy {
                should_use_emojis: false,
                max_emojis: 0,
                emoji_types: vec![EmojiType::None],
                recommended_tone: None,
                suggestions: vec![],
                reasoning: "Context inappropriate for emojis (formal/technical)".to_string(),
            });
        }

        // Determine context and tone
        let tone = self.determine_tone(&lower);
        let emoji_types = self.determine_types(&lower);
        let max_emojis = self.calculate_max_emojis(&lower, &emoji_types);
        let suggestions = self.suggest_emojis(&tone, &emoji_types);
        let reasoning = self.build_reasoning(&tone, &emoji_types, &suggestions);

        Ok(EmojiStrategy {
            should_use_emojis: true,
            max_emojis,
            emoji_types,
            recommended_tone: Some(tone),
            suggestions,
            reasoning,
        })
    }

    /// Determine if emojis should be used
    fn should_use_emojis(&self, text: &str, state: &State) -> bool {
        // Check character settings
        if let Some(settings) = state.data.get("characterSettings") {
            if let Some(no_emojis) = settings.get("noEmojis") {
                if no_emojis.as_bool().unwrap_or(false) {
                    return false;
                }
            }
        }

        // Formal contexts - no emojis
        let formal_indicators = [
            "formal report",
            "documentation",
            "legal",
            "contract",
            "official",
            "professional documentation",
            "technical specification",
        ];

        for indicator in &formal_indicators {
            if text.contains(indicator) {
                return false;
            }
        }

        // Very short technical messages - no emojis
        if text.len() < 20 && (text.contains("error") || text.contains("code")) {
            return false;
        }

        true
    }

    /// Determine appropriate emoji tone
    fn determine_tone(&self, text: &str) -> EmojiTone {
        let technical_keywords = [
            "code",
            "function",
            "algorithm",
            "implement",
            "debug",
            "compile",
            "syntax",
            "error",
        ];

        let warning_keywords = ["warning", "caution", "careful", "note", "important"];

        let is_question = text.contains('?') || text.contains("how") || text.contains("what");
        let tech_hits = technical_keywords
            .iter()
            .filter(|k| text.contains(*k))
            .count();

        if is_question {
            if tech_hits >= 2 {
                return EmojiTone::Technical;
            }
            return EmojiTone::Informative;
        }

        if tech_hits >= 1 {
            return EmojiTone::Technical;
        }

        if warning_keywords.iter().any(|k| text.contains(k)) {
            return EmojiTone::Warning;
        }

        // Check for positive sentiment
        let positive_keywords = ["thank", "great", "awesome", "good", "nice", "love"];
        if positive_keywords.iter().any(|k| text.contains(k)) {
            return EmojiTone::Positive;
        }

        // Check for thinking/contemplation
        let thinking_keywords = [
            "think",
            "consider",
            "maybe",
            "wonder",
            "curious",
            "interesting",
        ];
        if thinking_keywords.iter().any(|k| text.contains(k)) {
            return EmojiTone::Thinking;
        }

        // Default to friendly
        EmojiTone::Friendly
    }

    /// Determine which types of emojis to use
    fn determine_types(&self, text: &str) -> Vec<EmojiType> {
        let mut types = Vec::new();

        // Very short messages might just get a reaction
        if text.split_whitespace().count() < 5 {
            types.push(EmojiType::Reaction);
            return types;
        }

        // Questions and conversations can have inline emojis
        if text.contains('?') || text.split_whitespace().count() > 10 {
            types.push(EmojiType::InlineText);
        }

        // Important points can have emphasis
        if text.contains('!') || text.contains("important") || text.contains("note") {
            types.push(EmojiType::Emphasis);
        }

        // If nothing specific, default to reaction
        if types.is_empty() {
            types.push(EmojiType::Reaction);
        }

        types
    }

    /// Calculate maximum number of emojis
    fn calculate_max_emojis(&self, text: &str, emoji_types: &[EmojiType]) -> usize {
        let word_count = text.split_whitespace().count();

        // Base calculation: roughly 1 emoji per 20-30 words
        let base = ((word_count as f32 / 30.0).ceil() as usize).max(2).min(3);

        // Adjust based on types
        if emoji_types.contains(&EmojiType::Reaction) {
            1 // Just one reaction
        } else if emoji_types.contains(&EmojiType::InlineText) {
            base // Up to 3 inline by base limit
        } else {
            base.min(2) // Conservative default
        }
    }

    /// Suggest specific emojis based on tone
    fn suggest_emojis(&self, tone: &EmojiTone, emoji_types: &[EmojiType]) -> Vec<String> {
        let mut suggestions = Vec::new();

        let emoji_map = match tone {
            EmojiTone::Positive => {
                vec!["✅", "🎉", "😊", "👍", "⭐", "💚"]
            }
            EmojiTone::Informative => {
                vec!["💡", "📚", "ℹ️", "📖", "🔍", "📝"]
            }
            EmojiTone::Warning => {
                vec!["⚠️", "⚡", "🔔", "❗", "⛔"]
            }
            EmojiTone::Technical => {
                vec!["💻", "🔧", "🛠️", "⚙️", "🖥️", "⌨️"]
            }
            EmojiTone::Friendly => {
                vec!["👋", "🙂", "😄", "🤗", "💬"]
            }
            EmojiTone::Thinking => {
                vec!["🤔", "💭", "🧠", "💡", "📊"]
            }
        };

        // Select based on emoji types
        if emoji_types.contains(&EmojiType::Reaction) {
            suggestions.push(emoji_map[0].to_string());
        }

        if emoji_types.contains(&EmojiType::InlineText) {
            for emoji in emoji_map.iter().take(3) {
                suggestions.push(emoji.to_string());
            }
        }

        if emoji_types.contains(&EmojiType::Emphasis) {
            suggestions.push(emoji_map[0].to_string());
        }

        suggestions
    }

    /// Build reasoning explanation
    fn build_reasoning(
        &self,
        tone: &EmojiTone,
        emoji_types: &[EmojiType],
        suggestions: &[String],
    ) -> String {
        format!(
            "Tone: {:?} | Types: {:?} | Suggestions: {:?}",
            tone, emoji_types, suggestions
        )
    }
}

impl Default for EmojiPlanner {
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
    async fn test_emoji_planning_friendly() {
        let planner = EmojiPlanner::new();
        let message = create_test_message("Hi there! How are you doing today?");
        let state = State::new();

        let strategy = planner.plan_emoji_usage(&message, &state).await.unwrap();

        assert!(strategy.should_use_emojis);
        assert!(!strategy.suggestions.is_empty());
    }

    #[tokio::test]
    async fn test_emoji_planning_technical() {
        let planner = EmojiPlanner::new();
        let message =
            create_test_message("Can you help me debug this function? It's throwing an error.");
        let state = State::new();

        let strategy = planner.plan_emoji_usage(&message, &state).await.unwrap();

        assert!(strategy.should_use_emojis);
        assert_eq!(strategy.recommended_tone, Some(EmojiTone::Technical));
    }

    #[tokio::test]
    async fn test_emoji_planning_formal() {
        let planner = EmojiPlanner::new();
        let message = create_test_message("Please provide a formal report on the specifications.");
        let state = State::new();

        let strategy = planner.plan_emoji_usage(&message, &state).await.unwrap();

        assert!(!strategy.should_use_emojis);
    }

    #[test]
    fn test_tone_detection() {
        let planner = EmojiPlanner::new();

        assert_eq!(
            planner.determine_tone("how does this code work?"),
            EmojiTone::Informative
        );
        assert_eq!(
            planner.determine_tone("this is awesome!"),
            EmojiTone::Positive
        );
        assert_eq!(
            planner.determine_tone("warning: be careful"),
            EmojiTone::Warning
        );
    }

    #[test]
    fn test_max_emojis_calculation() {
        let planner = EmojiPlanner::new();

        let short_text = "hi";
        let reaction_types = vec![EmojiType::Reaction];
        assert_eq!(planner.calculate_max_emojis(short_text, &reaction_types), 1);

        let long_text = "this is a much longer message with many words that should allow for more emojis to be used in the response";
        let inline_types = vec![EmojiType::InlineText];
        let max = planner.calculate_max_emojis(long_text, &inline_types);
        assert!(max > 1 && max <= 3);
    }
}
