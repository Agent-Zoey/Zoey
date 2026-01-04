// Load .env file for environment variables
require('dotenv').config({ path: '/root/zoey-rust/.env' });

module.exports = {
  apps: [
    {
      name: 'zoey-agent',
      // Use the compiled binary (run `cargo build -p run-agent-ui --release` first)
      script: './target/release/run-agent-ui',
      // Or use cargo run directly (slower restarts):
      // script: 'cargo',
      // args: 'run -p run-agent-ui --release -- --log-level "warn,zoey_core=debug,zoey_core::agent_api::handlers=trace,zoey_provider_openai=info,zoey_plugin_bootstrap=info,zoey_plugin_knowledge=info,zoey_plugin_observability=info,zoey_plugin_compliance=info,zoey_storage_vector=info"',
      cwd: '/root/zoey-rust',
      interpreter: 'none',
      env: {
        // Library path for Vosk STT
        LD_LIBRARY_PATH: '/usr/local/lib',
        // API Keys - loaded from .env via dotenv
        OPENAI_API_KEY: process.env.OPENAI_API_KEY,
        ANTHROPIC_API_KEY: process.env.ANTHROPIC_API_KEY,
        OPENAI_MODEL: process.env.OPENAI_MODEL || 'gpt-4',
        // Discord
        DISCORD_TOKEN: process.env.DISCORD_TOKEN,
        DISCORD_APPLICATION_ID: process.env.DISCORD_APPLICATION_ID,
        // Telegram
        TELEGRAM_BOT_TOKEN: process.env.TELEGRAM_BOT_TOKEN,
        // Observability
        OBSERVABILITY_ENABLED: 'true',
        OBSERVABILITY_COST_TRACKING_ENABLED: 'true',
        OBSERVABILITY_DASHBOARD_ENABLED: 'true',
        OBSERVABILITY_REST_API_ENABLED: 'true',
        OBSERVABILITY_REST_API_HOST: '127.0.0.1',
        OBSERVABILITY_REST_API_PORT: '9100',
        AGENT_LOGS_ENABLED: 'true',
        UI_STREAMING: 'true',
        AGENT_API_PORT: '9090',
        SIMPLE_UI_PORT: '4000',
        UI_LOGS_ENABLED: 'true',
        RUST_LOG: 'warn,zoey_core=debug,zoey_core::agent_api::handlers=trace,zoey_provider_openai=info,zoey_plugin_bootstrap=info,zoey_plugin_knowledge=info,zoey_plugin_observability=info,zoey_plugin_compliance=info,zoey_storage_vector=info,zoey_plugin_lifeengine=info,zoey_adaptor_discord=debug',
      },
      // Pass log-level as argument
      args: '--log-level "warn,zoey_core=debug,zoey_core::agent_api::handlers=trace,zoey_provider_openai=info,zoey_plugin_bootstrap=info,zoey_plugin_knowledge=info,zoey_plugin_observability=info,zoey_plugin_compliance=info,zoey_storage_vector=info,zoey_plugin_lifeengine=info"',
      instances: 1,
      autorestart: true,
      watch: false,
      max_memory_restart: '2G',
      error_file: '/root/zoey-rust/logs/pm2-error.log',
      out_file: '/root/zoey-rust/logs/pm2-out.log',
      log_file: '/root/zoey-rust/logs/pm2-combined.log',
      time: true,
    },
    // Alternative: run via cargo (slower but doesn't require pre-build)
    {
      name: 'zoey-agent-dev',
      script: 'cargo',
      args: 'run -p run-agent-ui -- --log-level "warn,zoey_core=debug,zoey_core::agent_api::handlers=trace,zoey_provider_openai=info,zoey_plugin_bootstrap=info,zoey_plugin_knowledge=info,zoey_plugin_observability=info,zoey_plugin_compliance=info,zoey_storage_vector=info,zoey_plugin_x402_video=info"',
      cwd: '/root/zoey-rust',
      interpreter: 'none',
      // Load environment from .env file
      node_args: '--require dotenv/config',
      env: {
        // Observability settings
        OBSERVABILITY_ENABLED: 'true',
        OBSERVABILITY_COST_TRACKING_ENABLED: 'true',
        OBSERVABILITY_DASHBOARD_ENABLED: 'true',
        OBSERVABILITY_REST_API_ENABLED: 'true',
        OBSERVABILITY_REST_API_HOST: '127.0.0.1',
        OBSERVABILITY_REST_API_PORT: '9100',
        AGENT_LOGS_ENABLED: 'true',
        UI_STREAMING: 'true',
        AGENT_API_PORT: '9090',
        SIMPLE_UI_PORT: '4000',
        UI_LOGS_ENABLED: 'true',
        // X402 Video Plugin - Load from .env via require('dotenv').config()
        OPENAI_API_KEY: process.env.OPENAI_API_KEY,
        X402_WALLET_ADDRESS: process.env.X402_WALLET_ADDRESS || '0xfcc920316d41d8d203b99bDefB8FD688C58E78ca',
        X402_FACILITATOR_URL: process.env.X402_FACILITATOR_URL || 'https://facilitator.payai.network',
        X402_VIDEO_BASE_URL: process.env.X402_VIDEO_BASE_URL || 'https://x402.getzoey.ai',
      },
      instances: 1,
      autorestart: true,
      watch: false,
      max_memory_restart: '2G',
      error_file: '/root/zoey-rust/logs/pm2-dev-error.log',
      out_file: '/root/zoey-rust/logs/pm2-dev-out.log',
      log_file: '/root/zoey-rust/logs/pm2-dev-combined.log',
      time: true,
    },
    // Legal Zoey - AI Legal Assistant with PII Protection
    {
      name: 'zoey-law',
      script: './target/release/run-agent-ui',
      cwd: '/root/zoey-rust',
      interpreter: 'none',
      env: {
        // Character configuration
        CHARACTER_FILE: 'characters/legal-zoey.xml',
        
        // Local model configuration (Ollama)
        MODEL_PROVIDER: 'ollama',
        OLLAMA_BASE_URL: 'http://localhost:11434',
        OLLAMA_MODEL: 'llama3.2',
        
        // Database - separate from main agent
        DATABASE_URL: 'sqlite:./zoey-legal.db',
        
        // Observability settings
        OBSERVABILITY_ENABLED: 'true',
        OBSERVABILITY_COST_TRACKING_ENABLED: 'true',
        OBSERVABILITY_DASHBOARD_ENABLED: 'true',
        OBSERVABILITY_REST_API_ENABLED: 'true',
        OBSERVABILITY_REST_API_HOST: '127.0.0.1',
        OBSERVABILITY_REST_API_PORT: '9101',
        
        // Agent API and UI - different ports from main agent, bind to all interfaces
        AGENT_API_HOST: '0.0.0.0',
        AGENT_API_PORT: '9091',
        SIMPLE_UI_HOST: '0.0.0.0',
        SIMPLE_UI_PORT: '4001',
        AGENT_LOGS_ENABLED: 'true',
        UI_STREAMING: 'true',
        UI_LOGS_ENABLED: 'true',
        
        // Disable Telegram and Discord for this instance
        TELEGRAM_ENABLED: 'false',
        DISCORD_ENABLED: 'false',
        
        // PII Detection and HIPAA settings
        PII_DETECTION_ENABLED: 'true',
        PII_AUTO_REDACT: 'true',
        HIPAA_ENABLED: 'true',
        HIPAA_AUDIT_LOGGING: 'true',
        
        // AI4Privacy dataset location
        AI4PRIVACY_DATASET_DIR: '/root/eliza-rust/data/ai4privacy',
        
        // Logging
        RUST_LOG: 'warn,zoey_core=info,zoey_plugin_compliance=debug,zoey_storage_sql=info',
      },
      args: '--log-level "warn,zoey_core=info,zoey_plugin_compliance=debug"',
      instances: 1,
      autorestart: true,
      watch: false,
      max_memory_restart: '2G',
      error_file: '/root/zoey-rust/logs/zoey-law-error.log',
      out_file: '/root/zoey-rust/logs/zoey-law-out.log',
      log_file: '/root/zoey-rust/logs/zoey-law-combined.log',
      time: true,
    },
  ],
};
