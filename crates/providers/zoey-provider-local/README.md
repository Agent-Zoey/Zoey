<p align="center">
  <img src="../../assets/zoey-laughing.png" alt="Zoey" width="300" />
</p>

# 🏠 zoey-provider-local

> **Your secrets are safe with Zoey**

Local-first LLM provider for ZoeyOS—connect to Ollama, llama.cpp, LocalAI, and other local inference backends. Keep all your data on your hardware.

## Status: ✅ Beta

---

## Features

### 🔒 True Privacy
All inference happens locally—no data ever leaves your network:
- No API keys required
- No rate limits
- No cloud dependencies
- Works completely offline

### 🔌 Multiple Backends

| Backend | Description | Performance |
|---------|-------------|-------------|
| **Ollama** | Easy-to-use local LLM server | Best for getting started |
| **llama.cpp** | Direct HTTP API | Lowest latency |
| **LocalAI** | OpenAI-compatible API | Drop-in replacement |
| **Custom** | Any HTTP/OpenAI-compatible endpoint | Flexible |

### ⚡ Optimized for Local
- Connection pooling
- Response streaming
- Batch inference
- GPU acceleration support

---

## Quick Start

```rust
use zoey_provider_local::LocalLLMPlugin;

let plugin = LocalLLMPlugin::new();
// Automatically connects to Ollama at localhost:11434
```

---

## Configuration

### Environment Variables

```bash
# Ollama (default)
OLLAMA_BASE_URL=http://localhost:11434
OLLAMA_MODEL=llama3.2

# llama.cpp
LLAMACPP_BASE_URL=http://localhost:8080
LLAMACPP_MODEL=default

# LocalAI
LOCALAI_BASE_URL=http://localhost:8080
LOCALAI_MODEL=gpt-3.5-turbo

# Default model for the provider
DEFAULT_MODEL=llama3.2
```

### Programmatic Configuration

```rust
use zoey_provider_local::{LocalLLMPlugin, LocalConfig, Backend};

let config = LocalConfig {
    backend: Backend::Ollama,
    base_url: "http://localhost:11434".to_string(),
    default_model: "llama3.2".to_string(),
    timeout_secs: 120,
    max_connections: 4,
    enable_streaming: true,
    ..Default::default()
};

let plugin = LocalLLMPlugin::with_config(config);
```

---

## Backend Setup

### Ollama (Recommended)

```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull llama3.2

# Start server (usually automatic)
ollama serve
```

### llama.cpp

```bash
# Clone and build
git clone https://github.com/ggerganov/llama.cpp
cd llama.cpp
make

# Start server
./server -m model.gguf --port 8080
```

### LocalAI

```bash
# Docker
docker run -p 8080:8080 localai/localai:latest

# Or native install
# See: https://localai.io/basics/getting_started/
```

---

## Usage Examples

### Basic Inference

```rust
use zoey_provider_local::LocalLLMPlugin;

let plugin = LocalLLMPlugin::new();

let response = plugin.generate(
    "What is the capital of France?",
    GenerateOptions::default(),
).await?;

println!("{}", response.text);
```

### Streaming Response

```rust
let mut stream = plugin.generate_stream(
    "Write a short story about a robot",
    GenerateOptions::default(),
).await?;

while let Some(chunk) = stream.next().await {
    print!("{}", chunk.text);
}
```

### With System Prompt

```rust
let response = plugin.generate(
    "Explain this medical term",
    GenerateOptions {
        system_prompt: Some("You are a medical assistant. Explain terms clearly.".to_string()),
        temperature: 0.3,
        max_tokens: Some(500),
        ..Default::default()
    },
).await?;
```

### Model Selection

```rust
// Use a specific model
let response = plugin.generate_with_model(
    "llama3.2:70b",
    "Complex reasoning task",
    GenerateOptions::default(),
).await?;

// List available models
let models = plugin.list_models().await?;
for model in models {
    println!("{}: {} ({:.1} GB)", model.name, model.family, model.size_gb);
}
```

---

## Model Recommendations

| Use Case | Model | VRAM | Notes |
|----------|-------|------|-------|
| General chat | llama3.2:8b | 8 GB | Good balance |
| Fast responses | llama3.2:3b | 4 GB | Quick, less accurate |
| Complex tasks | llama3.2:70b | 48 GB | Best quality |
| Code | codellama:13b | 16 GB | Code-optimized |
| Medical | meditron:7b | 8 GB | Medical domain |
| Embeddings | nomic-embed-text | 2 GB | For RAG |

---

## Performance Tips

### GPU Acceleration

```bash
# Ollama with GPU
ollama run llama3.2 --gpu

# llama.cpp with CUDA
make LLAMA_CUBLAS=1
./server -m model.gguf --n-gpu-layers 35
```

### Memory Optimization

```rust
let config = LocalConfig {
    // Use smaller context for lower memory
    default_context_length: 2048,
    
    // Enable flash attention if available
    enable_flash_attention: true,
    
    // Quantization (4-bit is fastest)
    quantization: Some(Quantization::Q4_K_M),
    
    ..Default::default()
};
```

### Batch Processing

```rust
let prompts = vec![
    "Question 1",
    "Question 2",
    "Question 3",
];

let responses = plugin.generate_batch(
    prompts,
    GenerateOptions::default(),
).await?;
```

---

## Troubleshooting

### Connection Refused

```bash
# Check if Ollama is running
curl http://localhost:11434/api/tags

# Start if needed
ollama serve
```

### Slow Responses

- Ensure GPU is being used (`nvidia-smi` to check)
- Try a smaller model
- Reduce context length
- Enable streaming for perceived speed

### Out of Memory

- Use quantized models (Q4_K_M, Q5_K_M)
- Reduce batch size
- Reduce context length
- Use CPU offloading for large models

---

## Dependencies

- `zoey-core` - Core runtime and types

---

## Testing

```bash
cargo test -p zoey-provider-local
```

---

## License

MIT License

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
