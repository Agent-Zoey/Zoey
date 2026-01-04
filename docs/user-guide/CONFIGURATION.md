# ZoeyOS - Configuration Guide

## Flexible Configuration

All compliance features are optional and can be enabled/disabled based on your needs.

---

## Configuration Scenarios

### Scenario 1: Standard Agent (No Compliance)

**Use Case**: General chatbots, customer service, gaming, personal assistants

**Plugins**:
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
// Configure OpenAI via environment: OPENAI_API_KEY=sk-...

let plugins = vec![
    Arc::new(BootstrapPlugin::new()),  // Core functionality
];
```

**Features**:
- ✅ Full agent functionality
- ✅ Cloud AI (GPT-4, Claude)
- ✅ Best model quality
- ✅ No compliance overhead
- ❌ No HIPAA features
- ❌ No PII scanning
- ❌ No audit logging (required)

**Example**: `examples/standard_agent.rs`

---

### Scenario 2: Enterprise (Minimal Compliance)

**Use Case**: Enterprise deployments, internal tools, moderate security

**Plugins**:
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
// Configure providers via environment variables

let plugins = vec![
    Arc::new(BootstrapPlugin::new()),
];

// Minimal HIPAA (audit only)
let hipaa_config = HIPAAConfig::minimal();
```

**Features**:
- ✅ Full functionality
- ✅ Cloud + Local AI
- ✅ Audit logging only
- ✅ Moderate overhead
- ❌ No encryption (unless needed)
- ❌ No PII scanning (unless needed)

---

### Scenario 3: Healthcare (Full HIPAA)

**Use Case**: Healthcare providers, medical systems, PHI handling

**Plugins**:
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_compliance::CompliancePlugin;
// Configure local LLM via OLLAMA_BASE_URL environment variable

let plugins = vec![
    Arc::new(CompliancePlugin::new()),  // PII protection
    Arc::new(BootstrapPlugin::new()),
];

// Full HIPAA compliance
let hipaa_config = HIPAAConfig::default(); // or ::maximum()
```

**Features**:
- ✅ HIPAA compliant
- ✅ PII detection
- ✅ Local LLM only
- ✅ Audit logging
- ✅ Encryption
- ✅ Access control
- ✅ 7-year retention

**Example**: `examples/government_compliant_agent.rs`

---

### Scenario 4: Government (Maximum Security)

**Use Case**: Government agencies, defense, intelligence, air-gapped

**Plugins**:
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_compliance::CompliancePlugin;
// Configure local LLM only via OLLAMA_BASE_URL

let plugins = vec![
    Arc::new(CompliancePlugin::new()),  // PII protection
    Arc::new(BootstrapPlugin::new()),
];

// Government mode
let pipeline = create_government_pipeline(); // Strict + local-only
let hipaa_config = HIPAAConfig::maximum();
```

**Features**:
- ✅ Maximum security
- ✅ Local processing only
- ✅ PII detection
- ✅ Complete audit trail
- ✅ Encryption everywhere
- ✅ IPO pattern enforced
- ✅ Air-gap compatible

**Example**: `examples/government_compliant_agent.rs`

---

### Scenario 5: Hybrid (Flexible)

**Use Case**: Enterprise with some sensitive data, configurable per use case

**Plugins**:
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_compliance::CompliancePlugin;
// Configure both local and cloud providers via environment variables

let plugins = vec![
    Arc::new(CompliancePlugin::new()),   // Optional PII scanning
    Arc::new(BootstrapPlugin::new()),
];

// Selective HIPAA
let hipaa_config = HIPAAConfig {
    enabled: true,
    audit_logging: true,    // YES - audit everything
    encryption_at_rest: false, // NO - not needed for this data
    access_control: true,   // YES - isolate agents
    retention_days: 365,    // 1 year
    auto_deidentify: false, // NO - data not that sensitive
};
```

**Features**:
- ✅ Flexible per message
- ✅ Local LLM for sensitive data
- ✅ Cloud AI for general data
- ✅ Selective compliance
- ✅ PII scanning optional

---

## Configuration Matrix

| Feature | How to Enable | How to Disable |
|---------|---------------|----------------|
| **HIPAA** | `HIPAAConfig::default()` | `HIPAAConfig::disabled()` |
| **PII Scanning** | Include `CompliancePlugin` | Don't include plugin |
| **Local LLM** | Set `OLLAMA_BASE_URL` env var | Don't set env var |
| **Cloud AI** | Set `OPENAI_API_KEY` env var | Don't set env var |
| **Audit Logging** | `audit_logging: true` | `audit_logging: false` |
| **Encryption** | `encryption_at_rest: true` | `encryption_at_rest: false` |
| **Access Control** | `access_control: true` | `access_control: false` |
| **Planning Functors** | Included in Bootstrap | Always included |

---

## HIPAA Configuration Options

### Option 1: Disabled (No Healthcare)
```rust
let hipaa_config = HIPAAConfig::disabled();
// All HIPAA features OFF
```

### Option 2: Minimal (Audit Only)
```rust
let hipaa_config = HIPAAConfig::minimal();
// Only audit logging enabled
```

### Option 3: Custom (Pick and Choose)
```rust
let hipaa_config = HIPAAConfig {
    enabled: true,
    audit_logging: true,        // ✅ YES
    encryption_at_rest: false,  // ❌ NO
    access_control: true,       // ✅ YES
    retention_days: 730,        // 2 years
    auto_deidentify: false,     // ❌ NO
};
```

### Option 4: Maximum (Full Compliance)
```rust
let hipaa_config = HIPAAConfig::default(); // or ::maximum()
// All features enabled, 7-year retention
```

---

## Plugin Combinations

### Minimal (No Compliance)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
vec![Arc::new(BootstrapPlugin::new())]
// Just core functionality
```

