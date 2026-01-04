# Configuration Reference

## ⚙️ All Configuration Options

### HIPAAConfig

```rust
pub struct HIPAAConfig {
    pub enabled: bool,              // Master switch
    pub audit_logging: bool,        // Audit all data access
    pub encryption_at_rest: bool,   // Encrypt sensitive data
    pub access_control: bool,       // Row-level security
    pub retention_days: usize,      // Retention policy
    pub auto_deidentify: bool,      // Auto de-identification
}
```

**Presets**:
- `HIPAAConfig::disabled()` - All off
- `HIPAAConfig::minimal()` - Audit only
- `HIPAAConfig::default()` - Full HIPAA
- `HIPAAConfig::maximum()` - Same as default

### RuntimeOpts

```rust
pub struct RuntimeOpts {
    pub agent_id: Option<Uuid>,
    pub character: Option<Character>,
    pub plugins: Vec<Arc<dyn Plugin>>,
    pub adapter: Option<Arc<dyn IDatabaseAdapter>>,
    pub settings: Option<HashMap<String, serde_json::Value>>,
    pub conversation_length: Option<usize>,
    pub all_available_plugins: Option<Vec<Arc<dyn Plugin>>>,
}
```

### InitializeOptions

```rust
pub struct InitializeOptions {
    pub skip_migrations: bool,
}
```

### ClusterConfig

```rust
pub struct ClusterConfig {
    pub heartbeat_interval: Duration,
    pub node_timeout: Duration,
    pub auto_rebalance: bool,
    pub replication_factor: usize,
}
```

---

See `../user-guide/CONFIGURATION.md` for usage examples.

