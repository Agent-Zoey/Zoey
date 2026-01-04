# Security Configuration Guide

## Environment Variables

ZoeyOS uses environment variables to manage sensitive configuration. **Never commit your `.env` file to version control.**

## Setup

1. Copy the example environment file:
```bash
cp .env.example .env
```

2. Edit `.env` and update with your actual values.

## Critical Security Settings

### Encryption Key
The `ENCRYPTION_KEY` is used to encrypt sensitive data at rest using AES-256-GCM.

**Requirements:**
- Minimum 32 characters
- Use strong random characters
- Never reuse across environments

**Generate a secure key:**
```bash
openssl rand -base64 32
```

### Secret Salt
The `SECRET_SALT` is used for password hashing with Argon2.

**Generate a secure salt:**
```bash
openssl rand -base64 16
```

## API Keys

### OpenAI
Get your API key from: https://platform.openai.com/api-keys

```env
OPENAI_API_KEY=sk-proj-...
```

### Anthropic (Claude)
Get your API key from: https://console.anthropic.com/settings/keys

```env
ANTHROPIC_API_KEY=sk-ant-...
```

### Local LLM (Privacy-First)
No API key needed. Install Ollama locally:

```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull phi3:mini
```

```env
LOCAL_LLM_ENDPOINT=http://localhost:11434
LOCAL_LLM_MODEL=phi3:mini
```

## Database Security

### PostgreSQL (Production)
```env
DATABASE_URL=postgresql://username:password@host:port/database?sslmode=require
```

**Best practices:**
- Use SSL/TLS (`sslmode=require`)
- Strong passwords (16+ characters)
- Restrict network access
- Regular backups
- Enable audit logging

### SQLite (Development)
```env
DATABASE_URL=sqlite:zoey.db
```

**Best practices:**
- Encrypt the database file
- Restrict file permissions: `chmod 600 zoey.db`
- Regular backups

## Compliance Modes

### Standard Mode
```env
COMPLIANCE_MODE=standard
AUDIT_LOGGING=false
```

### HIPAA Compliance
```env
COMPLIANCE_MODE=hipaa
AUDIT_LOGGING=true
ENCRYPTION_KEY=<strong-key>
DATA_RETENTION_DAYS=2555  # 7 years
LOCAL_LLM_BACKEND=ollama  # Use local models only
```

**HIPAA Requirements:**
- Use local LLM models (no cloud APIs)
- Enable audit logging
- Encrypt all PHI (Protected Health Information)
- 7-year data retention
- Access controls and authentication

### Government Mode
```env
COMPLIANCE_MODE=government
STRICT_COMPLIANCE_MODE=true
AUDIT_LOGGING=true
LOCAL_LLM_BACKEND=ollama  # Use local models only
ENABLE_DISTRIBUTED=true   # For redundancy
```

**Government Requirements:**
- Air-gapped or on-premise deployment
- Local models only (no external API calls)
- Full audit trail
- Input/Process/Output validation pipeline
- Redundant infrastructure

## Rate Limiting

Protect your API keys and prevent abuse:

```env
RATE_LIMIT_WINDOW_SECONDS=60
RATE_LIMIT_MAX_REQUESTS=100
```

## Environment-Specific Configuration

### Development
```env
RUST_LOG=debug,zoey_core=trace
DATABASE_URL=sqlite:dev.db
LOCAL_LLM_MODEL=phi3:mini  # Faster, smaller model
```

### Production
```env
RUST_LOG=warn,zoey_core=info
DATABASE_URL=postgresql://...?sslmode=require
OPENAI_MODEL=gpt-4-turbo-preview
ANTHROPIC_MODEL=claude-3-opus-20240229
```

## Security Checklist

- [ ] `.env` is in `.gitignore`
- [ ] Generated strong `ENCRYPTION_KEY` (32+ chars)
- [ ] Generated unique `SECRET_SALT`
- [ ] API keys are valid and restricted
- [ ] Database uses SSL/TLS in production
- [ ] File permissions are restricted (`chmod 600 .env`)
- [ ] Rate limiting is configured
- [ ] Audit logging enabled for compliance modes
- [ ] Using local LLM for sensitive data
- [ ] Regular security updates applied

## Key Rotation

To rotate encryption keys:

1. Generate a new key: `openssl rand -base64 32`
2. Decrypt existing data with old key
3. Re-encrypt with new key
4. Update `ENCRYPTION_KEY` in `.env`
5. Restart services

## Incident Response

If API keys are compromised:

1. **Immediately revoke** the compromised keys
2. Generate new keys
3. Update `.env` file
4. Restart all services
5. Audit logs for unauthorized access
6. Notify affected parties if required

## Additional Resources

- [ZoeyOS Security Documentation](https://github.com/zoeyos/zoey-rust/security)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [HIPAA Compliance Guide](https://www.hhs.gov/hipaa/index.html)
- [NIST Cybersecurity Framework](https://www.nist.gov/cyberframework)