### Standard (Cloud AI)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
// Configure via: OPENAI_API_KEY=sk-...
vec![Arc::new(BootstrapPlugin::new())]
// Core + best quality AI
```

### Privacy-Conscious (Local AI)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
// Configure via: OLLAMA_BASE_URL=http://localhost:11434
vec![Arc::new(BootstrapPlugin::new())]
// Core + local processing
```

### Enterprise (Hybrid)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
// Configure both local and cloud via environment variables
vec![Arc::new(BootstrapPlugin::new())]
// Local first, cloud fallback via provider router
```

### Healthcare (HIPAA)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_compliance::CompliancePlugin;
// Configure local LLM only via: OLLAMA_BASE_URL=...
vec![
    Arc::new(CompliancePlugin::new()),  // PII protection
    Arc::new(BootstrapPlugin::new()),
]
// Maximum compliance
```

### Government (Maximum Security)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_compliance::CompliancePlugin;
// Local LLM only via environment
vec![
    Arc::new(CompliancePlugin::new()),
    Arc::new(BootstrapPlugin::new()),
]
// + create_government_pipeline()
// Strictest mode
```

---

## Runtime Configuration

### Standard Configuration
```rust
let runtime = AgentRuntime::new(RuntimeOpts {
    character: Some(character),
    adapter: Some(Arc::new(adapter)),
    plugins: standard_plugins,
    conversation_length: Some(32),
    ..Default::default()
}).await?;

// Initialize normally
runtime.write().unwrap()
    .initialize(InitializeOptions::default()).await?;
```

### With Selective HIPAA
```rust
// Create adapter
let adapter = PostgresAdapter::new(&database_url).await?;

// Configure HIPAA
let hipaa_config = HIPAAConfig {
    enabled: true,
    audit_logging: true,     // Enable
    encryption_at_rest: false, // Disable
    ..Default::default()
};

// Initialize HIPAA (only if using PostgreSQL)
let hipaa = HIPAACompliance::new(pool, hipaa_config);
hipaa.initialize().await?;
```

### Government Mode (Strictest)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_compliance::CompliancePlugin;

// Must use local LLM - configure via environment
// OLLAMA_BASE_URL=http://localhost:11434

// Must include compliance
let compliance = CompliancePlugin::new();

// Full HIPAA
let hipaa_config = HIPAAConfig::maximum();

// Strict pipeline
let pipeline = create_government_pipeline();
```

---

## Decision Guide

### Do I Need HIPAA?

**YES** if you handle:
- Protected Health Information (PHI)
- Medical records
- Healthcare data
- Patient information

**NO** if you handle:
- General customer data
- Public information
- Gaming content
- General chat

### Do I Need PII Detection?

**YES** if you:
- Handle user personal data
- Have compliance requirements
- Work with sensitive information
- Need audit trail

**NO** if you:
- Handle only public data
- Internal testing
- Non-sensitive use cases
- Controlled environment

### Do I Need Local LLM?

**YES** if you:
- Cannot send data externally
- Have data sovereignty requirements
- Work in air-gapped network
- Government/healthcare deployment

**NO** if you:
- Can use cloud APIs
- Want best model quality
- Don't handle sensitive data
- Cost is not a concern

---

## Performance Impact

### Standard Mode (No Compliance)
- **Overhead**: None
- **Performance**: Maximum (10-50x vs TypeScript)
- **Latency**: Minimum (<1ms local processing)
- **Cost**: API costs only

### With HIPAA Minimal (Audit Only)
- **Overhead**: ~5% (audit logging)
- **Performance**: 95% of maximum
- **Latency**: +0.5ms (write to audit table)
- **Cost**: Storage for audit logs

### With Full HIPAA
- **Overhead**: ~10-15% (audit + encryption + RLS)
- **Performance**: 85-90% of maximum
- **Latency**: +1-2ms (encryption + access checks)
- **Cost**: Storage + encryption CPU

### With PII Scanning (Judgment Plugin)
- **Overhead**: ~5% (regex scanning)
- **Performance**: 95% of maximum
- **Latency**: +0.5ms (scan input/output)
- **Cost**: None (CPU only)

### With Local LLM
- **Overhead**: None (actually better!)
- **Performance**: Depends on hardware
- **Latency**: Depends on model size
- **Cost**: Zero API costs!

