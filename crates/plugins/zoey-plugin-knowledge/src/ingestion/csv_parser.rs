/*!
# CSV Parser

Parses CSV files into structured documents.
*/

use super::{Document, DocumentType};
use anyhow::Result;
use csv::Reader;
use std::fs::File;
use std::path::Path;

pub struct CsvParser;

impl CsvParser {
    /// Parse a CSV file
    pub fn parse(path: impl AsRef<Path>) -> Result<Document> {
        let path = path.as_ref();
        let file = File::open(path)?;
        let mut reader = Reader::from_reader(file);

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled CSV")
            .to_string();

        let mut content = String::new();
        let headers = reader.headers()?.clone();

        // Add headers
        content.push_str(&headers.iter().collect::<Vec<_>>().join(", "));
        content.push_str("\n\n");

        // Add rows
        for result in reader.records() {
            let record = result?;
            content.push_str(&record.iter().collect::<Vec<_>>().join(", "));
            content.push('\n');
        }

        let mut doc = Document::new(DocumentType::Csv, title, content);
        doc.metadata.source_path = Some(path.display().to_string());

        Ok(doc)
    }

    /// Parse CSV with custom delimiter
    pub fn parse_with_delimiter(path: impl AsRef<Path>, delimiter: u8) -> Result<Document> {
        let path = path.as_ref();
        let file = File::open(path)?;
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .from_reader(file);

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled CSV")
            .to_string();

        let mut content = String::new();
        let headers = reader.headers()?.clone();

        content.push_str(&headers.iter().collect::<Vec<_>>().join(", "));
        content.push_str("\n\n");

        for result in reader.records() {
            let record = result?;
            content.push_str(&record.iter().collect::<Vec<_>>().join(", "));
            content.push('\n');
        }

        let mut doc = Document::new(DocumentType::Csv, title, content);
        doc.metadata.source_path = Some(path.display().to_string());

        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_parser_struct() {
        let _parser = CsvParser;
    }
}
