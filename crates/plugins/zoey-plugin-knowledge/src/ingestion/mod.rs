/*!
# Document Ingestion Module

Handles parsing and processing of various document formats.
*/

pub mod csv_parser;
pub mod json;
pub mod markdown;
pub mod pdf;
pub mod text;

pub use csv_parser::CsvParser;
pub use json::JsonParser;
pub use markdown::MarkdownParser;
pub use pdf::PdfParser;
pub use text::TextParser;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

/// Supported document types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocumentType {
    Pdf,
    Markdown,
    Csv,
    Json,
    Text,
}

/// A parsed document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Unique identifier
    pub id: Uuid,

    /// Document type
    pub doc_type: DocumentType,

    /// Title or filename
    pub title: String,

    /// Parsed content (plain text)
    pub content: String,

    /// Structured sections (for documents with headings)
    pub sections: Vec<DocumentSection>,

    /// Metadata
    pub metadata: DocumentMetadata,

    /// When ingested
    pub ingested_at: DateTime<Utc>,

    /// Word count
    pub word_count: usize,

    /// Character count
    pub char_count: usize,
}

/// A section within a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSection {
    /// Section title/heading
    pub title: String,

    /// Section content
    pub content: String,

    /// Hierarchy level (1 = top-level, 2 = subsection, etc.)
    pub level: usize,

    /// Child sections
    pub children: Vec<DocumentSection>,
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Original file path
    pub source_path: Option<String>,

    /// Author
    pub author: Option<String>,

    /// Creation date
    pub created_at: Option<DateTime<Utc>>,

    /// Last modified
    pub modified_at: Option<DateTime<Utc>>,

    /// Language (ISO 639-1 code)
    pub language: Option<String>,

    /// Keywords/tags
    pub keywords: Vec<String>,

    /// Additional metadata
    pub custom: serde_json::Value,
}

impl Default for DocumentMetadata {
    fn default() -> Self {
        Self {
            source_path: None,
            author: None,
            created_at: None,
            modified_at: None,
            language: Some("en".to_string()),
            keywords: Vec::new(),
            custom: serde_json::Value::Null,
        }
    }
}

impl Document {
    /// Create a new document
    pub fn new(
        doc_type: DocumentType,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let content = content.into();
        let word_count = content.split_whitespace().count();
        let char_count = content.chars().count();

        Self {
            id: Uuid::new_v4(),
            doc_type,
            title: title.into(),
            content,
            sections: Vec::new(),
            metadata: DocumentMetadata::default(),
            ingested_at: Utc::now(),
            word_count,
            char_count,
        }
    }

    /// Add a section
    pub fn add_section(&mut self, section: DocumentSection) {
        self.sections.push(section);
    }

    /// Get all text content (including sections)
    pub fn full_text(&self) -> String {
        let mut text = self.content.clone();

        for section in &self.sections {
            text.push_str("\n\n");
            text.push_str(&section.full_text());
        }

        text
    }

    /// Extract sentences
    pub fn sentences(&self) -> Vec<String> {
        // Simple sentence splitting (could be improved with NLP)
        self.content
            .split(|c| c == '.' || c == '!' || c == '?')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Extract paragraphs
    pub fn paragraphs(&self) -> Vec<String> {
        self.content
            .split("\n\n")
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    }
}

impl DocumentSection {
    /// Create a new section
    pub fn new(title: impl Into<String>, content: impl Into<String>, level: usize) -> Self {
        Self {
            title: title.into(),
            content: content.into(),
            level,
            children: Vec::new(),
        }
    }

    /// Add a child section
    pub fn add_child(&mut self, child: DocumentSection) {
        self.children.push(child);
    }