**Bottom Line**: Choose features based on your needs. Standard mode has zero overhead!

---

## Configuration Examples

### Example 1: Gaming NPC (Minimal)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;

RuntimeOpts {
    plugins: vec![Arc::new(BootstrapPlugin::new())],
    ..Default::default()
}
// No AI, just core functionality
```

### Example 2: Customer Service (Standard)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
// Configure via: OPENAI_API_KEY=sk-...

RuntimeOpts {
    plugins: vec![Arc::new(BootstrapPlugin::new())],
    ..Default::default()
}
// Cloud AI, no compliance
```

### Example 3: Corporate (Enterprise)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
// Configure both providers via environment variables

RuntimeOpts {
    plugins: vec![Arc::new(BootstrapPlugin::new())],
    ..Default::default()
}
// Hybrid: local preferred, cloud fallback
```

### Example 4: Medical (HIPAA)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_compliance::CompliancePlugin;
// Configure local LLM via: OLLAMA_BASE_URL=...

RuntimeOpts {
    plugins: vec![
        Arc::new(CompliancePlugin::new()),
        Arc::new(BootstrapPlugin::new()),
    ],
    ..Default::default()
}
// + HIPAAConfig::default()
```

---

## Feature Matrix

| Feature | Always On | Optional | How to Enable |
|---------|-----------|----------|---------------|
| Core Runtime | ✅ | ❌ | Automatic |
| Plugin System | ✅ | ❌ | Automatic |
| Actions/Providers | ✅ | ❌ | Via Bootstrap |
| Planning Functors | ✅ | ❌ | Via Bootstrap |
| State Composition | ✅ | ❌ | Automatic |
| **HIPAA Features** | ❌ | ✅ | `HIPAAConfig` |
| **PII Detection** | ❌ | ✅ | `CompliancePlugin` |
| **Local LLM** | ❌ | ✅ | `OLLAMA_BASE_URL` env var |
| **Cloud AI** | ❌ | ✅ | `OPENAI_API_KEY`/`ANTHROPIC_API_KEY` env vars |
| **Audit Logging** | ❌ | ✅ | `audit_logging: true` |
| **Encryption** | ❌ | ✅ | `encryption_at_rest: true` |
| **IPO Strict Mode** | ❌ | ✅ | `create_government_pipeline()` |

---

## Quick Reference

### Disable All Compliance
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
// Standard mode - fastest, simplest
let plugins = vec![Arc::new(BootstrapPlugin::new())];
let hipaa_config = HIPAAConfig::disabled();
```

### Enable Only What You Need
```rust
// Custom configuration
let hipaa_config = HIPAAConfig {
    enabled: true,
    audit_logging: true,     // ✅ Enable
    encryption_at_rest: false, // ❌ Disable
    access_control: true,    // ✅ Enable
    retention_days: 365,     // 1 year (not 7)
    auto_deidentify: false,  // ❌ Disable
};
```

### Enable Everything (Maximum Compliance)
```rust
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_compliance::CompliancePlugin;
// Configure local LLM via OLLAMA_BASE_URL

// Government/healthcare mode
let plugins = vec![
    Arc::new(CompliancePlugin::new()),
    Arc::new(BootstrapPlugin::new()),
];
let hipaa_config = HIPAAConfig::maximum();
let pipeline = create_government_pipeline();
```

---

## Examples Provided

| Example | Compliance Level | Use Case |
|---------|------------------|----------|
| `basic_agent.rs` | None | Learning, testing |
| `standard_agent.rs` | None | General purpose |
| `advanced_agent.rs` | Partial | Production features |
| `government_compliant_agent.rs` | Maximum | Gov/healthcare |

Run any example:
```bash
cargo run --example standard_agent           # No compliance
cargo run --example government_compliant_agent # Full compliance
```

---

## Summary

### Key Points

1. **HIPAA is OPTIONAL** - Enable only if needed
2. **PII scanning is OPTIONAL** - Add CompliancePlugin if needed
3. **Local LLM is OPTIONAL** - Configure via OLLAMA_BASE_URL if needed
4. **Cloud AI is OPTIONAL** - Configure via OPENAI_API_KEY/ANTHROPIC_API_KEY if allowed
5. **Mix and match** - Use any combination

### Default Behavior

**Without configuration**: Standard mode (no compliance overhead)

**With `HIPAAConfig::default()`**: Full HIPAA compliance

**With `HIPAAConfig::disabled()`**: Explicitly disable all HIPAA features

### Performance

- **Standard mode**: Maximum performance, zero overhead
- **Compliance mode**: 85-95% performance (still 10-50x faster than TypeScript!)

---

## Flexibility

ZoeyOS is flexible by design:

- Use compliance features only when needed
- No overhead for standard use cases
- Easy to enable/disable per deployment
- Mix local and cloud AI as appropriate

You decide what's right for your use case.

See examples for complete code.

**Recommendation**: Start with `standard_agent.rs`, add compliance only when required.

