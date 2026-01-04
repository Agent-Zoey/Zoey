<p align="center">
  <img src="../../../assets/zoey-confident.png" alt="Zoey" width="200" />
</p>

# 🧪 SQL Storage Integration Tests

> **Your secrets are safe with Zoey**

Integration tests for Zoey's SQL database adapters—SQLite and PostgreSQL.

---

## Quick Start

### SQLite Tests (No Setup Required)

```bash
cargo test -p zoey-storage-sql --test sqlite_integration_tests
```

SQLite tests use in-memory databases and run without external dependencies.

### PostgreSQL Tests (Setup Required)

```bash
# Start PostgreSQL
docker run -d \
  --name zoey-postgres \
  -p 5432:5432 \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=zoey_test \
  postgres:15

# Run tests
export DATABASE_URL="postgresql://postgres:postgres@localhost:5432/zoey_test"
cargo test -p zoey-storage-sql --test postgres_integration_tests -- --ignored
```

---

## Test Coverage

### SQLite Tests ✅
- Agent CRUD operations
- Entity CRUD operations
- Memory CRUD operations
- Memory filtering (room, unique flag, limits)
- Integration with AgentRuntime

### PostgreSQL Tests ✅
- All SQLite tests plus:
- Concurrent operations (connection pooling)

---

## Running All Tests

```bash
# SQLite only (no external dependencies)
cargo test -p zoey-storage-sql --test sqlite_integration_tests

# Both SQLite and PostgreSQL
cargo test -p zoey-storage-sql --tests -- --ignored
```

---

## Troubleshooting

### Connection Refused

```bash
# Check PostgreSQL is running
docker ps | grep postgres
pg_isready -h localhost -p 5432
```

### Authentication Failed

```bash
# Verify credentials
export DATABASE_URL="postgresql://postgres:postgres@localhost:5432/zoey_test"
```

### Database Does Not Exist

```bash
# Create database
docker exec -it zoey-postgres psql -U postgres -c "CREATE DATABASE zoey_test;"
```

---

## Cleanup

```bash
docker stop zoey-postgres
docker rm zoey-postgres
```

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
