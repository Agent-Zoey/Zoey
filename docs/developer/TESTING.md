# Testing Guide

## 🧪 Running Tests

### All Tests
```bash
cargo test --workspace
```

### Unit Tests
```bash
cargo test --lib
```

### Integration Tests
```bash
cargo test --test integration_tests
```

### Load Tests
```bash
cargo test --test load_tests
```

### Benchmarks
```bash
cargo bench
```

---

## 📝 Writing Tests

### Unit Test Example
```rust
#[tokio::test]
async fn test_my_feature() {
    let runtime = create_mock_runtime();
    let message = create_test_memory("test");
    
    // Test code here
    assert!(result.is_ok());
}
```

### Integration Test Example
```rust
#[tokio::test]
async fn test_full_workflow() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();
    let runtime = AgentRuntime::new(RuntimeOpts {
        adapter: Some(Arc::new(adapter)),
        ..Default::default()
    }).await.unwrap();
    
    // Test complete workflow
}
```

---

See `tests/` directory for complete examples.

