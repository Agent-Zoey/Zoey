<p align="center">
  <img src="https://raw.githubusercontent.com/Agent-Zoey/Zoey/main/crates/assets/zoey-eye.png" alt="Zoey" width="400" />
</p>

<p align="center">
  <em>Always watching over your data</em>
</p>

# 🗄️ zoey-storage-sql

[![Crates.io](https://img.shields.io/crates/v/zoey-storage-sql.svg)](https://crates.io/crates/zoey-storage-sql)
[![Documentation](https://docs.rs/zoey-storage-sql/badge.svg)](https://docs.rs/zoey-storage-sql)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> **SQL storage adapters for ZoeyAI**

SQLite and PostgreSQL implementations of the `IDatabaseAdapter` trait from `zoey-core`. The default choice for most deployments.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
zoey-core = "0.1"
zoey-storage-sql = "0.1"
```

---

## Features

- 🪶 **SQLite** - Zero-config, embedded database (perfect for local/edge)
- 🐘 **PostgreSQL** - Production-ready with full SQL capabilities
- 🔍 **Vector Search** - pgvector support for PostgreSQL
- 📊 **Migrations** - Automatic schema management

---

## Quick Start

### SQLite (Recommended for Development)

```rust
use zoey_core::{AgentRuntime, RuntimeOpts, IDatabaseAdapter};
use zoey_storage_sql::SqliteAdapter;
use std::sync::Arc;

#[tokio::main]
async fn main() -> zoey_core::Result<()> {
    // In-memory database
    let mut adapter = SqliteAdapter::new(":memory:");
    
    // Or file-based
    // let mut adapter = SqliteAdapter::new("./zoey.db");
    
    adapter.init().await?;
    
    let opts = RuntimeOpts::default()
        .with_adapter(Arc::new(adapter));
    
    let runtime = AgentRuntime::new(opts).await?;
    
    Ok(())
}
```

### PostgreSQL (Recommended for Production)

```rust
use zoey_core::{AgentRuntime, RuntimeOpts, IDatabaseAdapter};
use zoey_storage_sql::PostgresAdapter;
use std::sync::Arc;

#[tokio::main]
async fn main() -> zoey_core::Result<()> {
    let mut adapter = PostgresAdapter::new(
        "postgres://user:password@localhost:5432/zoey"
    );
    
    adapter.init().await?;
    
    let opts = RuntimeOpts::default()
        .with_adapter(Arc::new(adapter));
    
    let runtime = AgentRuntime::new(opts).await?;
    
    Ok(())
}
```

---

## Configuration

### Environment Variables

```bash
# SQLite
ZOEY_DB_PATH=./zoey.db

# PostgreSQL
DATABASE_URL=postgres://user:password@localhost:5432/zoey
```

### Connection Pooling (PostgreSQL)

```rust
let adapter = PostgresAdapter::new_with_pool_size(
    "postgres://localhost/zoey",
    max_connections: 10,
);
```

---

## Vector Search

### PostgreSQL with pgvector

```sql
-- Enable pgvector (run once)
CREATE EXTENSION IF NOT EXISTS vector;
```

### SQLite

SQLite uses a simple cosine similarity implementation for vector search. For production vector workloads, consider PostgreSQL with pgvector or `zoey-storage-vector`.

---

## When to Use Which?

| Scenario | Recommended |
|----------|-------------|
| Local development | SQLite |
| Edge/embedded deployment | SQLite |
| Production web service | PostgreSQL |
| High-volume vector search | PostgreSQL + pgvector |
| Serverless | Supabase or SQLite |

---

## Related Crates

| Crate | Description |
|-------|-------------|
| [`zoey-core`](https://crates.io/crates/zoey-core) | Core runtime and `IDatabaseAdapter` trait |
| [`zoey-storage-mongo`](https://crates.io/crates/zoey-storage-mongo) | MongoDB adapter |
| [`zoey-storage-supabase`](https://crates.io/crates/zoey-storage-supabase) | Supabase adapter |
| [`zoey-storage-vector`](https://crates.io/crates/zoey-storage-vector) | Local vector storage |

---

## License

MIT License - See [LICENSE](https://github.com/Agent-Zoey/Zoey/blob/main/LICENSE) for details.

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
