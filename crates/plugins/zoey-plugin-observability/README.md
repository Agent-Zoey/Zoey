<p align="center">
  <img src="../../assets/zoey-confident.png" alt="Zoey" width="250" />
</p>

# 🔍 zoey-plugin-observability

> **Your secrets are safe with Zoey**

Comprehensive observability for ZoeyOS—reasoning chains, confidence scoring, and audit logging.

## Status: ✅ Beta

---

## Features

### 📊 Reasoning Chains
Track Zoey's decision-making process step by step:
- Action selection rationale
- Provider data contributions
- Evaluator scores and reasoning
- Final response generation

### 📈 Confidence Scoring
Quantify certainty in every response:
- Per-response confidence scores
- Source attribution with confidence
- Uncertainty flagging
- Calibration metrics

### 📝 Audit Logging
Complete audit trails for compliance:
- Request/response logging
- Decision chain recording
- User interaction tracking
- Timestamp and context preservation

---

## Quick Start

```rust
use zoey_core::{AgentRuntime, RuntimeOpts};
use zoey_plugin_observability::ObservabilityPlugin;
use std::sync::Arc;

let mut opts = RuntimeOpts::default();
opts.add_plugin(Arc::new(ObservabilityPlugin::new()));

let runtime = AgentRuntime::new(opts).await?;

// All Zoey responses now include observability data
```

---

## Reasoning Chain Example

```rust
use zoey_plugin_observability::{ReasoningChain, ReasoningStep};

// Access reasoning for a response
let chain = response.reasoning_chain();

for step in chain.steps() {
    println!("Step: {}", step.name);
    println!("  Input: {:?}", step.input);
    println!("  Output: {:?}", step.output);
    println!("  Confidence: {:.2}", step.confidence);
    println!("  Duration: {:?}", step.duration);
}

println!("Overall confidence: {:.2}", chain.overall_confidence());
```

## Confidence Scoring

```rust
use zoey_plugin_observability::ConfidenceScorer;

let scorer = ConfidenceScorer::new();

let score = scorer.score(&response);
println!("Confidence: {:.0}%", score.value * 100.0);
println!("Sources:");
for source in score.sources {
    println!("  - {} ({:.0}%)", source.name, source.confidence * 100.0);
}

if score.value < 0.7 {
    println!("⚠️ Low confidence response");
}
```

## Audit Logging

```rust
use zoey_plugin_observability::{AuditLogger, AuditEvent};

let logger = AuditLogger::new("./audit_logs");

// Automatically logs all interactions
// Or manually log events:
logger.log(AuditEvent::UserQuery {
    user_id: "user_123".to_string(),
    query: "What are the side effects?".to_string(),
    timestamp: Utc::now(),
});

logger.log(AuditEvent::AgentResponse {
    response_id: "resp_456".to_string(),
    content: "The common side effects include...".to_string(),
    confidence: 0.92,
    reasoning_chain_id: "chain_789".to_string(),
});
```

---

## Configuration

```rust
use zoey_plugin_observability::{ObservabilityPlugin, ObservabilityConfig};

let config = ObservabilityConfig {
    // Reasoning chains
    enable_reasoning_chains: true,
    max_chain_depth: 10,
    
    // Confidence scoring
    enable_confidence_scoring: true,
    min_confidence_threshold: 0.5,
    flag_low_confidence: true,
    
    // Audit logging
    enable_audit_logging: true,
    audit_log_path: "./audit_logs".to_string(),
    log_retention_days: 365,
    encrypt_logs: true,
    
    ..Default::default()
};

let plugin = ObservabilityPlugin::with_config(config);
```

---

## Providers

| Provider | Description |
|----------|-------------|
| `reasoning` | Current reasoning chain |
| `confidence` | Response confidence data |
| `audit` | Audit log access |

---

## Use Cases

### 🏥 Healthcare Compliance
Track every decision made about patient care:

```rust
// Audit log shows complete reasoning for medical recommendations
logger.log(AuditEvent::MedicalRecommendation {
    patient_id: "P12345",
    recommendation: "Consider blood pressure monitoring",
    reasoning_chain_id: chain.id(),
    confidence: 0.89,
    sources: vec!["CDC Guidelines 2024", "Patient History"],
});
```

### ⚖️ Legal Defensibility
Document AI reasoning for legal review:

```rust
// Export reasoning chains for legal discovery
let export = chain.export_for_legal_review();
// Includes all steps, sources, confidence scores, timestamps
```

### 🔬 Model Improvement
Analyze low-confidence responses:

```rust
// Find patterns in uncertain responses
let low_confidence = audit_log
    .query()
    .filter(|e| e.confidence < 0.7)
    .collect();

for event in low_confidence {
    // Analyze what causes uncertainty
}
```

---

## Dependencies

- `zoey-core` - Core runtime and types
- `tracing` - Structured logging

---

## Testing

```bash
cargo test -p zoey-plugin-observability
```

---

## License

MIT License

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
