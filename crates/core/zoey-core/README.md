<p align="center">
  <img src="https://raw.githubusercontent.com/Agent-Zoey/Zoey/main/crates/assets/zoey-eye.png" alt="Zoey" width="400" />
</p>

<p align="center">
  <em>Always watching over your code</em>
</p>

# 🔐 zoey-core

[![Crates.io](https://img.shields.io/crates/v/zoey-core.svg)](https://crates.io/crates/zoey-core)
[![Documentation](https://docs.rs/zoey-core/badge.svg)](https://docs.rs/zoey-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> **Your secrets are safe with Zoey**

The foundational crate powering ZoeyOS—providing the runtime, plugin system, type definitions, and agent API. Built for privacy-first AI agents optimized for local model deployment.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
zoey-core = "0.1"
```

---

## Features

### 🚀 Agent Runtime
- Async-first runtime built on Tokio
- Plugin lifecycle management
- Service orchestration
- Configuration management

### 🔌 Plugin System
- Dynamic plugin loading
- Action, provider, and evaluator registration
- Plugin dependency resolution
- Hot-reloading support (planned)

### 🎭 Agent API
- HTTP REST endpoints for agent interaction
- WebSocket support for real-time communication
- Authentication and authorization
- Rate limiting and request validation

### 📦 Core Types
- Memory and knowledge structures
- Message and conversation models
- Agent configuration and state
- Provider and action interfaces

---

## Quick Start

```rust
use zoey_core::{AgentRuntime, RuntimeOpts};

#[tokio::main]
async fn main() -> zoey_core::Result<()> {
    // Create runtime with default options
    let opts = RuntimeOpts::default();
    let runtime = AgentRuntime::new(opts).await?;
    
    // Zoey is ready to serve
    println!("🔐 Zoey runtime initialized");
    
    Ok(())
}
```

## With Plugins

```rust
use zoey_core::{AgentRuntime, RuntimeOpts};
use std::sync::Arc;

let mut opts = RuntimeOpts::default();

// Add plugins
opts.add_plugin(Arc::new(BootstrapPlugin::new()));
opts.add_plugin(Arc::new(KnowledgePlugin::new()));
opts.add_plugin(Arc::new(CompliancePlugin::new()));

let runtime = AgentRuntime::new(opts).await?;
```

---

## Architecture

```
zoey-core
├── runtime/          # Agent runtime and lifecycle
│   ├── mod.rs        # AgentRuntime implementation
│   ├── opts.rs       # RuntimeOpts configuration
│   └── services.rs   # Service orchestration
├── plugin/           # Plugin system
│   ├── mod.rs        # Plugin trait and registry
│   ├── loader.rs     # Dynamic plugin loading
│   └── context.rs    # Plugin execution context
├── agent_api/        # HTTP/WebSocket API
│   ├── handlers.rs   # Request handlers
│   ├── routes.rs     # Route definitions
│   └── auth.rs       # Authentication
├── types/            # Core type definitions
│   ├── memory.rs     # Memory structures
│   ├── message.rs    # Message models
│   └── agent.rs      # Agent configuration
└── lib.rs            # Public API exports
```

---

## Core Traits

### Plugin Trait

```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    
    async fn init(&self, runtime: &AgentRuntime) -> Result<()>;
    
    fn actions(&self) -> Vec<Arc<dyn Action>>;
    fn providers(&self) -> Vec<Arc<dyn Provider>>;
    fn evaluators(&self) -> Vec<Arc<dyn Evaluator>>;
}
```

### Action Trait

```rust
#[async_trait]
pub trait Action: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn examples(&self) -> Vec<Example>;
    
    async fn validate(&self, input: &ActionInput) -> Result<bool>;
    async fn execute(&self, ctx: ActionContext) -> Result<ActionOutput>;
}
```

### Provider Trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    
    async fn get(&self, ctx: &ProviderContext) -> Result<ProviderOutput>;
}
```

---

## Configuration

### Environment Variables

```bash
# Runtime configuration
ZOEY_LOG_LEVEL=info
ZOEY_MAX_WORKERS=4
ZOEY_PLUGIN_DIR=./plugins

# API configuration
ZOEY_API_HOST=127.0.0.1
ZOEY_API_PORT=3000
ZOEY_API_CORS_ORIGINS=http://localhost:3000

# Storage configuration
ZOEY_DATA_DIR=./.zoey/db
ZOEY_DB_PATH=./.zoey/db/zoey.db
```

### Programmatic Configuration

```rust
let opts = RuntimeOpts {
    log_level: LogLevel::Info,
    max_workers: 4,
    plugin_dir: PathBuf::from("./plugins"),
    api_host: "127.0.0.1".to_string(),
    api_port: 3000,
    ..Default::default()
};
```

---

## Storage Adapters

`zoey-core` defines the `IDatabaseAdapter` trait. Choose your storage backend:

| Crate | Backend | Best For |
|-------|---------|----------|
| [`zoey-storage-sql`](https://crates.io/crates/zoey-storage-sql) | SQLite, PostgreSQL | Most deployments |
| [`zoey-storage-mongo`](https://crates.io/crates/zoey-storage-mongo) | MongoDB | Document-based storage |
| [`zoey-storage-supabase`](https://crates.io/crates/zoey-storage-supabase) | Supabase | Serverless PostgreSQL |
| [`zoey-storage-vector`](https://crates.io/crates/zoey-storage-vector) | Local | Dedicated vector search |

```toml
[dependencies]
zoey-core = "0.1"
zoey-storage-sql = "0.1"  # Pick your backend
```

```rust
use zoey_core::{AgentRuntime, RuntimeOpts, IDatabaseAdapter};
use zoey_storage_sql::SqliteAdapter;
use std::sync::Arc;

let mut adapter = SqliteAdapter::new(":memory:");
adapter.init().await?;

let opts = RuntimeOpts::default()
    .with_adapter(Arc::new(adapter));
```

---

## Ecosystem

### Providers
| Crate | Description |
|-------|-------------|
| `zoey-provider-openai` | OpenAI GPT models |
| `zoey-provider-anthropic` | Claude models |
| `zoey-provider-local` | Ollama, llama.cpp (no API key!) |
| `zoey-provider-voice` | TTS/STT capabilities |

### Plugins
| Crate | Description |
|-------|-------------|
| `zoey-plugin-bootstrap` | Essential actions and providers |
| `zoey-plugin-memory` | Conversation memory |
| `zoey-plugin-knowledge` | Document ingestion and RAG |
| `zoey-plugin-search` | Web search integration |

### Adaptors
| Crate | Description |
|-------|-------------|
| `zoey-adaptor-discord` | Discord bot |
| `zoey-adaptor-telegram` | Telegram bot |
| `zoey-adaptor-web` | Web UI |
| `zoey-adaptor-terminal` | CLI interface |

---

## Dependencies

This crate has minimal external dependencies:

- `tokio` - Async runtime
- `axum` - HTTP framework
- `sqlx` - Database access
- `serde` - Serialization
- `tracing` - Logging and diagnostics

---

## Testing

```bash
# Run unit tests
cargo test -p zoey-core

# Run with logging
RUST_LOG=debug cargo test -p zoey-core -- --nocapture

# Run specific test
cargo test -p zoey-core test_runtime_init
```

---

## License

MIT License - See [LICENSE](https://github.com/Agent-Zoey/Zoey/blob/main/LICENSE) for details.

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
