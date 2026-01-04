<p align="center">
  <img src="../../assets/zoey-happy.png" alt="Zoey" width="250" />
</p>

# 🚀 zoey-plugin-bootstrap

> **Your secrets are safe with Zoey**

The essential plugin that makes Zoey functional. Provides core actions, providers, and evaluators required for basic agent operation.

## Status: ✅ Production

---

## What It Does

- **Core Actions**: Message handling, memory management, conversation flow
- **Essential Providers**: Context building, state management
- **Base Evaluators**: Response quality, relevance scoring

## What It Does NOT Do

- Advanced compliance features (see `zoey-plugin-compliance`)
- Knowledge management (see `zoey-plugin-knowledge`)
- ML/training operations (see `zoey-plugin-ml`)

---

## Quick Start

```rust
use zoey_core::{AgentRuntime, RuntimeOpts};
use zoey_plugin_bootstrap::BootstrapPlugin;
use std::sync::Arc;

let mut opts = RuntimeOpts::default();
opts.add_plugin(Arc::new(BootstrapPlugin::new()));

let runtime = AgentRuntime::new(opts).await?;
// Zoey now has basic conversational abilities
```

---

## Included Actions

| Action | Description |
|--------|-------------|
| `RESPOND` | Generate a response to user input |
| `REMEMBER` | Store information in memory |
| `RECALL` | Retrieve information from memory |
| `SUMMARIZE` | Condense conversation history |

## Included Providers

| Provider | Description |
|----------|-------------|
| `conversation` | Current conversation context |
| `memory` | Agent memory state |
| `time` | Current time and date |
| `agent_info` | Agent configuration and identity |

## Included Evaluators

| Evaluator | Description |
|-----------|-------------|
| `relevance` | Score response relevance |
| `quality` | Assess response quality |
| `safety` | Basic safety checks |

---

## Configuration

```rust
use zoey_plugin_bootstrap::{BootstrapPlugin, BootstrapConfig};

let config = BootstrapConfig {
    enable_memory: true,
    max_memory_items: 1000,
    enable_summarization: true,
    summary_threshold: 20,  // messages before summarization
    ..Default::default()
};

let plugin = BootstrapPlugin::with_config(config);
```

---

## Dependencies

- `zoey-core` - Core runtime and types

---

## Testing

```bash
cargo test -p zoey-plugin-bootstrap
```

---

## License

MIT License

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
