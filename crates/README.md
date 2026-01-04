<p align="center">
  <img src="assets/zoey-windswept.png" alt="Zoey" width="450" />
</p>

<h1 align="center">🔐 ZoeyAI</h1>

<p align="center">
  <strong>Your secrets are safe with Zoey</strong>
</p>

<p align="center">
  <em>A privacy-first, local-first AI agent framework written in Rust</em>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#architecture">Architecture</a> •
  <a href="#crates">Crates</a> •
  <a href="#getting-started">Getting Started</a> •
  <a href="#license">License</a>
</p>

---

## Overview

**Zoey** is an intelligent AI assistant built for privacy-conscious environments. Run AI agents entirely on your hardware with zero data leaving your network.

<p align="center">
  <img src="assets/zoey-gentle.png" alt="Zoey" width="350" />
</p>

## Features

### 🛡️ Privacy First
- **Local Execution**: All processing happens on your hardware
- **Zero Cloud Dependencies**: Works completely offline
- **Air-gapped Support**: Deploy in isolated networks

### 🧠 Intelligent Retrieval
- **Knowledge Ingestion**: Process documents and answer domain questions
- **Hybrid Retrieval**: Semantic + BM25 search
- **Vector Storage**: Local vector database

<p align="center">
  <img src="assets/zoey-curious.png" alt="Zoey - Curious" width="350" />
</p>

### ⚡ Production Ready
- **High Performance**: Rust-powered for speed and safety
- **Memory Management**: Efficient memory handling
- **Multi-Platform**: Web, CLI, and API interfaces

### 🔌 Extensible
- **Plugin System**: Modular architecture for custom functionality
- **Provider Agnostic**: Local LLMs, OpenAI, Anthropic, and more
- **Workflow Engine**: Multi-step task orchestration

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                           ZoeyAI Framework                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                │
│   │   Adaptors  │  │    Core     │  │  Providers  │                │
│   │             │  │             │  │             │                │
│   │ • Web       │  │ • Runtime   │  │ • Local LLM │                │
│   │ • Discord   │  │ • Plugins   │  │ • Router    │                │
│   │ • Telegram  │  │ • Agent API │  │ • OpenAI    │                │
│   └─────────────┘  └─────────────┘  └─────────────┘                │
│          │                │                │                        │
│          └────────────────┼────────────────┘                        │
│                           │                                         │
│   ┌─────────────────────────────────────────────────────────┐      │
│   │                        Plugins                           │      │
│   ├─────────────┬─────────────┬─────────────┬───────────────┤      │
│   │ Bootstrap   │ Knowledge   │ Memory      │ Observability │      │
│   │ X402 Video  │ Scheduler   │ Moderation  │ Search        │      │
│   └─────────────┴─────────────┴─────────────┴───────────────┘      │
│                           │                                         │
│   ┌─────────────────────────────────────────────────────────┐      │
│   │                      Extensions                          │      │
│   ├─────────────────────────────────────────────────────────┤      │
│   │                      Workflow                            │      │
│   └─────────────────────────────────────────────────────────┘      │
│                           │                                         │
│   ┌─────────────────────────────────────────────────────────┐      │
│   │                       Storage                            │      │
│   ├─────────────────────────────┬───────────────────────────┤      │
│   │       Vector Store          │         SQL Store         │      │
│   └─────────────────────────────┴───────────────────────────┘      │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Crates

### Core

| Crate | Description | Status |
|-------|-------------|--------|
| [`zoey-core`](core/zoey-core) | Runtime, plugin system, types, and agent API | ✅ Production |

### Plugins

