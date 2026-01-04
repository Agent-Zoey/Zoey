# Generate Config Tool

Secure configuration generator for ZoeyOS with cryptographically random keys.

## Features

- 🔐 Generates cryptographically secure random keys
- 🏥 Supports multiple compliance modes (Standard, HIPAA, Government)
- 🗄️ Configurable database types (SQLite, PostgreSQL)
- 🔒 Sets secure file permissions (600) automatically
- ⚡ Fast and lightweight CLI tool

## Installation

The tool is built as part of the ZoeyOS workspace:

```bash
cargo build --bin generate-config
```

## Usage

### Basic Usage

Generate a standard configuration:

```bash
cargo run --bin generate-config
```

This creates `.env` with:
- Randomly generated encryption key (32 bytes)
- Randomly generated secret salt (24 bytes)
- SQLite database
- Standard compliance mode

### HIPAA Compliance Mode

Generate HIPAA-compliant configuration:

```bash
cargo run --bin generate-config -- --mode hipaa
```

Features:
- ✅ Cloud APIs disabled (local LLM only)
- ✅ 7-year data retention (2555 days)
- ✅ Audit logging enabled
- ✅ Larger model (llama2) for healthcare accuracy

### Government Compliance Mode

Generate government-compliant configuration:

```bash
cargo run --bin generate-config -- --mode government
```

Features:
- ✅ Strict compliance mode
- ✅ Local LLM only (air-gapped ready)
- ✅ Full audit trail
- ✅ 7-year data retention

### Custom Output Location

Generate configuration to a custom file:

```bash
cargo run --bin generate-config -- --output /path/to/config.env
```

### PostgreSQL Database

Generate with PostgreSQL configuration:

```bash
cargo run --bin generate-config -- --database postgres
```

Or with custom connection string:

```bash
cargo run --bin generate-config -- \
    --database postgres \
    --database-url "postgresql://user:pass@localhost/zoey?sslmode=require"
```

### Force Overwrite

Overwrite existing file:

```bash
cargo run --bin generate-config -- --force
```

### Local-Only Mode

Disable cloud APIs regardless of compliance mode:

```bash
cargo run --bin generate-config -- --local-only
```

## Command Reference

```
Usage: generate-config [OPTIONS]

Options:
  -o, --output <OUTPUT>
          Output file path [default: .env]

  -m, --mode <MODE>
          Compliance mode [default: standard]
          Possible values: standard, hipaa, government

  -f, --force
          Force overwrite if file exists

      --local-only
          Use local LLM only (no cloud APIs)

  -d, --database <DATABASE>
          Database type [default: sqlite]
          Possible values: sqlite, postgres

      --database-url <DATABASE_URL>
          Database connection string (optional)

      --show-keys
          Show generated keys (WARNING: insecure, testing only)

  -h, --help
          Print help

  -V, --version
          Print version
```

## Examples

### Development Setup

```bash
# Generate standard config for development
cargo run --bin generate-config

# Add your API keys to .env
# Start developing!
```

### HIPAA Healthcare Deployment

```bash
# Generate HIPAA config
cargo run --bin generate-config -- \
    --mode hipaa \
    --database postgres \
    --database-url "postgresql://hipaa_user:secure_pass@localhost/healthcare?sslmode=require"

# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull HIPAA-appropriate model
ollama pull llama2

# Deploy with strict compliance
```

### Government Deployment

```bash
# Generate government config
cargo run --bin generate-config -- \
    --mode government \
    --database postgres

# Review and deploy on air-gapped infrastructure
```

### Testing (Show Keys)

```bash
# Generate config and show keys (for testing only!)
cargo run --bin generate-config -- \
    --output .env.test \
    --show-keys

# ⚠️ WARNING: Never use --show-keys in production!
```

## Security

### Generated Keys

**ENCRYPTION_KEY** (32 bytes, base64):
- Used for AES-256-GCM encryption
- Cryptographically random via `rand::thread_rng()`
- 256 bits of entropy

**SECRET_SALT** (24 bytes, base64):
- Used for Argon2 password hashing
- Cryptographically random
- 192 bits of entropy

### File Permissions

On Unix systems, the tool automatically sets:
```
chmod 600 .env    # -rw------- (owner only)
```

### Best Practices

1. **Never show keys**: Don't use `--show-keys` in production
2. **Secure storage**: Keep `.env` file permissions at 600
3. **Version control**: `.env` is in `.gitignore` (never commit)
4. **Key rotation**: Generate new config regularly
5. **Backup safely**: Encrypt backups of `.env` file

## Integration

### In CI/CD

```bash
# Generate config in deployment pipeline
./target/release/generate-config \
    --mode hipaa \
    --database postgres \
    --database-url "$DATABASE_URL"

# Inject additional secrets from vault
echo "OPENAI_API_KEY=$VAULT_OPENAI_KEY" >> .env
```

### Docker

```dockerfile
# Generate config at container start
RUN cargo build --release --bin generate-config
ENTRYPOINT ["sh", "-c", "./target/release/generate-config && ./your-app"]
```

## Troubleshooting

### File Already Exists

```bash
Error: File ".env" already exists!
   Use --force to overwrite
```

**Solution**: Use `--force` flag or delete the existing file

### Permission Denied

```bash
Error: Permission denied
```

**Solution**: Run with appropriate permissions or change output directory

## Development

Built with:
- `clap` - Command-line argument parsing
- `rand` - Cryptographically secure random number generation
- `base64` - Key encoding
- `chrono` - Timestamps

## License

MIT License - Part of ZoeyOS Rust project

