<p align="center">
  <img src="../../assets/zoey-confident.png" alt="Zoey" width="250" />
</p>

# 🗄️ zoey-storage-vector

> **Your secrets are safe with Zoey**

Local vector storage for ZoeyOS—optimized for constrained hardware with zero cloud dependencies. Keep your embeddings private and fast.

## Status: ✅ Beta

---

## Features

### 🔒 Privacy First
- All vectors stored locally
- No external API calls
- Works completely offline
- Full data ownership

### ⚡ Performance
- SIMD-accelerated similarity search
- Memory-mapped file support
- Efficient indexing structures
- Configurable caching

### 💾 Storage Options

| Backend | Description | Best For |
|---------|-------------|----------|
| **In-Memory** | Pure RAM storage | Development, small datasets |
| **SQLite** | Persistent local file | Single-node production |
| **Memory-Mapped** | File-backed with mmap | Large datasets |

---

## Quick Start

```rust
use zoey_storage_vector::LocalVectorPlugin;

let plugin = LocalVectorPlugin::new();

// Store vectors
plugin.insert("doc-1", embedding_vector).await?;

// Search
let results = plugin.search(&query_vector, 10).await?;
for result in results {
    println!("{}: {:.4}", result.id, result.similarity);
}
```

---

## Usage Examples

### Basic Operations

```rust
use zoey_storage_vector::{LocalVectorPlugin, VectorConfig};

let config = VectorConfig {
    dimensions: 384,  // Must match your embedding model
    storage_path: "./vectors".to_string(),
    ..Default::default()
};

let store = LocalVectorPlugin::with_config(config);

// Insert a vector
let embedding = vec![0.1, 0.2, 0.3, /* ... */];
store.insert("document-1", embedding).await?;

// Insert with metadata
store.insert_with_metadata(
    "document-2",
    embedding,
    json!({"source": "manual", "page": 42}),
).await?;

// Search
let query = vec![0.15, 0.25, 0.35, /* ... */];
let results = store.search(&query, 10).await?;

// Delete
store.delete("document-1").await?;
```

### Batch Operations

```rust
// Insert many vectors at once
let vectors: Vec<(String, Vec<f32>)> = documents
    .iter()
    .map(|doc| (doc.id.clone(), embed(doc.text)))
    .collect();

store.insert_batch(vectors).await?;

// Search with filters
let results = store.search_with_filter(
    &query,
    10,
    Filter::Eq("source", "manual"),
).await?;
```

### Collection Management

```rust
// Create named collection
store.create_collection("medical_docs", 384).await?;

// Insert into collection
store.insert_to_collection(
    "medical_docs",
    "doc-123",
    embedding,
).await?;

// Search specific collection
let results = store.search_collection(
    "medical_docs",
    &query,
    10,
).await?;

// List collections
let collections = store.list_collections().await?;

// Delete collection
store.delete_collection("old_collection").await?;
```

---

## Configuration

```rust
use zoey_storage_vector::{VectorConfig, StorageBackend, DistanceMetric};

let config = VectorConfig {
    // Vector dimensions (must match embedding model)
    dimensions: 384,
    
    // Storage backend
    backend: StorageBackend::SQLite,
    storage_path: "./.zoey/db/vectors".to_string(),
    
    // Distance metric
    metric: DistanceMetric::Cosine,  // or Euclidean, DotProduct
    
    // Performance tuning
    cache_size_mb: 256,
    enable_mmap: true,
    
    // Indexing
    enable_indexing: true,
    index_type: IndexType::HNSW,
    
    ..Default::default()
};
```

### Distance Metrics

| Metric | Description | Use Case |
|--------|-------------|----------|
| **Cosine** | Angle between vectors | Text similarity (normalized) |
| **Euclidean** | Straight-line distance | Spatial similarity |
| **DotProduct** | Inner product | Pre-normalized vectors |

---

## Performance Benchmarks

Tested on Raspberry Pi 4 (4GB RAM):

| Operation | 10K vectors | 100K vectors | 1M vectors |
|-----------|-------------|--------------|------------|
| Insert | 0.1ms | 0.2ms | 0.5ms |
| Search (top-10) | 2ms | 15ms | 150ms |
| Batch insert (1K) | 50ms | 60ms | 80ms |

**Memory Usage:**
- ~400 bytes per 384-dim vector
- 10K vectors ≈ 4 MB
- 100K vectors ≈ 40 MB
- 1M vectors ≈ 400 MB

---

## Integration with Embeddings

### With Local Embeddings

```rust
use zoey_provider_local::LocalLLMPlugin;
use zoey_storage_vector::LocalVectorPlugin;

let llm = LocalLLMPlugin::new();
let store = LocalVectorPlugin::new();

// Generate and store embeddings
let text = "Important document content";
let embedding = llm.embed(text).await?;
store.insert("doc-1", embedding).await?;

// Search
let query_embedding = llm.embed("search query").await?;
let results = store.search(&query_embedding, 10).await?;
```

### With Knowledge Plugin

```rust
use zoey_plugin_knowledge::KnowledgePlugin;
use zoey_storage_vector::LocalVectorPlugin;

// Knowledge plugin uses vector storage internally
let knowledge = KnowledgePlugin::with_vector_store(
    Arc::new(LocalVectorPlugin::new())
);
```

---

## Troubleshooting

### Slow Searches

- Enable indexing: `enable_indexing: true`
- Use HNSW index for large datasets
- Reduce search radius with filters
- Increase cache size

### High Memory Usage

- Enable memory mapping: `enable_mmap: true`
- Use SQLite backend for persistence
- Reduce cache size
- Consider dimensionality reduction

### Accuracy Issues

- Ensure consistent embedding model
- Check dimension mismatch
- Verify distance metric matches training

---

## Dependencies

- `zoey-core` - Core runtime and types

---

## Testing

```bash
cargo test -p zoey-storage-vector
```

---

## License

MIT License

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
