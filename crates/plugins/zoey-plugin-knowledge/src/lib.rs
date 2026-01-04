/*!
# Knowledge Foundation Plugin for ZoeyAI

This plugin provides comprehensive knowledge management capabilities:

- **Document Ingestion**: Parse PDFs, Markdown, CSV, JSON, and text files
- **Knowledge Graph**: Build entity-relationship graphs with ontologies
- **Advanced Retrieval**: Hybrid search combining semantic, lexical, and graph-based retrieval

## Phase 1 Deliverable

**Goal**: Make the agent actually learn domain knowledge

Enable agents to ingest 1000-page technical manuals and answer domain questions offline.

## Example Usage

```rust
use zoey_plugin_knowledge::{
    DocumentIngestionPipeline, KnowledgeGraph, HybridRetriever,
    DocumentType, EntityExtractor
};

// 1. Ingest documents
let mut pipeline = DocumentIngestionPipeline::new();
#[cfg(feature = "knowledge_real")]
{
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let _ = pipeline.ingest_file("medical_guidelines.pdf", DocumentType::Pdf).await;
        let _ = pipeline.ingest_file("procedures.md", DocumentType::Markdown).await;
    });
}
#[cfg(not(feature = "knowledge_real"))]
{
    let _ = pipeline.ingest_file("medical_guidelines.pdf", DocumentType::Pdf);
    let _ = pipeline.ingest_file("procedures.md", DocumentType::Markdown);
}

// 2. Build knowledge graph
let mut graph = KnowledgeGraph::new("medical_domain".to_string());
let extractor = EntityExtractor::new();

for doc in pipeline.documents() {
    let entities = extractor.extract_entities(&doc.content).unwrap_or_default();
    graph.add_entities(entities);
}

// 3. Query with hybrid retrieval
let retriever = HybridRetriever::new(graph, pipeline.corpus());
#[cfg(feature = "knowledge_real")]
let results = {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async { retriever.search("treatment for hypertension", 10).await.unwrap_or_default() })
};
#[cfg(not(feature = "knowledge_real"))]
let results = {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async { retriever.search("treatment for hypertension", 10).await.unwrap_or_default() })
};

for result in results {
    println!("Score: {:.2} - {}", result.score, result.text);
}
```
*/

pub mod graph;
pub mod ingestion;
pub mod plugin;
pub mod retrieval;

pub use ingestion::{
    CsvParser, Document, DocumentIngestionPipeline, DocumentMetadata, DocumentType, JsonParser,
    MarkdownParser, PdfParser, TextParser,
};

pub use graph::{
    Entity, EntityExtractor, EntityType, KnowledgeGraph, Ontology, RelationType, Relationship,
    RelationshipDetector,
};

pub use retrieval::{
    BM25Search, GraphSearch, HybridRetriever, QueryExpansion, ReRanker, SearchResult,
    SemanticSearch,
};

pub use plugin::KnowledgePlugin;

use serde::{Deserialize, Serialize};

/// Configuration for knowledge management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeConfig {
    /// Enable PDF parsing
    pub enable_pdf: bool,

    /// Enable Markdown parsing
    pub enable_markdown: bool,

    /// Enable CSV parsing
    pub enable_csv: bool,

    /// Enable JSON parsing
    pub enable_json: bool,

    /// Maximum document size in bytes
    pub max_document_size: usize,

    /// Enable entity extraction
    pub enable_entity_extraction: bool,

    /// Enable relationship detection
    pub enable_relationship_detection: bool,

    /// Hybrid search weights
    pub semantic_weight: f64,
    pub lexical_weight: f64,
    pub graph_weight: f64,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            enable_pdf: true,
            enable_markdown: true,
            enable_csv: true,
            enable_json: true,
            max_document_size: 100 * 1024 * 1024, // 100 MB
            enable_entity_extraction: true,
            enable_relationship_detection: true,
            semantic_weight: 0.4,
            lexical_weight: 0.4,
            graph_weight: 0.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = KnowledgeConfig::default();
        assert!(config.enable_pdf);
        assert!(config.enable_entity_extraction);
        assert_eq!(
            config.semantic_weight + config.lexical_weight + config.graph_weight,
            1.0
        );
    }
}
