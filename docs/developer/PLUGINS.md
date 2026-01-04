# Creating Plugins

## 🔌 Plugin Development Guide

### Basic Plugin Template

```rust
use zoey_core::*;
use async_trait::async_trait;

pub struct MyPlugin;

#[async_trait]
impl Plugin for MyPlugin {
    fn name(&self) -> &str {
        "my-plugin"
    }
    
    fn description(&self) -> &str {
        "My custom plugin"
    }
    
    async fn init(
        &self,
        config: HashMap<String, String>,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        tracing::info!("My plugin initialized");
        Ok(())
    }
    
    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![
            Arc::new(MyAction),
        ]
    }
    
    fn providers(&self) -> Vec<Arc<dyn Provider>> {
        vec![
            Arc::new(MyProvider),
        ]
    }
}
```

### Creating Actions, Providers, and Evaluators

See `crates/plugins/zoey-plugin-bootstrap/src/` for complete examples of:
- Actions: `src/actions/reply.rs`
- Providers: `src/providers/time.rs`
- Evaluators: `src/evaluators/reflection.rs`

---

For detailed guide, see source code examples in `crates/plugins/zoey-plugin-bootstrap/`.

