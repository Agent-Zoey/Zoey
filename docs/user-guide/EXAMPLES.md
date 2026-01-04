<p align="center">
  <img src="../../crates/assets/zoey-happy.png" alt="Zoey" width="250" />
</p>

# 📚 Examples Guide

> **Your secrets are safe with Zoey**

ZoeyOS includes comprehensive examples demonstrating different use cases.

---

## Quick Reference

| Example | Complexity | Privacy | Use Case |
|---------|-----------|---------|----------|
| `basic_agent` | Beginner | Any | Learning |
| `standard_agent` | Intermediate | Any | General |
| `advanced_agent` | Advanced | Any | Enterprise |
| `government_compliant_agent` | Expert | Maximum | Gov/Healthcare |
| `training_example` | Intermediate | Any | ML Training |
| `local_infrastructure_example` | Intermediate | Maximum | Local-first |

---

## 1. basic_agent.rs - Learning & Testing

**Purpose**: Simple introduction to ZoeyOS

**What it shows**:
- Character creation
- Runtime initialization
- State composition
- UUID utilities
- Component listing

```bash
cargo run --example basic_agent
```

---

## 2. standard_agent.rs - General Purpose

**Purpose**: Standard deployment for most applications

**What it shows**:
- Standard mode configuration
- Cloud or local AI usage
- Maximum performance
- Feature comparison

```bash
cargo run --example standard_agent
```

---

## 3. advanced_agent.rs - Production Features

**Purpose**: Production-ready features demonstration

**What it shows**:
- Health monitoring
- Circuit breakers
- Retry logic with backoff
- Streaming responses
- Rate limiting
- Input validation

```bash
cargo run --example advanced_agent
```

---

## 4. government_compliant_agent.rs - Maximum Compliance

**Purpose**: Full compliance demonstration (HIPAA patterns)

**What it shows**:
- Local LLM usage (Ollama) 🔒
- PII detection and redaction
- Audit logging
- All compliance features

⚠️ **Note**: Compliance features are patterns, not certifications.

```bash
cargo run --example government_compliant_agent
```

---

## 5. training_example.rs - ML Training

**Purpose**: Training data collection and RLHF

**What it shows**:
- Automatic conversation collection
- Quality scoring
- Human feedback integration
- Multi-format export

```bash
cargo run --example training_example
```

---

## 6. local_infrastructure_example.rs - Privacy First 🔒

**Purpose**: Fully local, privacy-focused deployment

**What it shows**:
- Ollama integration
- Local vector database
- Zero cloud dependencies
- Air-gapped operation

```bash
cargo run --example local_infrastructure_example
```

---

## Which Example Should I Run?

### Just Learning?
→ `basic_agent.rs`

### Building a Chatbot?
→ `standard_agent.rs`

### Need Production Features?
→ `advanced_agent.rs`

### Healthcare or Government?
→ `government_compliant_agent.rs`

### Training Your Own Model?
→ `training_example.rs`

### Maximum Privacy?
→ `local_infrastructure_example.rs` 🔒

---

## Running All Examples

```bash
# Run examples in sequence
cargo run --example basic_agent
cargo run --example standard_agent
cargo run --example advanced_agent
```

---

## Streaming Chat Example (Web)

```javascript
// Minimal SSE-like client using fetch streaming
async function startStreaming(roomId, text) {
  const resp = await fetch('/agent/chat/stream', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ roomId, text, stream: true })
  });
  const reader = resp.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop();
    
    for (const line of lines) {
      if (line.startsWith('data:')) {
        const payload = JSON.parse(line.slice(5));
        if (payload.final) {
          console.log('🔐 Complete:', payload.text);
          return;
        }
        console.log('Chunk:', payload.text);
      }
    }
  }
}
```

---

All examples are production-ready code that you can use as templates.

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
