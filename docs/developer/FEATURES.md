<p align="center">
  <img src="../../crates/assets/zoey-happy.png" alt="Zoey" width="250" />
</p>

# 🔐 ZoeyOS - Complete Feature List

> **Your secrets are safe with Zoey**

---

## Table of Contents

1. [Core Features](#core-features)
2. [Privacy & Security](#privacy--security)
3. [Database & Storage](#database--storage)
4. [LLM Integration](#llm-integration)
5. [Knowledge Management](#knowledge-management)
6. [ML & Training](#ml--training)
7. [Workflow & Orchestration](#workflow--orchestration)
8. [Agent Components](#agent-components)
9. [Production Features](#production-features)
10. [Testing](#testing)

---

## Core Features

### Runtime System
- **AgentRuntime**: Main runtime implementation with full state management
- **Plugin System**: Trait-based with dependency resolution
- **Service Registry**: Type-based service lookup and management
- **Event Bus**: Pub/sub messaging for component communication
- **State Management**: Caching, composition, and merging
- **Settings**: Key-value configuration with type conversion

### Type System
Comprehensive type definitions:
- **primitives.rs**: UUID, Content, Media, Metadata, Role
- **memory.rs**: Memory, MemoryMetadata, MemoryQuery
- **environment.rs**: Entity, Room, World, Component
- **agent.rs**: Character, Agent, CharacterStyle
- **state.rs**: State with values and data separation
- **components.rs**: Action, Provider, Evaluator traits
- **plugin.rs**: Plugin trait, Route, ComponentType
- **service.rs**: Service trait, ServiceHealth
- **runtime.rs**: IAgentRuntime trait
- **database.rs**: IDatabaseAdapter trait

---

## Privacy & Security

### 🔒 Local-First Architecture
- **Zero Cloud Dependencies**: Works completely offline
- **Local LLM Support**: Ollama, llama.cpp, LocalAI
- **On-Device Processing**: All data stays on your hardware
- **No Telemetry**: Nothing phones home

### PII Detection & Protection
- **Pattern Matching**: SSN, email, phone, credit card, API keys
- **Automatic Redaction**: Replace sensitive data with placeholders
- **Compliance Signals**: HIPAA, GDPR, PCI-DSS pattern detection

### Security Features
- **Memory Safety**: Rust ownership prevents vulnerabilities
- **Type Safety**: Strong typing prevents type confusion
- **No SQL Injection**: Parameterized queries via SQLx
- **Secrets Management**: Environment variables, encrypted storage
- **Rate Limiting**: Built-in rate limiting for API calls
- **Input Validation**: Validation at API boundaries

---

## Database & Storage

### PostgreSQL Adapter
- Connection pooling with SQLx
- Schema auto-initialization
- Agent, Memory, Entity CRUD
- Vector similarity search (pgvector)
- Component and relationship tracking

### SQLite Adapter
- In-memory mode for testing
- File-based for persistence
- BM25 fallback for text search
- **Perfect for privacy—all data stays local**

### Local Vector Store
- HNSW-based indexing
- No external dependencies
- Disk persistence
- Fast similarity search

---

## LLM Integration

### Local Providers (Privacy First)
- **Ollama**: Easy-to-use local LLM server
- **llama.cpp**: Direct HTTP API, lowest latency
- **LocalAI**: OpenAI-compatible API

### Cloud Providers (Optional)
- **OpenAI**: GPT-3.5, GPT-4 support
- **Anthropic**: Claude 3 models

### Model Abstraction
- **ModelType Enum**: text, embedding, image, audio, video
- **Model Handlers**: Async function handlers
- **Priority System**: Select best available model
- **Provider Agnostic**: Works with any LLM provider
- **Provider Router**: Multi-provider request routing

---

## Knowledge Management

### Document Ingestion
- **PDF**: Extract text from documents
- **Markdown**: Parse with section hierarchy
- **CSV/JSON**: Structured data import
- **Plain Text**: Universal fallback

### Knowledge Graph
- **Entity Extraction**: Persons, organizations, locations, terms
- **Relationship Detection**: Semantic relationships
- **Ontologies**: Domain-specific concept hierarchies
- **Graph Queries**: Relationship traversal

### Hybrid Retrieval
- **Semantic Search**: Dense vector similarity
- **BM25 Lexical**: Term-based ranking
- **Graph-Based**: Entity relationship traversal
- **Query Expansion**: Automatic synonym generation
- **Re-Ranking**: Relevance optimization

---

## ML & Training

### Training Data Collection
- **Automatic Collection**: Every conversation is potential training data
- **Quality Scoring**: Automatic quality assessment
- **Human Feedback**: RLHF support

### Export Formats
- **JSONL**: Custom pipelines
- **Alpaca**: LLaMA, Vicuna models
- **ShareGPT**: FastChat trainers
- **OpenAI**: GPT fine-tuning

### PyTorch Integration
- Tensor operations via ndarray
- Model training loops
- Export to ONNX, TorchScript
- ⚠️ Optional: enable `libtorch` feature for real bindings

### Adaptive Learning
- **LoRA**: Low-rank adaptation (mathematical simulation)
- **Experience Replay**: Prevent catastrophic forgetting
- **EWC**: Elastic weight consolidation
- **Human Feedback**: Correction and quality scoring

### ML Bridge
- Python/Rust interoperability
- Execute Python training scripts
- Framework detection (PyTorch)
- Model registration and tracking

---

## Workflow & Orchestration

### Workflow Engine
- Multi-step workflow definitions
- Task dependencies and DAG execution
- Parallel and sequential execution
- Error handling and retries

### Scheduler
- Cron-based scheduling
- Recurring workflow execution
- Job monitoring

### Resource Management
- CPU/GPU quotas
- Memory limits
- Rate limiting and throttling
- Concurrent task limits

### Conditional Branching
- If/else conditions
- Switch statements
- Loop constructs

### Distributed Execution (Alpha)
- Multi-worker task distribution
- Load balancing
- Worker health monitoring

---

## Agent Components

### Actions
1. **Reply** - Respond to messages
2. **Ignore** - Skip response
3. **None** - Continue without action
4. **SendMessage** - Send to specific target
5. **FollowRoom** - Start following a room
6. **UnfollowRoom** - Stop following a room

### Providers
1. **Time** - Current date/time
2. **Character** - Agent personality
3. **Actions** - Available actions list
4. **Entities** - Entity information
5. **RecentMessages** - Conversation history

### Evaluators
1. **Reflection** - Self-assessment
2. **FactExtraction** - Extract facts
3. **GoalTracking** - Track goals

---

## Production Features

### Model Orchestration
- **Cascade Strategies**: Conservative, Adaptive, MaxQuality
- **Dynamic Model Swapping**: Cost/latency optimization
- **Complexity Estimation**: Route to appropriate model

### Memory Management
- **Compression**: GZIP, deduplication, summarization
- **Forgetting Curves**: Exponential, linear, step
- **Hierarchical Summarization**: Day → Week → Month

### Observability
- **Reasoning Chains**: Track decision-making
- **Confidence Scoring**: Quantify certainty
- **Audit Logging**: Complete audit trails

### Resilience
- **Circuit Breakers**: Prevent cascading failures
- **Health Checks**: Service monitoring
- **Retry Logic**: Exponential backoff

---

## Testing

### Unit Tests
- Core runtime tests
- Plugin registration tests
- Component tests
- Utility tests

### Integration Tests
- Runtime initialization
- Plugin registration
- State composition
- Message processing
- Action execution

### Load Tests
- Concurrent processing
- Memory stress tests
- Rate limiter load tests

### Benchmarks
- UUID generation
- BM25 search
- State operations
- Template rendering

---

## Summary

| Category | Features |
|----------|----------|
| **Core** | Runtime, Plugins, Events, State |
| **Privacy** | Local-first, PII detection, Encryption |
| **Database** | PostgreSQL, SQLite, Vector store |
| **LLM** | Ollama, llama.cpp, OpenAI, Anthropic |
| **Knowledge** | Ingestion, Graphs, Hybrid search |
| **ML** | Training, RLHF, LoRA, PyTorch |
| **Workflow** | Engine, Scheduler, Distributed |
| **Production** | Orchestration, Memory, Observability |

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
