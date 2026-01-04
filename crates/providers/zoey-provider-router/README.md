<p align="center">
  <img src="../../assets/zoey-confident.png" alt="Zoey" width="250" />
</p>

# 🔀 zoey-provider-router

> **Your secrets are safe with Zoey**

Multi-provider request routing for ZoeyOS—intelligently distribute LLM requests across providers using round-robin, least-loaded, capability-based, or cost-optimized strategies.

## Status: ✅ Alpha

---

## Features

### 🎯 Routing Strategies

| Strategy | Description | Best For |
|----------|-------------|----------|
| **RoundRobin** | Cycle through providers evenly | Load distribution |
| **LeastLoaded** | Route to provider with lowest load | Performance |
| **Capability** | Match request to provider capabilities | Task-specific models |
| **Cost** | Minimize cost per request | Budget optimization |

### 📊 Provider Management
- Dynamic provider registration
- Health monitoring
- Automatic failover
- Priority weighting

---

## Quick Start

```rust
use zoey_provider_router::{ProviderRouter, RoutingStrategy, ProviderInfo};

let mut router = ProviderRouter::new(RoutingStrategy::RoundRobin);

// Register providers
router.register(ProviderInfo {
    name: "local".into(),
    base_url: "http://localhost:11434".into(),
    capabilities: vec!["chat", "embeddings"],
    ..Default::default()
});

router.register(ProviderInfo {
    name: "openai".into(),
    base_url: "https://api.openai.com".into(),
    capabilities: vec!["chat", "vision", "embeddings"],
    cost_per_1k_tokens: 0.002,
    ..Default::default()
});

// Route a request
let provider = router.route(None)?;
println!("Routing to: {}", provider.name);
```

---

## Configuration

### Programmatic Configuration

```rust
use zoey_provider_router::{ProviderRouter, RoutingStrategy, RouterConfig};

let config = RouterConfig {
    strategy: RoutingStrategy::LeastLoaded,
    enable_failover: true,
    failover_threshold: 3,  // failures before failover
    health_check_interval_secs: 30,
    ..Default::default()
};

let router = ProviderRouter::with_config(config);
```

### Provider Configuration

```rust
use zoey_provider_router::ProviderInfo;

let provider = ProviderInfo {
    name: "local-fast".to_string(),
    base_url: "http://localhost:11434".to_string(),
    
    // Capabilities this provider supports
    capabilities: vec![
        "chat".to_string(),
        "embeddings".to_string(),
    ],
    
    // Models available
    models: vec![
        "llama3.2:8b".to_string(),
        "nomic-embed-text".to_string(),
    ],
    
    // Routing metrics
    priority: 10,           // Higher = preferred
    cost_per_1k_tokens: 0.0, // Local = free
    avg_latency_ms: 100,
    
    // Limits
    max_concurrent: 4,
    rate_limit_rpm: 1000,
    
    ..Default::default()
};
```

---

## Routing Strategies

### Round Robin

Distribute requests evenly across all healthy providers:

```rust
let router = ProviderRouter::new(RoutingStrategy::RoundRobin);

// Request 1 -> Provider A
// Request 2 -> Provider B
// Request 3 -> Provider C
// Request 4 -> Provider A
// ...
```

### Least Loaded

Route to the provider with the lowest current load:

```rust
let router = ProviderRouter::new(RoutingStrategy::LeastLoaded);

// Tracks active requests per provider
// Routes to provider with fewest active requests
```

### Capability-Based

Match requests to providers based on required capabilities:

```rust
let router = ProviderRouter::new(RoutingStrategy::Capability);

// Vision request -> OpenAI (has vision capability)
// Embedding request -> Local or OpenAI (both have embeddings)
// Code request -> Codellama provider (code-optimized)
```

### Cost-Optimized

Minimize cost while meeting requirements:

```rust
let router = ProviderRouter::new(RoutingStrategy::Cost);

// Simple request -> Local (free)
// Complex request needing GPT-4 -> OpenAI (cheapest option with capability)
```

---

## Usage Examples

### Basic Routing

```rust
// Route with no specific requirements
let provider = router.route(None)?;

// Route with capability requirement
let provider = router.route(Some(RouteRequirements {
    capabilities: vec!["vision".to_string()],
    ..Default::default()
}))?;

// Route with model requirement
let provider = router.route(Some(RouteRequirements {
    model: Some("gpt-4".to_string()),
    ..Default::default()
}))?;
```

### Provider Health Monitoring

```rust
// Check provider health
let health = router.health_check("local").await?;
println!("Provider local: {:?}", health.status);

// Get all provider statuses
let statuses = router.all_statuses();
for (name, status) in statuses {
    println!("{}: {:?} (latency: {}ms)", name, status.health, status.avg_latency_ms);
}
```

### Failover Handling

```rust
let config = RouterConfig {
    enable_failover: true,
    failover_threshold: 3,
    ..Default::default()
};

let router = ProviderRouter::with_config(config);

// If primary provider fails 3 times, automatically routes to next provider
let provider = router.route_with_failover(requirements).await?;
```

### Dynamic Registration

```rust
// Add provider at runtime
router.register(new_provider);

// Remove provider
router.unregister("old-provider");

// Update provider metrics
router.update_metrics("local", ProviderMetrics {
    avg_latency_ms: 150,
    error_rate: 0.01,
    load: 0.5,
});
```

---

## Planned Features

Currently in alpha. Planned improvements:

- [ ] Real-time load metrics collection
- [ ] Rate limit backoff and retry logic
- [ ] Cost tracking and budgeting
- [ ] A/B testing support
- [ ] Request caching
- [ ] Circuit breaker pattern
- [ ] Prometheus metrics export

---

## Dependencies

- `zoey-core` - Core runtime and types

---

## Testing

```bash
cargo test -p zoey-provider-router
```

---

## License

MIT License

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