| Crate | Description | Status |
|-------|-------------|--------|
| [`zoey-plugin-bootstrap`](plugins/zoey-plugin-bootstrap) | Essential actions, providers, and evaluators | ✅ Production |
| [`zoey-plugin-hardware`](plugins/zoey-plugin-hardware) | Hardware detection and optimization | ✅ Beta |
| [`zoey-plugin-knowledge`](plugins/zoey-plugin-knowledge) | Document ingestion and hybrid retrieval | ✅ Production |
| [`zoey-plugin-lifeengine`](plugins/zoey-plugin-lifeengine) | Life engine features | 🚧 Alpha |
| [`zoey-plugin-memory`](plugins/zoey-plugin-memory) | Memory management | ✅ Beta |
| [`zoey-plugin-moderation`](plugins/zoey-plugin-moderation) | Content moderation | ✅ Beta |
| [`zoey-plugin-observability`](plugins/zoey-plugin-observability) | Reasoning chains and logging | ✅ Beta |
| [`zoey-plugin-rag-connectors`](plugins/zoey-plugin-rag-connectors) | RAG connector integrations | ✅ Beta |
| [`zoey-plugin-scheduler`](plugins/zoey-plugin-scheduler) | Task scheduling | ✅ Beta |
| [`zoey-plugin-search`](plugins/zoey-plugin-search) | Search functionality | ✅ Beta |
| [`zoey-plugin-x402-video`](plugins/zoey-plugin-x402-video) | Payment-gated AI video generation | ✅ Production |

### Extensions

| Crate | Description | Status |
|-------|-------------|--------|
| [`zoey-ext-workflow`](extensions/zoey-ext-workflow) | Workflow orchestration engine | ✅ Production |

### Providers

| Crate | Description | Status |
|-------|-------------|--------|
| [`zoey-provider-anthropic`](providers/zoey-provider-anthropic) | Anthropic Claude integration | ✅ Beta |
| [`zoey-provider-local`](providers/zoey-provider-local) | Local LLM backends (Ollama, llama.cpp) | ✅ Beta |
| [`zoey-provider-openai`](providers/zoey-provider-openai) | OpenAI GPT integration | ✅ Beta |
| [`zoey-provider-router`](providers/zoey-provider-router) | Multi-provider request routing | ✅ Alpha |
| [`zoey-provider-voice`](providers/zoey-provider-voice) | Voice synthesis and recognition | 🚧 Alpha |

### Storage

| Crate | Description | Status |
|-------|-------------|--------|
| [`zoey-storage-sql`](storage/zoey-storage-sql) | SQLite and PostgreSQL adapters | ✅ Production |
| [`zoey-storage-vector`](storage/zoey-storage-vector) | Local vector storage | ✅ Beta |

### Adaptors

| Crate | Description | Status |
|-------|-------------|--------|
| [`zoey-adaptor-discord`](adaptors/zoey-adaptor-discord) | Discord bot integration | 🚧 Alpha |
| [`zoey-adaptor-telegram`](adaptors/zoey-adaptor-telegram) | Telegram bot integration | 🚧 Alpha |
| [`zoey-adaptor-terminal`](adaptors/zoey-adaptor-terminal) | Terminal/CLI interface | ✅ Beta |
| [`zoey-adaptor-web`](adaptors/zoey-adaptor-web) | Web interface and REST API | ✅ Production |

---

## Getting Started

### Prerequisites

- Rust 1.75+
- (Optional) Ollama or llama.cpp for local inference

### Quick Start

```rust
use zoey_core::{AgentRuntime, RuntimeOpts};
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_knowledge::KnowledgePlugin;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize Zoey with plugins
    let mut opts = RuntimeOpts::default();
    opts.add_plugin(Arc::new(BootstrapPlugin::new()));
    opts.add_plugin(Arc::new(KnowledgePlugin::new()));
    
    let runtime = AgentRuntime::new(opts).await?;
    
    // Zoey is ready to help—your secrets are safe
    println!("🔐 Zoey is online");
    
    Ok(())
}
```

### Configuration

Set up your environment:

```bash
# Local LLM (recommended for privacy)
OLLAMA_BASE_URL=http://localhost:11434
DEFAULT_MODEL=llama3.2

# Or use cloud providers (data leaves your network)
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
```

---

## Meet Zoey

<p align="center">
  <img src="assets/zoey-eye.png" alt="Zoey - Detail" width="500" />
</p>

<p align="center">
  <em>Always watching over your data</em>
</p>

<table align="center">
  <tr>
    <td align="center"><img src="assets/zoey-laughing.png" width="200" /><br><em>Happy to help</em></td>
    <td align="center"><img src="assets/zoey-gentle.png" width="200" /><br><em>Gentle guidance</em></td>
    <td align="center"><img src="assets/zoey-curious.png" width="200" /><br><em>Curious learner</em></td>
  </tr>
</table>

<p align="center">
  <img src="assets/zoey-forest.png" alt="Zoey - Full" width="600" />
</p>

---

## License

MIT License - See [LICENSE](../LICENSE) for details.

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
