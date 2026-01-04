/*!
# Text Parser

Simple plain text file parser.
*/

use super::{Document, DocumentType};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct TextParser;

impl TextParser {
    /// Parse a text file
    pub fn parse(path: impl AsRef<Path>) -> Result<Document> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled Text")
            .to_string();

        let mut doc = Document::new(DocumentType::Text, title, content);
        doc.metadata.source_path = Some(path.display().to_string());

        Ok(doc)
    }

    /// Parse from string
    pub fn parse_string(title: impl Into<String>, content: impl Into<String>) -> Document {
        Document::new(DocumentType::Text, title, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_parser() {
        let doc = TextParser::parse_string("Test", "Hello world");
        assert_eq!(doc.title, "Test");
        assert_eq!(doc.content, "Hello world");
        assert!(doc.word_count > 0);
    }
}
