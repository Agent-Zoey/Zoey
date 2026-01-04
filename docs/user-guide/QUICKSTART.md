<p align="center">
  <img src="../../crates/assets/zoey-happy.png" alt="Zoey" width="250" />
</p>

# ⚡ ZoeyOS Quick Start

> **Your secrets are safe with Zoey**

---

## Setup (5 minutes)

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Clone and Build

```bash
git clone https://github.com/Freytes/ZoeyAI
cd ZoeyAI
cargo build --release
```

### 3. Run Tests

```bash
cargo test --workspace
```

### 4. Try an Example

```bash
cargo run --example basic_agent
```

---

## What's Included

- Actions (Reply, SendMessage, etc.)
- Providers (Time, Character, RecentMessages, etc.)
- Evaluators (Reflection, FactExtraction, GoalTracking)
- Database adapters (SQLite, PostgreSQL)
- LLM providers (OpenAI, Anthropic, Ollama, llama.cpp)
- Security features (encryption, validation, rate limiting)
- Training & RLHF system

---

## Your First Agent

```rust
use zoey_core::*;
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_sql::SqliteAdapter;

#[tokio::main]
async fn main() -> Result<()> {
    // Create character
    let character = Character {
        name: "Zoey".to_string(),
        bio: vec!["Your secrets are safe with me!".to_string()],
        ..Default::default()
    };
    
    // Initialize
    let adapter = SqliteAdapter::new(":memory:").await?;
    let runtime = AgentRuntime::new(RuntimeOpts {
        character: Some(character),
        adapter: Some(Arc::new(adapter)),
        plugins: vec![Arc::new(BootstrapPlugin::new())],
        ..Default::default()
    }).await?;
    
    runtime.write().unwrap()
        .initialize(InitializeOptions::default()).await?;
    
    println!("🔐 Zoey is online!");
    Ok(())
}
```

---

## Common Commands

```bash
# Build
cargo build --release          # Optimized build

# Test
cargo test --workspace         # All tests

# Run examples
cargo run --example basic_agent
cargo run --example training_example

# Lint and format
cargo clippy
cargo fmt

# Documentation
cargo doc --open
```

---

## Next Steps

1. Explore the examples in `examples/`
2. Read the [Architecture](../developer/ARCHITECTURE.md) guide
3. Configure your agent - see [CONFIGURATION.md](CONFIGURATION.md)
4. Learn about [Training](../developer/TRAINING.md)

---

## Troubleshooting

**Can't find cargo?**
```bash
source $HOME/.cargo/env
```

**Build fails?**
```bash
rustup update
cargo clean
cargo build
```

**Tests fail?**
```bash
cargo test -- --nocapture
```

See [INSTALL.md](INSTALL.md) for more detailed setup instructions.

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
