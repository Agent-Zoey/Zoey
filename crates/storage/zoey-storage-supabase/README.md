<p align="center">
  <img src="https://raw.githubusercontent.com/Agent-Zoey/Zoey/main/crates/assets/zoey-eye.png" alt="Zoey" width="400" />
</p>

<p align="center">
  <em>Always watching over your data</em>
</p>

# 🗄️ zoey-storage-supabase

[![Crates.io](https://img.shields.io/crates/v/zoey-storage-supabase.svg)](https://crates.io/crates/zoey-storage-supabase)
[![Documentation](https://docs.rs/zoey-storage-supabase/badge.svg)](https://docs.rs/zoey-storage-supabase)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> **Supabase storage adapter for ZoeyAI**

A Supabase implementation of the `IDatabaseAdapter` trait from `zoey-core`. Leverages Supabase's PostgreSQL backend with REST API integration.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
zoey-core = "0.1"
zoey-storage-supabase = "0.1"
```

---

## Features

- ⚡ **Supabase Native** - REST API and direct PostgreSQL access
- 🔍 **Vector Search** - pgvector integration for similarity search
- 🔐 **Row Level Security** - Supabase RLS support
- 📊 **Real-time** - Ready for Supabase real-time subscriptions

---

## Quick Start

```rust
use zoey_core::{AgentRuntime, RuntimeOpts, IDatabaseAdapter};
use zoey_storage_supabase::{SupabaseAdapter, SupabaseConfig};
use std::sync::Arc;

#[tokio::main]
async fn main() -> zoey_core::Result<()> {
    // Create Supabase adapter
    let config = SupabaseConfig {
        url: "https://your-project.supabase.co".to_string(),
        api_key: std::env::var("SUPABASE_KEY").unwrap(),
        ..Default::default()
    };
    
    let mut adapter = SupabaseAdapter::new(config);
    
    // Initialize
    adapter.init().await?;
    
    // Use with runtime
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
SUPABASE_URL=https://your-project.supabase.co
SUPABASE_KEY=your-anon-or-service-key
```

### Direct PostgreSQL Connection

For direct database access (bypassing REST API):

```rust
let config = SupabaseConfig {
    url: "postgresql://postgres:password@db.your-project.supabase.co:5432/postgres".to_string(),
    api_key: "".to_string(),
    use_direct_connection: true,
    ..Default::default()
};
```

---

## Vector Search with pgvector

Supabase supports pgvector for vector similarity search:

```sql
-- Enable pgvector (run once in Supabase SQL editor)
CREATE EXTENSION IF NOT EXISTS vector;
```

The adapter automatically handles vector operations when pgvector is enabled.

---

## Related Crates

| Crate | Description |
|-------|-------------|
| [`zoey-core`](https://crates.io/crates/zoey-core) | Core runtime and `IDatabaseAdapter` trait |
| [`zoey-storage-sql`](https://crates.io/crates/zoey-storage-sql) | SQLite & PostgreSQL adapters |
| [`zoey-storage-mongo`](https://crates.io/crates/zoey-storage-mongo) | MongoDB adapter |
| [`zoey-storage-vector`](https://crates.io/crates/zoey-storage-vector) | Local vector storage |

---

## License

MIT License - See [LICENSE](https://github.com/Agent-Zoey/Zoey/blob/main/LICENSE) for details.

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
