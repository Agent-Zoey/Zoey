<p align="center">
  <img src="../../assets/zoey-curious.png" alt="Zoey" width="300" />
</p>

# 📚 zoey-plugin-knowledge

> **Your secrets are safe with Zoey**

Transform Zoey into a domain expert. Ingest 1000-page technical manuals, build knowledge graphs, and answer domain questions entirely offline through hybrid retrieval.

## Status: ✅ Production

---

## Features

### 📄 Document Ingestion Pipeline

Parse and process multiple document formats:

| Format | Description |
|--------|-------------|
| **PDF** | Extract text from research papers, manuals, reports |
| **Markdown** | Parse with section hierarchy extraction |
| **CSV** | Structured data import |
| **JSON** | Nested object flattening |
| **Plain Text** | Universal fallback |

### 🕸️ Knowledge Graph

Build entity-relationship graphs with domain-specific ontologies:

- **Entity Extraction**: Automatic detection of persons, organizations, locations, medical terms
- **Relationship Detection**: Extract semantic relationships between entities
- **Ontologies**: Define domain-specific concept hierarchies and rules
- **Graph Queries**: Traverse relationships to find connected information

### 🔍 Hybrid Retrieval

Multi-strategy search combining:

| Strategy | Description | Speed |
|----------|-------------|-------|
| **Semantic** | Dense vector similarity | ~10ms |
| **BM25 Lexical** | Term-based ranking with stemming | 3.8 µs |
| **Graph-Based** | Entity relationship traversal | O(log n) |
| **Query Expansion** | Automatic synonym generation | ~5ms |
| **Re-Ranking** | Relevance optimization | ~2ms |

---

## Quick Start

```rust
use zoey_plugin_knowledge::KnowledgePlugin;
use std::sync::Arc;

let mut opts = RuntimeOpts::default();
opts.add_plugin(Arc::new(KnowledgePlugin::new()));

let runtime = AgentRuntime::new(opts).await?;

// Zoey can now ingest documents and answer domain questions
```

---

## Usage Examples

### Ingest Documents

```rust
use zoey_plugin_knowledge::{DocumentIngestionPipeline, DocumentType};

let mut pipeline = DocumentIngestionPipeline::new();

// Ingest various formats
pipeline.ingest_file("Manual_2000_pages.pdf", DocumentType::Pdf).await?;
pipeline.ingest_file("Procedures.md", DocumentType::Markdown).await?;
pipeline.ingest_file("Equipment_List.csv", DocumentType::Csv).await?;

println!("Ingested {} documents", pipeline.document_count());
```

### Build Knowledge Graph

```rust
use zoey_plugin_knowledge::{KnowledgeGraph, EntityExtractor, Entity, Relationship, RelationType};

let mut graph = KnowledgeGraph::new("medical");
let extractor = EntityExtractor::new();

// Extract entities from documents
for doc in pipeline.documents() {
    let entities = extractor.extract_entities(&doc.content)?;
    graph.add_entities(entities);
}

// Add custom relationships
let aspirin = graph.find_entity("Aspirin")?;
let headache = graph.find_entity("Headache")?;

graph.add_relationship(
    aspirin.id,
    headache.id,
    Relationship::new(RelationType::Treats, 0.9),
)?;

println!("Graph has {} entities, {} relationships", 
    graph.entity_count(), 
    graph.relationship_count()
);
```

### Hybrid Search

```rust
use zoey_plugin_knowledge::HybridRetriever;

let retriever = HybridRetriever::new(graph, pipeline.corpus());

let results = retriever.search("treatment for hypertension", 10).await?;

for result in results {
    println!("Confidence: {:.0}%", result.score * 100.0);
    println!("Text: {}", result.text);
    println!("Scores:");
    println!("  Semantic: {:.2}", result.scores.semantic_score);
    println!("  Lexical:  {:.2}", result.scores.lexical_score);
    println!("  Graph:    {:.2}", result.scores.graph_score);
    println!();
}
```

---

## Use Cases

### 🏥 Medical Knowledge Base

```rust
// Ingest medical guidelines
pipeline.ingest_file("CDC_Guidelines_2024.pdf", DocumentType::Pdf).await?;
pipeline.ingest_file("Treatment_Protocols.md", DocumentType::Markdown).await?;

// Build medical knowledge graph
let mut graph = KnowledgeGraph::new("medical");
let entities = extractor.extract_entities_with_domain(&content, "medical")?;
graph.add_entities(entities);

// Query treatment options (works offline)
let results = retriever.search("diabetes management guidelines", 5).await?;
```

