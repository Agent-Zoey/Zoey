/*!
# PDF Parser

Extracts text content from PDF files.
*/

use super::{Document, DocumentMetadata, DocumentType};
use anyhow::Result;
use std::path::Path;

pub struct PdfParser;

impl PdfParser {
    /// Parse a PDF file
    pub fn parse(path: impl AsRef<Path>) -> Result<Document> {
        let path = path.as_ref();

        // Extract text from PDF
        let text = pdf_extract::extract_text(path)
            .map_err(|e| anyhow::anyhow!("Failed to extract PDF text: {}", e))?;

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled PDF")
            .to_string();

        let mut doc = Document::new(DocumentType::Pdf, title, text);

        doc.metadata.source_path = Some(path.display().to_string());

        // Try to get file metadata
        if let Ok(metadata) = std::fs::metadata(path) {
            if let Ok(modified) = metadata.modified() {
                doc.metadata.modified_at = Some(modified.into());
            }
        }

        Ok(doc)
    }

    /// Parse with custom metadata
    pub fn parse_with_metadata(
        path: impl AsRef<Path>,
        metadata: DocumentMetadata,
    ) -> Result<Document> {
        let mut doc = Self::parse(path)?;
        doc.metadata = metadata;
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_parser_struct() {
        // Just test that the struct exists
        let _parser = PdfParser;
    }
}
