<p align="center">
  <img src="crates/assets/zoey-windswept.png" alt="Zoey" width="500" />
</p>

<h1 align="center">🔐 ZoeyAI</h1>

<p align="center">
  <strong>Your secrets are safe with Zoey</strong>
</p>

<p align="center">
  <a href="#features"><img src="https://img.shields.io/badge/status-alpha-yellow" alt="Status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.75%2B-orange" alt="Rust"></a>
</p>

<p align="center">
  A privacy-first, local-first AI agent framework written in Rust.<br>
  Run AI agents on your own hardware with support for Ollama, llama.cpp, and other local inference engines.
</p>

## Overview

ZoeyAI provides a modular runtime for building AI agents that can run entirely on local hardware. The framework emphasizes resource efficiency, privacy, and offline operation while maintaining compatibility with cloud providers when needed.

## Quick Start

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/Agent-Zoey/Zoey
cd Zoey
cargo build --release

# Run an example
cargo run --example basic_agent
```

## Use Cases

- **Home Servers** - Privacy-focused self-hosted AI
- **Edge Devices** - Raspberry Pi, embedded systems
- **Industrial** - Air-gapped networks, factory floor automation
- **Privacy-Critical** - Self-hosted AI without cloud dependencies

## Features

### Resource Efficient
- Low memory footprint (Rust vs Node.js overhead)
- Fast startup time
- Suitable for Raspberry Pi and embedded systems

### Local-First
- Works offline
- SQLite for embedded databases
- Local vector search support
- Optimized for Ollama, llama.cpp, LocalAI
- Optional cloud provider support (OpenAI, Anthropic)

### Privacy
- Air-gapped operation support
- No telemetry by default
- Your data stays on your hardware

### Cross-Platform
- Linux (x86_64, ARM64, ARM32)
- Windows
- macOS
- Single binary deployment

## Current Status (Alpha)

### Core Components
- Agent runtime with async execution
- Plugin system with dependency resolution
- Memory management (vector embeddings, BM25 search)
- Handlebars template engine
- State composition system

### Model Support
- **Local**: Ollama, llama.cpp, LocalAI
- **Cloud**: OpenAI, Anthropic (optional)

### Database Adapters
- SQLite (embedded)
- PostgreSQL (with pgvector)
- In-memory (testing)

### Workflow Orchestration
- Multi-step workflow execution
- Task scheduling with cron support
- Conditional branching

## Architecture

ZoeyAI follows a modular, layered architecture designed for extensibility and local-first operation.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                              Client Adaptors                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐                     │
│  │ Discord  │  │ Telegram │  │   Web    │  │ Terminal │                     │
│  │  Voice   │  │   Bot    │  │  UI/API  │  │   CLI    │                     │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘                     │
└───────┼─────────────┼─────────────┼─────────────┼────────────────────────────┘
        │             │             │             │
        └─────────────┴──────┬──────┴─────────────┘
                             │
┌────────────────────────────┼─────────────────────────────────────────────────┐
│                      Agent API & Runtime                                      │
│  ┌─────────────────────────┴─────────────────────────┐                       │
│  │              AgentRuntime                          │                       │
│  │  ┌─────────────┐  ┌─────────────┐  ┌───────────┐  │                       │
│  │  │   State     │  │   Events    │  │  Context  │  │                       │
│  │  │ Management  │  │   System    │  │  Builder  │  │                       │
│  │  └─────────────┘  └─────────────┘  └───────────┘  │                       │
│  └───────────────────────────────────────────────────┘                       │
└──────────────────────────────────────────────────────────────────────────────┘
                             │
┌────────────────────────────┼─────────────────────────────────────────────────┐
│                       Plugin System                                           │
│                            │                                                  │
│  ┌─────────────────────────┴─────────────────────────┐                       │
│  │              Plugin Registry                       │                       │
│  └─────────────────────────┬─────────────────────────┘                       │
│                            │                                                  │
│  ┌────────────┬────────────┼────────────┬────────────┐                       │
│  │            │            │            │            │                       │
│  ▼            ▼            ▼            ▼            ▼                       │
│ ┌──────┐  ┌────────┐  ┌─────────┐  ┌────────┐  ┌──────────┐                 │
│ │Action│  │Provider│  │Evaluator│  │Functor │  │ Service  │                 │
│ │      │  │        │  │         │  │        │  │          │                 │
│ │REPLY │  │ time   │  │reflect  │  │compose │  │knowledge │                 │
│ │IGNORE│  │ char   │  │extract  │  │format  │  │scheduler │                 │
│ │SEND  │  │ recall │  │goal     │  │        │  │workflow  │                 │
│ └──────┘  └────────┘  └─────────┘  └────────┘  └──────────┘                 │
└──────────────────────────────────────────────────────────────────────────────┘
                             │
┌────────────────────────────┼─────────────────────────────────────────────────┐
│                      LLM Providers                                            │
│  ┌─────────────────────────┴─────────────────────────┐                       │
│  │              Provider Router                       │                       │
│  │   • Cost-optimized routing                        │                       │
│  │   • Load balancing                                │                       │
│  │   • Fallback chains                               │                       │
│  └─────────────────────────┬─────────────────────────┘                       │
│                            │                                                  │
│  ┌────────────┬────────────┼────────────┬────────────┐                       │
│  │            │            │            │            │                       │
│  ▼            ▼            ▼            ▼            ▼                       │
│ ┌──────┐  ┌────────┐  ┌─────────┐  ┌────────┐  ┌──────────┐                 │
│ │Ollama│  │llama   │  │ LocalAI │  │ OpenAI │  │Anthropic │                 │
│ │      │  │  .cpp  │  │         │  │        │  │          │                 │
│ │ Local│  │ Native │  │  Docker │  │  Cloud │  │  Cloud   │                 │
│ └──────┘  └────────┘  └─────────┘  └────────┘  └──────────┘                 │
│                                                                               │
│  ┌────────────────────────────────────────────────────┐                      │
│  │              Voice Provider                         │                      │
│  │   • Whisper (STT)  • Piper (TTS)  • Moshi          │                      │
│  └────────────────────────────────────────────────────┘                      │
└──────────────────────────────────────────────────────────────────────────────┘
                             │
┌────────────────────────────┼─────────────────────────────────────────────────┐
│                      Storage Layer                                            │
│  ┌─────────────────────────┴─────────────────────────┐                       │
│  │              IDatabaseAdapter Trait                │                       │
│  └─────────────────────────┬─────────────────────────┘                       │
│                            │                                                  │
│  ┌────────────┬────────────┼────────────┬────────────┐                       │
│  │            │            │            │            │                       │
│  ▼            ▼            ▼            ▼            ▼                       │
│ ┌──────┐  ┌────────┐  ┌─────────┐  ┌────────┐  ┌──────────┐                 │
│ │SQLite│  │Postgres│  │  Vector │  │ Memory │  │  Cache   │                 │
│ │      │  │        │  │  Store  │  │        │  │          │                 │
│ │Local │  │pgvector│  │ BM25+   │  │Messages│  │ LRU/TTL  │                 │
│ │ File │  │  Cloud │  │Embedding│  │ State  │  │  Layer   │                 │
│ └──────┘  └────────┘  └─────────┘  └────────┘  └──────────┘                 │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Core Components

| Component | Description |
|-----------|-------------|
| **AgentRuntime** | Central orchestrator managing agent lifecycle, state, and plugin coordination |
| **Plugin Registry** | Dependency-aware loader for actions, providers, evaluators, and services |
| **Context Builder** | Assembles conversation context from templates, memory, and state |
| **Provider Router** | Routes LLM requests with load balancing, fallbacks, and cost optimization |

### Plugin Types

| Type | Purpose | Examples |
|------|---------|----------|
| **Action** | Defines what the agent can do | REPLY, IGNORE, SEND_MESSAGE |
| **Provider** | Supplies context data | time, character, recall, entities |
| **Evaluator** | Post-response analysis | reflection, fact_extraction, goal_tracking |
| **Functor** | Data transformation | compose, format, validate |
| **Service** | Background processes | knowledge ingestion, scheduling, workflows |

### Data Flow

```
User Input → Adaptor → Agent API → Context Builder → LLM Provider
                                        ↓
                              ← Action Execution ← Response Parsing
                                        ↓
                              Evaluators → Memory Storage → Response
