/*!
# Markdown Parser

Parses Markdown files with section extraction.
*/

use super::{Document, DocumentSection, DocumentType};
use anyhow::Result;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use std::fs;
use std::path::Path;

pub struct MarkdownParser;

impl MarkdownParser {
    /// Parse a Markdown file
    pub fn parse(path: impl AsRef<Path>) -> Result<Document> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled Markdown")
            .to_string();

        let mut doc = Document::new(DocumentType::Markdown, title, String::new());

        // Parse markdown and extract structure
        let (plain_text, sections) = Self::parse_markdown(&content);

        doc.content = plain_text;
        doc.sections = sections;
        doc.word_count = doc.content.split_whitespace().count();
        doc.char_count = doc.content.chars().count();
        doc.metadata.source_path = Some(path.display().to_string());

        Ok(doc)
    }

    /// Parse markdown content into plain text and sections
    fn parse_markdown(markdown: &str) -> (String, Vec<DocumentSection>) {
        let parser = Parser::new(markdown);

        let mut plain_text = String::new();
        let mut sections = Vec::new();
        let mut current_section: Option<(String, String, usize)> = None;
        let mut in_heading = false;
        let mut current_heading = String::new();
        let mut current_level = 0;

        for event in parser {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    in_heading = true;
                    current_heading.clear();
                    current_level = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                }
                Event::End(TagEnd::Heading(_)) => {
                    in_heading = false;

                    // Save previous section
                    if let Some((title, content, level)) = current_section.take() {
                        sections.push(DocumentSection::new(title, content, level));
                    }

                    // Start new section
                    current_section = Some((current_heading.clone(), String::new(), current_level));
                }
                Event::Text(text) => {
                    if in_heading {
                        current_heading.push_str(&text);
                    } else {
                        plain_text.push_str(&text);
                        plain_text.push(' ');

                        if let Some((_, ref mut content, _)) = current_section {
                            content.push_str(&text);
                            content.push(' ');
                        }
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    plain_text.push('\n');
                    if let Some((_, ref mut content, _)) = current_section {
                        content.push('\n');
                    }
                }
                _ => {}
            }
        }

        // Save last section
        if let Some((title, content, level)) = current_section {
            sections.push(DocumentSection::new(title, content, level));
        }

        (plain_text.trim().to_string(), sections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_parsing() {
        let markdown = r#"# Heading 1

Some content here.

## Heading 2

More content.

### Heading 3

Even more content.
"#;

        let (plain_text, sections) = MarkdownParser::parse_markdown(markdown);

        assert!(plain_text.contains("Some content"));
        assert!(plain_text.contains("More content"));
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].level, 1);
        assert_eq!(sections[1].level, 2);
        assert_eq!(sections[2].level, 3);
    }

    #[test]
    fn test_section_hierarchy() {
        let markdown = "# Main\nContent\n## Sub\nMore";
        let (_, sections) = MarkdownParser::parse_markdown(markdown);

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, "Main");
        assert_eq!(sections[1].title, "Sub");
    }
}
