/*!
# JSON Parser

Parses JSON files into searchable documents.
*/

use super::{Document, DocumentType};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct JsonParser;

impl JsonParser {
    /// Parse a JSON file
    pub fn parse(path: impl AsRef<Path>) -> Result<Document> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled JSON")
            .to_string();

        // Parse JSON to validate
        let json: serde_json::Value = serde_json::from_str(&content)?;

        // Flatten JSON to searchable text
        let searchable_text = Self::flatten_json(&json);

        let mut doc = Document::new(DocumentType::Json, title, searchable_text);
        doc.metadata.source_path = Some(path.display().to_string());
        doc.metadata.custom = json;

        Ok(doc)
    }

    /// Flatten JSON structure into searchable text
    fn flatten_json(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => String::from("null"),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => arr
                .iter()
                .map(Self::flatten_json)
                .collect::<Vec<_>>()
                .join(" "),
            serde_json::Value::Object(obj) => {
                let mut parts = Vec::new();
                for (key, val) in obj {
                    parts.push(format!("{}: {}", key, Self::flatten_json(val)));
                }
                parts.join("\n")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_flattening() {
        let json = serde_json::json!({
            "name": "Test",
            "age": 25,
            "tags": ["rust", "ai"]
        });

        let text = JsonParser::flatten_json(&json);
        assert!(text.contains("name"));
        assert!(text.contains("Test"));
        assert!(text.contains("rust"));
    }
}
