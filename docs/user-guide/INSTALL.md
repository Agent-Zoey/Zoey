<p align="center">
  <img src="../../crates/assets/zoey-confident.png" alt="Zoey" width="250" />
</p>

# 📦 Installation Guide - ZoeyOS

> **Your secrets are safe with Zoey**

---

## Prerequisites

### 1. Install Rust

The project requires Rust 1.75.0 or later.

```bash
# Install Rust using rustup (recommended)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Follow the prompts, then reload your shell
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version
```

### 2. Install System Dependencies

#### Linux (Debian/Ubuntu)
```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev
```

#### Linux (Fedora/RHEL)
```bash
sudo dnf install -y gcc pkg-config openssl-devel
```

#### macOS
```bash
# Install Xcode Command Line Tools
xcode-select --install

# Or install via Homebrew
brew install openssl pkg-config
```

### 3. Install Database (Optional)

For production use, install PostgreSQL:

#### Linux
```bash
# Debian/Ubuntu
sudo apt install -y postgresql postgresql-contrib

# Fedora/RHEL
sudo dnf install -y postgresql postgresql-server
```

#### macOS
```bash
brew install postgresql
brew services start postgresql
```

#### Docker (All Platforms)
```bash
docker run -d \
  --name zoey-postgres \
  -e POSTGRES_PASSWORD=password \
  -e POSTGRES_DB=zoey \
  -p 5432:5432 \
  postgres:15
```

---

## Building the Project

### 1. Clone Repository

```bash
git clone https://github.com/Freytes/ZoeyAI
cd ZoeyAI
```

### 2. Build All Crates

```bash
# Debug build (faster compilation, slower runtime)
cargo build

# Release build (optimized, recommended for production)
cargo build --release
```

### 3. Build Specific Crates

```bash
# Build only core
cargo build -p zoey-core

# Build only SQL storage
cargo build -p zoey-storage-sql

# Build with verbose output
cargo build --verbose
```

---

## Running Tests

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p zoey-core

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_runtime_creation
```

---

## Running Examples

### Basic Agent Example

```bash
# Run the basic agent example
cargo run --example basic_agent

# Run with release optimizations
cargo run --release --example basic_agent
```

---

## Development Setup

### 1. Install Development Tools

```bash
# Rust formatter
rustup component add rustfmt

# Rust linter
rustup component add clippy

# Rust language server (for IDE support)
rustup component add rust-analyzer
```

### 2. Setup IDE

#### VS Code
```bash
# Install Rust extension
code --install-extension rust-lang.rust-analyzer
```

#### IntelliJ IDEA / CLion
- Install Rust plugin from marketplace

### 3. Pre-commit Checks

```bash
# Format code
cargo fmt --all

# Check for lints
cargo clippy --all-targets --all-features

# Check compilation without building
cargo check --workspace
```

---

## Configuration

### Environment Variables

Create a `.env` file in the project root:

```bash
# Database
DATABASE_URL=postgresql://postgres:password@localhost/zoey
# or for SQLite:
# DATABASE_URL=sqlite:./zoey.db

# Logging
RUST_LOG=info,zoey_core=debug

# Local LLM (recommended for privacy)
OLLAMA_BASE_URL=http://localhost:11434
DEFAULT_MODEL=llama3.2

# Cloud providers (optional - data leaves your network)
OPENAI_API_KEY=your_key_here
ANTHROPIC_API_KEY=your_key_here
```

### Logging Configuration

```bash
# Set log level
export RUST_LOG=debug

# Filter specific modules
export RUST_LOG=zoey_core=debug,zoey_plugin_sql=info

# Log format options
export RUST_LOG_FORMAT=json  # or "pretty" or "compact"
```

---

## Database Setup

### PostgreSQL

```bash
# Create database
createdb zoey

# Connect and create tables (done automatically by adapter)
# Or run migrations manually:
psql zoey < schema.sql
```

### SQLite

```bash
# SQLite database is created automatically
# For in-memory: Use ":memory:" as database URL
# For file: Use "sqlite:./zoey.db"
```

---

## Troubleshooting

### Compilation Errors

**Error**: `linker 'cc' not found`
```bash
# Install build tools
sudo apt install build-essential  # Debian/Ubuntu
sudo dnf install gcc              # Fedora/RHEL
```

**Error**: `Could not find OpenSSL`
```bash
# Install OpenSSL development files
sudo apt install libssl-dev pkg-config  # Debian/Ubuntu
sudo dnf install openssl-devel          # Fedora/RHEL
```

### Database Connection Errors

**Error**: `connection refused`
```bash
# Check PostgreSQL is running
systemctl status postgresql

# Start if not running
systemctl start postgresql

# Check connection
psql -U postgres -h localhost
```

**Error**: `authentication failed`
```bash
# Update DATABASE_URL with correct credentials
export DATABASE_URL=postgresql://username:password@localhost/zoey
```

---

## Docker Deployment

### Build Docker Image

```Dockerfile
# Dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/zoey-agent /usr/local/bin/
CMD ["zoey-agent"]
```

```bash
# Build image
docker build -t zoey-ai .

# Run container
docker run -e DATABASE_URL=postgresql://host.docker.internal/zoey zoey-ai
```

---

## Verification

After installation, verify everything works:

```bash
# Check Rust version
rustc --version

# Build project
cargo build

# Run tests
cargo test

# Run example
cargo run --example basic_agent

# Check lints
cargo clippy

# Format code
cargo fmt --check
```

If all commands succeed, your installation is complete!

---

## Next Steps

1. Read the [Architecture Guide](../developer/ARCHITECTURE.md)
2. Review the [Examples](EXAMPLES.md)
3. Try modifying the [Basic Agent Example](../../examples/basic_agent.rs)
4. Create your first custom plugin

---

## Getting Help

- **Documentation**: See `docs/` directory
- **Examples**: See `examples/` directory
- **Issues**: https://github.com/Freytes/ZoeyAI/issues

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
