//! Security Configuration Generator for ZoeyOS
//!
//! This tool generates cryptographically secure configuration files
//! with random encryption keys and salts.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use clap::{Parser, ValueEnum};
use rand::RngCore;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Output file path
    #[arg(short, long, default_value = ".env")]
    output: PathBuf,

    /// Compliance mode
    #[arg(short, long, value_enum, default_value = "standard")]
    mode: ComplianceMode,

    /// Force overwrite if file exists
    #[arg(short, long)]
    force: bool,

    /// Use local LLM only (no cloud APIs)
    #[arg(long)]
    local_only: bool,

    /// Database type
    #[arg(short, long, value_enum, default_value = "sqlite")]
    database: DatabaseType,

    /// Database connection string (optional, will use default if not provided)
    #[arg(long)]
    database_url: Option<String>,

    /// Show generated keys (WARNING: insecure, only for testing)
    #[arg(long)]
    show_keys: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ComplianceMode {
    /// Standard mode - no special compliance
    Standard,
    /// HIPAA compliant - healthcare data protection
    Hipaa,
    /// Government compliant - full audit and local processing
    Government,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DatabaseType {
    /// SQLite (file-based)
    Sqlite,
    /// PostgreSQL (production)
    Postgres,
}

fn generate_random_key(length: usize) -> String {
    let mut key = vec![0u8; length];
    rand::thread_rng().fill_bytes(&mut key);
    BASE64.encode(&key)
}

fn generate_env_content(cli: &Cli, encryption_key: &str, secret_salt: &str) -> String {
    let database_url = cli
        .database_url
        .clone()
        .unwrap_or_else(|| match cli.database {
            DatabaseType::Sqlite => "sqlite:zoey.db".to_string(),
            DatabaseType::Postgres => {
                "postgresql://zoey:password@localhost:5432/zoey?sslmode=require".to_string()
            }
        });

    let (compliance_mode, data_retention, audit_logging, strict_mode) = match cli.mode {
        ComplianceMode::Standard => ("standard", 90, false, false),
        ComplianceMode::Hipaa => ("hipaa", 2555, true, false), // 7 years
        ComplianceMode::Government => ("government", 2555, true, true),
    };

    let local_llm_model = match cli.mode {
        ComplianceMode::Standard => "phi3:mini",
        ComplianceMode::Hipaa => "llama2", // Larger model for healthcare
        ComplianceMode::Government => "llama2",
    };

    let llm_config = if cli.local_only
        || matches!(cli.mode, ComplianceMode::Hipaa | ComplianceMode::Government)
    {
        format!(
            "# OpenAI Configuration (DISABLED - Using local LLM only)\n\
             # OPENAI_API_KEY=\n\
             # OPENAI_MODEL=gpt-4\n\
             # OPENAI_TEMPERATURE=0.7\n\
             # OPENAI_MAX_TOKENS=2000\n\
             \n\
             # Anthropic (Claude) Configuration (DISABLED - Using local LLM only)\n\
             # ANTHROPIC_API_KEY=\n\
             # ANTHROPIC_MODEL=claude-3-opus-20240229\n\
             # ANTHROPIC_TEMPERATURE=0.7\n\
             # ANTHROPIC_MAX_TOKENS=2000"
        )
    } else {
        format!(
            "# OpenAI Configuration\n\
             OPENAI_API_KEY=\n\
             OPENAI_MODEL=gpt-4\n\
             OPENAI_TEMPERATURE=0.7\n\
             OPENAI_MAX_TOKENS=2000\n\
             \n\
             # Anthropic (Claude) Configuration\n\
             ANTHROPIC_API_KEY=\n\
             ANTHROPIC_MODEL=claude-3-opus-20240229\n\
             ANTHROPIC_TEMPERATURE=0.7\n\
             ANTHROPIC_MAX_TOKENS=2000"
        )
    };

    format!(
        "# ========================================\n\
         # ZoeyOS Rust - Environment Configuration\n\
         # ========================================\n\
         # Generated: {}\n\
         # Compliance Mode: {:?}\n\
         # Database: {:?}\n\
         # Local Only: {}\n\
         #\n\
         # ⚠️  SECURITY WARNING:\n\
         # - This file contains sensitive data\n\
         # - Never commit to version control\n\
         # - File permissions set to 600 (owner only)\n\
         # - Rotate keys regularly\n\
         \n\
         {}\n\
         \n\
         # Local LLM Configuration (privacy-first)\n\
         LOCAL_LLM_BACKEND=ollama\n\
         LOCAL_LLM_ENDPOINT=http://localhost:11434\n\
         LOCAL_LLM_MODEL={}\n\
         LOCAL_LLM_TEMPERATURE=0.7\n\
         LOCAL_LLM_MAX_TOKENS=2000\n\
         \n\
         # Database Configuration\n\
         DATABASE_URL={}\n\
         \n\
         # Security Configuration (AUTO-GENERATED)\n\
         # These keys are cryptographically random and unique to this installation\n\
         ENCRYPTION_KEY={}\n\
         SECRET_SALT={}\n\
         \n\
         # Server Configuration\n\
         SERVER_HOST=127.0.0.1\n\
         SERVER_PORT=3000\n\
         ENABLE_CORS=true\n\
         \n\
         # Logging Configuration\n\
         RUST_LOG=info,zoey_core=debug\n\
         LOG_LEVEL=info\n\
         \n\
         # Rate Limiting\n\
         RATE_LIMIT_WINDOW_SECONDS=60\n\
         RATE_LIMIT_MAX_REQUESTS=100\n\
         \n\
         # Feature Flags\n\
         ENABLE_DISTRIBUTED=false\n\
         ENABLE_STREAMING=true\n\
         ENABLE_EMBEDDINGS=true\n\
         STRICT_COMPLIANCE_MODE={}\n\
         \n\
         # Compliance Settings\n\
         COMPLIANCE_MODE={}\n\
         DATA_RETENTION_DAYS={}\n\
         AUDIT_LOGGING={}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        cli.mode,
        cli.database,
        cli.local_only,
        llm_config,
        local_llm_model,
        database_url,
        encryption_key,
        secret_salt,
        strict_mode,
        compliance_mode,
        data_retention,
        audit_logging,
    )
}

fn main() {
    let cli = Cli::parse();

    // Check if file exists
    if cli.output.exists() && !cli.force {
        eprintln!("❌ Error: File {:?} already exists!", cli.output);
        eprintln!("   Use --force to overwrite");
        std::process::exit(1);
    }

    println!("🔐 ZoeyOS Security Configuration Generator");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    // Generate cryptographically secure random keys
    println!("📝 Generating cryptographically secure keys...");
    let encryption_key = generate_random_key(32); // 256 bits for AES-256
    let secret_salt = generate_random_key(24); // 192 bits for salt

    if cli.show_keys {
        println!("⚠️  WARNING: Showing keys (DO NOT use in production)");
        println!("   ENCRYPTION_KEY: {}", encryption_key);
        println!("   SECRET_SALT: {}", secret_salt);
    } else {
        println!("✓ Generated ENCRYPTION_KEY (32 bytes, base64)");
        println!("✓ Generated SECRET_SALT (24 bytes, base64)");
    }
    println!();

    // Generate configuration content
    println!("📄 Generating configuration for {:?} mode...", cli.mode);
    let content = generate_env_content(&cli, &encryption_key, &secret_salt);

    // Write to file
    match fs::write(&cli.output, content) {
        Ok(_) => {
            println!("✓ Configuration written to: {:?}", cli.output);
        }
        Err(e) => {
            eprintln!("❌ Failed to write file: {}", e);
            std::process::exit(1);
        }
    }

    // Set secure permissions (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        if let Err(e) = fs::set_permissions(&cli.output, perms) {
            eprintln!("⚠️  Warning: Could not set file permissions: {}", e);
        } else {
            println!("✓ Set secure file permissions (600 - owner only)");
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Configuration generated successfully!");
    println!();
    println!("📋 Next Steps:");
    println!("   1. Edit {:?} and add your API keys", cli.output);

    match cli.mode {
        ComplianceMode::Standard => {
            println!("   2. (Optional) Add OPENAI_API_KEY or ANTHROPIC_API_KEY");
            println!("   3. Or use local LLM: Install Ollama and run 'ollama pull phi3:mini'");
        }
        ComplianceMode::Hipaa => {
            println!("   2. ⚠️  HIPAA Mode: Only local LLM allowed (cloud APIs disabled)");
            println!("   3. Install Ollama: curl -fsSL https://ollama.com/install.sh | sh");
            println!("   4. Pull model: ollama pull llama2");
            println!("   5. Review SECURITY.md for HIPAA compliance checklist");
        }
        ComplianceMode::Government => {
            println!("   2. ⚠️  Government Mode: Strict compliance enabled");
            println!("   3. Install Ollama: curl -fsSL https://ollama.com/install.sh | sh");
            println!("   4. Pull model: ollama pull llama2");
            println!("   5. Review SECURITY.md for government compliance requirements");
            println!("   6. Enable audit logging and review IPO pipeline settings");
        }
    }

    println!("   • Read SECURITY.md for detailed security configuration");
    println!("   • Never commit .env to version control");
    println!("   • Rotate keys regularly in production");
    println!();
}
