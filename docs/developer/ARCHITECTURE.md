<p align="center">
  <img src="../../crates/assets/zoey-dreaming.png" alt="Zoey" width="350" />
</p>

# 🏗️ ZoeyOS Architecture

> **Your secrets are safe with Zoey**

---

## Overview

This document describes the architecture of ZoeyOS—a privacy-first, local-first AI agent framework written in Rust. The implementation maintains API compatibility with ZoeyOS while providing performance, safety, and embeddability benefits.

---

## Design Principles

1. **Privacy First**: All processing on local hardware by default
2. **Type Safety**: Leverage Rust's type system for compile-time guarantees
3. **Zero-Cost Abstractions**: Use trait objects and generics efficiently
4. **Memory Safety**: No unsafe code unless absolutely necessary
5. **Async-First**: Built on Tokio for high-performance concurrency
6. **Plugin Architecture**: Modular, extensible design

---

## Core Components

### 1. Type System (`types/`)

All core types are defined in the `types` module:

- **primitives.rs**: Basic types (UUID, Content, Metadata, Media)
- **memory.rs**: Memory and MemoryMetadata types
- **environment.rs**: Entity, Room, World, Component types
- **agent.rs**: Character and Agent configuration
- **state.rs**: State management for conversations
- **components.rs**: Action, Provider, Evaluator traits
- **plugin.rs**: Plugin trait and system
- **service.rs**: Service trait for stateful components
- **runtime.rs**: IAgentRuntime trait (main interface)
- **database.rs**: IDatabaseAdapter trait
- **model.rs**: Model types and LLM integration
- **events.rs**: Event system types

### 2. Runtime (`runtime.rs`)

The `AgentRuntime` struct is the main implementation of the `IAgentRuntime` trait. It manages:

- Plugin lifecycle and registration
- Service initialization
- Action/Provider/Evaluator registration
- Event dispatching
- State composition
- Model handler selection
- Memory management delegation

Key features:
- Thread-safe with `Arc<RwLock<T>>` for shared state
- Plugin dependency resolution
- Service registry with type-based lookup
- Event bus for pub/sub messaging
- State caching for performance

### 3. Error Handling (`error.rs`)

Comprehensive error handling using `thiserror`:

```rust
pub enum ZoeyError {
    Database(#[from] sqlx::Error),
    Plugin(String),
    Runtime(String),
    Model(String),
    // ... more variants
}

pub type Result<T> = std::result::Result<T, ZoeyError>;
```

---

## Plugin System

### Plugin Trait

```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn dependencies(&self) -> Vec<String>;
    fn priority(&self) -> i32;
    
    async fn init(&self, config: HashMap<String, String>, runtime: Arc<dyn IAgentRuntime>) -> Result<()>;
    
    fn actions(&self) -> Vec<Arc<dyn Action>>;
    fn providers(&self) -> Vec<Arc<dyn Provider>>;
    fn evaluators(&self) -> Vec<Arc<dyn Evaluator>>;
    fn services(&self) -> Vec<Arc<dyn Service>>;
    fn models(&self) -> HashMap<String, ModelHandler>;
    fn events(&self) -> HashMap<String, Vec<EventHandler>>;
}
```

### Plugin Lifecycle

1. **Registration**: `runtime.register_plugin(plugin)`
2. **Dependency Resolution**: Topological sort based on dependencies
3. **Initialization**: Call `plugin.init()` with configuration
4. **Component Registration**: Register actions, providers, evaluators, services
5. **Runtime**: Plugin components are called by runtime as needed

### Component Types

#### Actions

Define what Zoey can do:

```rust
#[async_trait]
pub trait Action: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn validate(&self, runtime: Arc<dyn IAgentRuntime>, message: &Memory, state: &State) -> Result<bool>;
    async fn handler(&self, runtime: Arc<dyn IAgentRuntime>, message: &Memory, state: &State, ...) -> Result<Option<ActionResult>>;
}
```

#### Providers

Supply contextual information (Zoey's "senses"):

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    async fn get(&self, runtime: Arc<dyn IAgentRuntime>, message: &Memory, state: &State) -> Result<ProviderResult>;
}
```

#### Evaluators

Post-interaction processing:

```rust
#[async_trait]
pub trait Evaluator: Send + Sync {
    fn name(&self) -> &str;
    async fn validate(&self, runtime: Arc<dyn IAgentRuntime>, message: &Memory, state: &State) -> Result<bool>;
    async fn handler(&self, runtime: Arc<dyn IAgentRuntime>, message: &Memory, state: &State, did_respond: bool, ...) -> Result<()>;
}
```

---

## Database Layer

### IDatabaseAdapter Trait

Defines interface for all database operations:

- Agent CRUD
- Entity/Room/World management
- Memory operations (create, search, update)
- Component management
- Task management
- Relationship tracking
- Logging

### Implementations

1. **PostgreSQL** (`crates/storage/zoey-storage-sql/src/postgres.rs`)
   - Full-featured with pgvector for embeddings
   - Connection pooling via SQLx
   - Schema migrations
   - Vector similarity search

2. **SQLite** (`crates/storage/zoey-storage-sql/src/sqlite.rs`)
   - Lightweight, embedded database
   - Falls back to BM25 for search
   - Good for development and testing
   - **Perfect for privacy—all data stays local**

---

## Memory System

### Memory Flow

1. Create memory with content
2. Optionally generate embedding (async)
3. Store in database
4. Search by:
   - Filters (agent_id, room_id, entity_id, time range)
   - Vector similarity (semantic search)
   - BM25 (text search fallback)

---

## Message Processing Pipeline

1. **Receive message** → Store in database
2. **Determine if should respond** → Simple rules + LLM evaluation
3. **Compose state** → Run providers to gather context
4. **Generate response** → Call LLM with state
5. **Process actions** → Validate and execute actions
6. **Run evaluators** → Post-processing (fact extraction, reflection)
7. **Emit events** → Notify listeners
8. **Return response**

---

## Concurrency Model

- **Tokio runtime**: Multi-threaded async executor
- **Shared state**: `Arc<RwLock<T>>` for interior mutability
- **Lock granularity**: Fine-grained locks (separate locks for actions, providers, services)
- **Async all the way**: No blocking operations in async context

---

## Performance Optimizations

1. **Connection Pooling**: SQLx pools for database connections
2. **State Caching**: LRU cache for composed states
3. **Lazy Initialization**: Services initialized on-demand
4. **Parallel Provider Execution**: Providers can run concurrently
5. **Model Handler Caching**: Pre-sorted by priority
6. **Zero-Copy**: Use `&str` and slices where possible

---

## Security Features

1. **Memory Safety**: Rust ownership prevents memory vulnerabilities
2. **Type Safety**: Strong typing prevents type confusion
3. **No SQL Injection**: Parameterized queries via SQLx
4. **Secrets Management**: Environment variables, encrypted storage
5. **Rate Limiting**: Built-in rate limiting for API calls
6. **Input Validation**: Validation at API boundaries
7. **Local-First**: Your data never leaves your network

---

## Performance Targets

| Metric | Target |
|--------|--------|
| Throughput | 10,000+ messages/second |
| Latency | < 1ms for action processing |
| Memory | < 50MB per agent |
| Startup | < 100ms to ready state |

---

## Development Workflow

1. **Setup**: Install Rust 1.75+, Cargo
2. **Build**: `cargo build --release`
3. **Test**: `cargo test --workspace`
4. **Run Example**: `cargo run --example basic_agent`
5. **Documentation**: `cargo doc --open`
6. **Format**: `cargo fmt`
7. **Lint**: `cargo clippy`

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
