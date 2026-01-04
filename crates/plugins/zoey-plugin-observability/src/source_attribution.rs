/*!
# Source Attribution Module

Tracks which documents, memories, and knowledge sources influenced a decision.
*/

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A source that contributed to a decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Unique identifier
    pub id: Uuid,

    /// Source type (document, memory, knowledge base, etc.)
    pub source_type: SourceType,

    /// Title or name
    pub title: String,

    /// Optional URL or path
    pub location: Option<String>,

    /// Author or creator
    pub author: Option<String>,

    /// Publication date or last modified
    pub date: Option<String>,

    /// Excerpt or relevant section
    pub excerpt: Option<String>,

    /// Additional metadata
    pub metadata: serde_json::Value,
}

/// Type of source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    /// Research paper
    ResearchPaper,

    /// Technical manual
    Manual,

    /// Medical guideline
    Guideline,

    /// Previous conversation/memory
    Memory,

    /// Knowledge base entry
    KnowledgeBase,

    /// External API or database
    ExternalData,

    /// User-provided information
    UserInput,

    /// Training data
    TrainingData,

    /// Other
    Other,
}

/// Attribution of a source to a decision with scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAttribution {
    /// The source
    pub source: Source,

    /// Attribution score (0.0-1.0) - how much this source influenced the decision
    pub attribution_score: f64,

    /// Relevance score (0.0-1.0) - how relevant the source is to the query
    pub relevance_score: f64,

    /// Which specific parts were used
    pub used_sections: Vec<String>,

    /// How it was used (direct quote, paraphrase, inference, etc.)
    pub usage_type: UsageType,
}

/// How a source was used
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsageType {
    /// Direct quotation
    DirectQuote,

    /// Paraphrased
    Paraphrase,

    /// Used for inference
    Inference,

    /// Background context
    Context,

    /// Supporting evidence
    Evidence,

    /// Contradiction that was considered
    Contradiction,
}

/// Score for attribution calculation
pub type AttributionScore = f64;

impl SourceAttribution {
    /// Create a new source attribution
    pub fn new(source: Source, attribution_score: f64, relevance_score: f64) -> Self {
        Self {
            source,
            attribution_score: attribution_score.clamp(0.0, 1.0),
            relevance_score: relevance_score.clamp(0.0, 1.0),
            used_sections: Vec::new(),
            usage_type: UsageType::Context,
        }
    }

    /// Add a section that was used
    pub fn add_used_section(&mut self, section: impl Into<String>) {
        self.used_sections.push(section.into());
    }

    /// Set usage type
    pub fn with_usage_type(mut self, usage_type: UsageType) -> Self {
        self.usage_type = usage_type;
        self
    }

    /// Combined importance score
    pub fn importance_score(&self) -> f64 {
        // Weight both attribution and relevance
        (self.attribution_score * 0.6) + (self.relevance_score * 0.4)
    }
}

impl Source {
    /// Create a new source
    pub fn new(source_type: SourceType, title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            source_type,
            title: title.into(),
            location: None,
            author: None,
            date: None,
            excerpt: None,
            metadata: serde_json::Value::Null,
        }
    }

    /// Add location/URL
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Add author
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Add date
    pub fn with_date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    /// Add excerpt
    pub fn with_excerpt(mut self, excerpt: impl Into<String>) -> Self {
        self.excerpt = Some(excerpt.into());
        self
    }

    /// Format as citation (APA-style)
    pub fn to_citation(&self) -> String {
        let mut citation = String::new();

        if let Some(author) = &self.author {
            citation.push_str(author);
            citation.push_str(". ");
        }

        if let Some(date) = &self.date {
            citation.push_str(&format!("({}). ", date));
        }

        citation.push_str(&self.title);

        if let Some(location) = &self.location {
            citation.push_str(&format!(". Retrieved from {}", location));
        }

        citation
    }
}

/// Builder for collecting source attributions
pub struct AttributionBuilder {
    attributions: Vec<SourceAttribution>,
}

impl AttributionBuilder {
    /// Create a new attribution builder
    pub fn new() -> Self {
        Self {
            attributions: Vec::new(),
        }
    }

    /// Add an attribution
    pub fn add(&mut self, attribution: SourceAttribution) {
        self.attributions.push(attribution);
    }

    /// Add a simple attribution
    pub fn add_simple(
        &mut self,
        source_type: SourceType,
        title: impl Into<String>,
        attribution_score: f64,
    ) {
        let source = Source::new(source_type, title);
        let attribution = SourceAttribution::new(source, attribution_score, attribution_score);
        self.attributions.push(attribution);
    }

    /// Build and sort by importance
    pub fn build(mut self) -> Vec<SourceAttribution> {
        // Sort by importance (highest first)
        self.attributions.sort_by(|a, b| {
            b.importance_score()
                .partial_cmp(&a.importance_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.attributions
    }

    /// Get top N attributions
    pub fn top_n(self, n: usize) -> Vec<SourceAttribution> {
        let mut attributions = self.build();
        attributions.truncate(n);
        attributions
    }
}

impl Default for AttributionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_creation() {
        let source = Source::new(SourceType::ResearchPaper, "Test Paper")
            .with_author("Dr. Smith")
            .with_date("2024")
            .with_location("https://example.com/paper");

        assert_eq!(source.title, "Test Paper");
        assert_eq!(source.author, Some("Dr. Smith".to_string()));
    }

    #[test]
    fn test_citation_format() {
        let source = Source::new(SourceType::ResearchPaper, "AI Explainability")
            .with_author("Smith, J.")
            .with_date("2024");

        let citation = source.to_citation();
        assert!(citation.contains("Smith, J."));
        assert!(citation.contains("2024"));
        assert!(citation.contains("AI Explainability"));
    }

    #[test]
    fn test_attribution_scoring() {
        let source = Source::new(SourceType::Manual, "User Guide");
        let attribution = SourceAttribution::new(source, 0.8, 0.9);

        assert_eq!(attribution.attribution_score, 0.8);
        assert_eq!(attribution.relevance_score, 0.9);
        assert!(attribution.importance_score() > 0.8);
    }

    #[test]
    fn test_attribution_builder() {
        let mut builder = AttributionBuilder::new();

        builder.add_simple(SourceType::ResearchPaper, "Paper 1", 0.9);
        builder.add_simple(SourceType::Manual, "Manual 1", 0.7);
        builder.add_simple(SourceType::Guideline, "Guideline 1", 0.95);

        let attributions = builder.build();

        // Should be sorted by importance
        assert_eq!(attributions.len(), 3);
        assert!(attributions[0].importance_score() >= attributions[1].importance_score());
        assert!(attributions[1].importance_score() >= attributions[2].importance_score());
    }
}