### ⚖️ Legal Case Law Research

```rust
// Ingest case law documents
for case_file in case_files {
    pipeline.ingest_file(case_file, DocumentType::Pdf).await?;
}

// Build legal knowledge graph with precedent relationships
let mut graph = KnowledgeGraph::new("legal");

let smith_v_jones = Entity::new("Legal Case", "Smith v. Jones (2022)");
let precedent = Entity::new("Legal Case", "Brown v. Board (1954)");

graph.add_entity(smith_v_jones.clone());
graph.add_entity(precedent.clone());

graph.add_relationship(
    smith_v_jones.id,
    precedent.id,
    Relationship::new(RelationType::CitesPrecedent, 0.95),
)?;

// Search with citation tracking
let results = retriever.search("breach of contract damages", 5).await?;
```

### 🏭 Industrial Equipment Manuals

```rust
// Ingest 1000-page technical manual
pipeline.ingest_file("Machine_Manual_v3.pdf", DocumentType::Pdf).await?;

// Build equipment knowledge graph
let mut graph = KnowledgeGraph::new("industrial");

let bearing = Entity::new("Component", "Ball Bearing Assembly");
let lubrication = Entity::new("Procedure", "Lubrication Schedule");

graph.add_entity(bearing.clone());
graph.add_entity(lubrication.clone());

graph.add_relationship(
    bearing.id,
    lubrication.id,
    Relationship::new(RelationType::RequiresProcedure, 1.0),
)?;

// Offline troubleshooting queries
let results = retriever.search("bearing temperature high alarm", 10).await?;
```

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│              Knowledge Foundation               │
├─────────────────────────────────────────────────┤
│                                                 │
│  ┌─────────────┐    ┌──────────────────┐       │
│  │  Ingestion  │    │ Knowledge Graph  │       │
│  │             │    │                  │       │
│  │ • PDF       │    │ • Entities       │       │
│  │ • Markdown  │────│ • Relationships  │       │
│  │ • CSV/JSON  │    │ • Ontology       │       │
│  │ • Text      │    │ • Graph Queries  │       │
│  └─────────────┘    └──────────────────┘       │
│          │                    │                 │
│          └────────┬───────────┘                 │
│                   │                             │
│          ┌────────▼─────────┐                   │
│          │ Hybrid Retrieval │                   │
│          │                  │                   │
│          │ • Semantic       │                   │
│          │ • BM25 Lexical   │                   │
│          │ • Graph Search   │                   │
│          │ • Re-Ranking     │                   │
│          └──────────────────┘                   │
└─────────────────────────────────────────────────┘
```

---

## Performance

### Document Processing

| Operation | Speed |
|-----------|-------|
| PDF parsing | ~1000 pages/minute |
| Markdown parsing | ~5000 pages/minute |
| Entity extraction | ~10,000 words/second |

### Search Performance

| Operation | Latency |
|-----------|---------|
| BM25 search | 3.8 µs/query |
| Graph traversal | O(log n) |
| Hybrid search | ~10ms for 1000 docs |

### Memory Efficiency

| Component | Size |
|-----------|------|
| Knowledge graph | ~100 bytes/entity |
| BM25 index | ~50 KB/1000 docs |
| Total overhead | <10 MB for 1000-page manual |

---

## Configuration

```rust
use zoey_plugin_knowledge::{KnowledgePlugin, KnowledgeConfig};

let config = KnowledgeConfig {
    // Document formats
    enable_pdf: true,
    enable_markdown: true,
    enable_csv: true,
    enable_json: true,
    max_document_size: 100 * 1024 * 1024, // 100 MB
    
    // Entity extraction
    enable_entity_extraction: true,
    enable_relationship_detection: true,
    
    // Hybrid search weights (must sum to 1.0)
    semantic_weight: 0.4,   // Dense vector similarity
    lexical_weight: 0.4,    // BM25 keyword matching
    graph_weight: 0.2,      // Entity relationships
    
    ..Default::default()
};

let plugin = KnowledgePlugin::with_config(config);
```

---

## Providers

| Provider | Description |
|----------|-------------|
| `knowledge` | Access to knowledge graph and retrieval |
| `documents` | Ingested document metadata |
| `entities` | Entity lookup and traversal |

---

## Dependencies

- `zoey-core` - Core runtime and types

---

## Testing

```bash
cargo test -p zoey-plugin-knowledge
```

---

## License

MIT License

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