```

## Documentation

See [docs/](docs/) for complete documentation:

- [Quick Start](docs/user-guide/QUICKSTART.md)
- [Configuration](docs/user-guide/CONFIGURATION.md)
- [Examples](docs/user-guide/EXAMPLES.md)
- [Architecture](docs/developer/ARCHITECTURE.md)
- [Plugin Development](docs/developer/PLUGINS.md)

## Examples

See the [examples/](examples/) directory for working code:

- `basic_agent.rs` - Simple agent setup
- `standard_agent.rs` - Standard agent configuration
- `advanced_agent.rs` - Advanced features
- `interactive_agent.rs` - Interactive chat agent
- `local_infrastructure_example.rs` - Local vector DB and routing

## Project Structure

```
ZoeyAI/
├── crates/
│   ├── core/
│   │   └── zoey-core/              # Core runtime with agent API, plugins, types
│   │
│   ├── plugins/
│   │   ├── zoey-plugin-bootstrap/  # Actions, providers, evaluators
│   │   ├── zoey-plugin-hardware/   # Hardware detection and optimization
│   │   ├── zoey-plugin-knowledge/  # Document ingestion and retrieval
│   │   ├── zoey-plugin-lifeengine/ # Life engine features
│   │   ├── zoey-plugin-memory/     # Memory management
│   │   ├── zoey-plugin-moderation/ # Content moderation
│   │   ├── zoey-plugin-observability/ # Reasoning chains, logging
│   │   ├── zoey-plugin-rag-connectors/ # RAG connector integrations
│   │   ├── zoey-plugin-scheduler/  # Task scheduling
│   │   ├── zoey-plugin-search/     # Search functionality
│   │   └── zoey-plugin-x402-video/ # Payment-gated AI video generation
│   │
│   ├── extensions/
│   │   └── zoey-ext-workflow/      # Workflow orchestration engine
│   │
│   ├── providers/
│   │   ├── zoey-provider-anthropic/ # Anthropic integration
│   │   ├── zoey-provider-local/    # Local LLMs (Ollama, llama.cpp)
│   │   ├── zoey-provider-openai/   # OpenAI integration
│   │   ├── zoey-provider-router/   # Multi-provider request routing
│   │   └── zoey-provider-voice/    # Voice synthesis/recognition
│   │
│   ├── storage/
│   │   ├── zoey-storage-sql/       # SQLite and PostgreSQL adapters
│   │   └── zoey-storage-vector/    # Local vector storage
│   │
│   └── adaptors/
│       ├── zoey-adaptor-discord/   # Discord integration
│       ├── zoey-adaptor-telegram/  # Telegram integration
│       ├── zoey-adaptor-terminal/  # Terminal/CLI interface
│       └── zoey-adaptor-web/       # Web interface and REST API
│
├── examples/                        # Example applications
├── docs/                            # Documentation
└── tools/                           # Utilities
```

## Contributing

Contributions are welcome! Please see the [issues](https://github.com/ZoeyAI/Zoey/issues) for areas where help is needed.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Status

**Alpha** - Core features implemented, under active development.
