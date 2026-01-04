# Generate-Config Examples

## Quick Examples

### 1. Standard Development Setup
```bash
cargo run --bin generate-config
```
Creates `.env` with:
- Random encryption keys
- SQLite database
- Cloud LLM APIs enabled
- Standard compliance

### 2. HIPAA Healthcare Setup
```bash
cargo run --bin generate-config -- --mode hipaa --database postgres
```
Creates `.env` with:
- Random encryption keys
- PostgreSQL database
- **Cloud APIs disabled** (local LLM only)
- 7-year data retention
- Audit logging enabled

### 3. Government Deployment
```bash
cargo run --bin generate-config -- --mode government --database postgres
```
Creates `.env` with:
- Random encryption keys
- PostgreSQL database
- Strict compliance mode
- Local LLM only
- Full audit trail

### 4. Privacy-First Setup
```bash
cargo run --bin generate-config -- --local-only
```
Creates `.env` with:
- Random encryption keys
- Local LLM only (no cloud)
- Standard compliance
- Maximum privacy

### 5. Custom Database
```bash
cargo run --bin generate-config -- \
    --database postgres \
    --database-url "postgresql://user:pass@db.example.com/zoey?sslmode=require"
```

### 6. Multiple Environments

**Development:**
```bash
cargo run --bin generate-config -- --output .env.dev
```

**Staging:**
```bash
cargo run --bin generate-config -- \
    --output .env.staging \
    --mode hipaa \
    --database postgres
```

**Production:**
```bash
cargo run --bin generate-config -- \
    --output .env.production \
    --mode government \
    --database postgres \
    --database-url "$PROD_DATABASE_URL"
```

## Comparison of Modes

| Feature | Standard | HIPAA | Government |
|---------|----------|-------|------------|
| Cloud LLM APIs | ✅ Enabled | ❌ Disabled | ❌ Disabled |
| Local LLM | Optional | ✅ Required | ✅ Required |
| Data Retention | 90 days | 7 years | 7 years |
| Audit Logging | ❌ Optional | ✅ Enabled | ✅ Enabled |
| Strict Mode | ❌ Disabled | ❌ Disabled | ✅ Enabled |
| IPO Pipeline | Optional | Recommended | ✅ Required |
| PII Detection | Optional | ✅ Required | ✅ Required |

## Testing the Generated Config

After generation, verify the keys were created:

```bash
# Check file was created with secure permissions
ls -l .env

# Verify keys exist (without showing values)
grep -E "^(ENCRYPTION_KEY|SECRET_SALT)" .env | cut -d= -f1

# Count key length (should be 44+ chars for encryption, 32+ for salt)
grep "^ENCRYPTION_KEY=" .env | cut -d= -f2 | wc -c
```

## CI/CD Integration

### GitHub Actions

```yaml
- name: Generate secure config
  run: |
    cargo run --bin generate-config -- \
      --mode ${{ secrets.COMPLIANCE_MODE }} \
      --database postgres \
      --database-url "${{ secrets.DATABASE_URL }}"
    
    # Add API keys from secrets
    echo "OPENAI_API_KEY=${{ secrets.OPENAI_KEY }}" >> .env
```

### Docker Compose

```yaml
services:
  zoey:
    build: .
    volumes:
      - ./generate-config.sh:/app/generate-config.sh
    command: sh -c "/app/generate-config.sh && ./zoey"
```

**generate-config.sh:**
```bash
#!/bin/bash
./target/release/generate-config \
    --mode ${COMPLIANCE_MODE:-standard} \
    --database ${DATABASE_TYPE:-postgres} \
    --database-url "${DATABASE_URL}"
```

## Regenerating Keys

To rotate security keys:

```bash
# Backup old config
cp .env .env.backup

# Generate new config
cargo run --bin generate-config -- --force

# Manually copy over API keys from backup if needed
```

## Advanced Usage

### Custom Key Length (Future)

Currently generates:
- ENCRYPTION_KEY: 32 bytes (256 bits)
- SECRET_SALT: 24 bytes (192 bits)

These are optimal for AES-256-GCM and Argon2.

### Environment-Specific Configs

**Development:**
```bash
cargo run --bin generate-config -- \
    --output .env.development \
    --mode standard
```

**Production:**
```bash
cargo run --bin generate-config -- \
    --output .env.production \
    --mode hipaa \
    --database postgres \
    --database-url "postgresql://prod_user:$DB_PASS@prod-db:5432/zoey?sslmode=require"
```

## Security Best Practices

1. **Never use `--show-keys` in production**
2. **Rotate keys every 90 days**
3. **Use different keys per environment**
4. **Store backups encrypted**
5. **Restrict file permissions to 600**
6. **Use vault/secrets manager for CI/CD**

## Troubleshooting

### Issue: File already exists

```
❌ Error: File ".env" already exists!
   Use --force to overwrite
```

**Solution:** Use `--force` or specify different output file

### Issue: Permission denied

**Solution:** Check directory permissions or use sudo (not recommended)

### Issue: Keys look wrong

Keys should be base64-encoded strings, looking like:
```
ENCRYPTION_KEY=zlOhAz8CRw57K8dw0kqcTBAdMchjjSGblDdHQ+T2nOI=
SECRET_SALT=qfeyRcPyPcLRQtfqqaMP/I6vMdJb+m7A
```

If they don't, regenerate with `--force`.

## See Also

- [SECURITY.md](../../SECURITY.md) - Complete security guide
- [README.md](../../README.md) - ZoeyOS main documentation
