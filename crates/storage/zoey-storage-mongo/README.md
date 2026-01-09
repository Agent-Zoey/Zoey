<p align="center">
  <img src="https://raw.githubusercontent.com/Agent-Zoey/Zoey/main/crates/assets/zoey-eye.png" alt="Zoey" width="400" />
</p>

<p align="center">
  <em>Always watching over your data</em>
</p>

# 🗄️ zoey-storage-mongo

[![Crates.io](https://img.shields.io/crates/v/zoey-storage-mongo.svg)](https://crates.io/crates/zoey-storage-mongo)
[![Documentation](https://docs.rs/zoey-storage-mongo/badge.svg)](https://docs.rs/zoey-storage-mongo)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> **MongoDB storage adapter for ZoeyAI**

A MongoDB implementation of the `IDatabaseAdapter` trait from `zoey-core`. Provides document-based storage with local vector search capabilities.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
zoey-core = "0.1"
zoey-storage-mongo = "0.1"
```

---

## Features

- 🍃 **MongoDB Native** - Full MongoDB driver integration
- 🔍 **Vector Search** - Local cosine similarity search (no Atlas required)
- 📊 **Schema Management** - Automatic collection and index creation
- 🔐 **HIPAA Ready** - Optional compliance features

---

## Quick Start

```rust
use zoey_core::{AgentRuntime, RuntimeOpts, IDatabaseAdapter};
use zoey_storage_mongo::MongoAdapter;
use std::sync::Arc;

#[tokio::main]
async fn main() -> zoey_core::Result<()> {
    // Create MongoDB adapter
    let mut adapter = MongoAdapter::new(
        "mongodb://localhost:27017",
        "zoey_db"
    );
    
    // Initialize (creates collections and indexes)
    adapter.init().await?;
    
    // Use with runtime
    let opts = RuntimeOpts::default()
        .with_adapter(Arc::new(adapter));
    
    let runtime = AgentRuntime::new(opts).await?;
    
    Ok(())
}
```

---

## Vector Search

This adapter implements local vector search using MongoDB aggregation pipelines with cosine similarity calculation—**no MongoDB Atlas required**.

```rust
use zoey_storage_mongo::vector_search::MongoVectorSearch;

let searcher = MongoVectorSearch::new(collection, 1536);

let results = searcher.search(SearchMemoriesParams {
    embedding: query_vector,
    count: 10,
    ..Default::default()
}).await?;
```

---

## Configuration

### Environment Variables

```bash
MONGODB_URI=mongodb://localhost:27017
MONGODB_DATABASE=zoey_db
```

### Connection Options

```rust
// Local development
let adapter = MongoAdapter::new("mongodb://localhost:27017", "zoey_db");

// With authentication
let adapter = MongoAdapter::new(
    "mongodb://user:password@localhost:27017",
    "zoey_db"
);

// Replica set
let adapter = MongoAdapter::new(
    "mongodb://host1:27017,host2:27017,host3:27017/?replicaSet=rs0",
    "zoey_db"
);
```

---

## Related Crates

| Crate | Description |
|-------|-------------|
| [`zoey-core`](https://crates.io/crates/zoey-core) | Core runtime and `IDatabaseAdapter` trait |
| [`zoey-storage-sql`](https://crates.io/crates/zoey-storage-sql) | SQLite & PostgreSQL adapters |
| [`zoey-storage-supabase`](https://crates.io/crates/zoey-storage-supabase) | Supabase adapter |
| [`zoey-storage-vector`](https://crates.io/crates/zoey-storage-vector) | Local vector storage |

---

## License

MIT License - See [LICENSE](https://github.com/Agent-Zoey/Zoey/blob/main/LICENSE) for details.

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