    /// Get full text including children
    pub fn full_text(&self) -> String {
        let mut text = format!("{}\n{}", self.title, self.content);

        for child in &self.children {
            text.push_str("\n\n");
            text.push_str(&child.full_text());
        }

        text
    }
}

/// Document ingestion pipeline
pub struct DocumentIngestionPipeline {
    documents: Vec<Document>,
    config: crate::KnowledgeConfig,
}

impl DocumentIngestionPipeline {
    /// Create a new ingestion pipeline
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            config: crate::KnowledgeConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: crate::KnowledgeConfig) -> Self {
        Self {
            documents: Vec::new(),
            config,
        }
    }

    /// Ingest a file
    pub async fn ingest_file(
        &mut self,
        path: impl AsRef<Path>,
        doc_type: DocumentType,
    ) -> Result<Uuid> {
        let path = path.as_ref();

        let document = match doc_type {
            DocumentType::Pdf if self.config.enable_pdf => PdfParser::parse(path)?,
            DocumentType::Markdown if self.config.enable_markdown => MarkdownParser::parse(path)?,
            DocumentType::Csv if self.config.enable_csv => CsvParser::parse(path)?,
            DocumentType::Json if self.config.enable_json => JsonParser::parse(path)?,
            DocumentType::Text => TextParser::parse(path)?,
            _ => anyhow::bail!("Document type {:?} is not enabled", doc_type),
        };

        let id = document.id;
        self.documents.push(document);
        Ok(id)
    }

    /// Ingest from string content
    pub fn ingest_string(
        &mut self,
        title: impl Into<String>,
        content: impl Into<String>,
        doc_type: DocumentType,
    ) -> Uuid {
        let doc = Document::new(doc_type, title, content);
        let id = doc.id;
        self.documents.push(doc);
        id
    }

    /// Get all documents
    pub fn documents(&self) -> &[Document] {
        &self.documents
    }

    /// Get document by ID
    pub fn get_document(&self, id: &Uuid) -> Option<&Document> {
        self.documents.iter().find(|d| &d.id == id)
    }

    /// Build a searchable corpus
    pub fn corpus(&self) -> Vec<String> {
        self.documents.iter().map(|d| d.full_text()).collect()
    }

    /// Get total word count across all documents
    pub fn total_words(&self) -> usize {
        self.documents.iter().map(|d| d.word_count).sum()
    }

    /// Get statistics
    pub fn statistics(&self) -> IngestionStatistics {
        IngestionStatistics {
            total_documents: self.documents.len(),
            total_words: self.total_words(),
            total_characters: self.documents.iter().map(|d| d.char_count).sum(),
            by_type: self.count_by_type(),
        }
    }

    fn count_by_type(&self) -> std::collections::HashMap<DocumentType, usize> {
        let mut counts = std::collections::HashMap::new();
        for doc in &self.documents {
            *counts.entry(doc.doc_type).or_insert(0) += 1;
        }
        counts
    }
}

impl Default for DocumentIngestionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about ingested documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionStatistics {
    pub total_documents: usize,
    pub total_words: usize,
    pub total_characters: usize,
    pub by_type: std::collections::HashMap<DocumentType, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_creation() {
        let doc = Document::new(
            DocumentType::Text,
            "Test Document",
            "This is a test. It has multiple sentences! Does it work?",
        );

        assert_eq!(doc.doc_type, DocumentType::Text);
        assert!(doc.word_count > 0);
        assert_eq!(doc.sentences().len(), 3);
    }

    #[test]
    fn test_pipeline() {
        let mut pipeline = DocumentIngestionPipeline::new();

        let id = pipeline.ingest_string("Test Doc", "Some content here.", DocumentType::Text);

        assert_eq!(pipeline.documents().len(), 1);
        assert!(pipeline.get_document(&id).is_some());

        let stats = pipeline.statistics();
        assert_eq!(stats.total_documents, 1);
    }

    #[test]
    fn test_sections() {
        let mut section = DocumentSection::new("Introduction", "This is the intro", 1);
        let subsection = DocumentSection::new("Background", "Background info", 2);

        section.add_child(subsection);
        assert_eq!(section.children.len(), 1);

        let text = section.full_text();
        assert!(text.contains("Introduction"));
        assert!(text.contains("Background"));
    }
}
