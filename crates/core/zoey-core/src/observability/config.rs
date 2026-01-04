use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub enabled: bool,
    pub cost_tracking: CostTrackingConfig,
    pub prompt_storage: PromptStorageConfig,
    pub rest_api: RestApiConfig,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cost_tracking: CostTrackingConfig::default(),
            prompt_storage: PromptStorageConfig::default(),
            rest_api: RestApiConfig::default(),
        }
    }
}

impl ObservabilityConfig {
    /// Load from environment variables
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("OBSERVABILITY_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),

            cost_tracking: CostTrackingConfig::from_env(),
            prompt_storage: PromptStorageConfig::from_env(),
            rest_api: RestApiConfig::from_env(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTrackingConfig {
    pub enabled: bool,
    pub database_storage: bool,
    pub in_memory_retention_hours: u32,
    pub anomaly_multiplier: f64,
}

impl Default for CostTrackingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            database_storage: true,
            in_memory_retention_hours: 24,
            anomaly_multiplier: 2.0,
        }
    }
}

impl CostTrackingConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("OBSERVABILITY_COST_TRACKING_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),

            database_storage: std::env::var("OBSERVABILITY_DATABASE_STORAGE")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),

            in_memory_retention_hours: std::env::var("OBSERVABILITY_COST_RETENTION_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .unwrap_or(24),

            anomaly_multiplier: std::env::var("OBSERVABILITY_COST_ANOMALY_MULTIPLIER")
                .unwrap_or_else(|_| "2.0".to_string())
                .parse()
                .unwrap_or(2.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptStorageConfig {
    pub enabled: bool,
    pub mode: PromptStorageMode,
    pub sanitization: PIISanitizationLevel,
    pub retention_days: u32,
}

impl Default for PromptStorageConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for security
            mode: PromptStorageMode::HashOnly,
            sanitization: PIISanitizationLevel::Standard,
            retention_days: 90,
        }
    }
}

impl PromptStorageConfig {
    pub fn from_env() -> Self {
        let mode_str = std::env::var("OBSERVABILITY_PROMPT_STORAGE_MODE")
            .unwrap_or_else(|_| "hash_only".to_string());

        let mode = match mode_str.as_str() {
            "disabled" => PromptStorageMode::Disabled,
            "hash_only" => PromptStorageMode::HashOnly,
            "preview" => {
                let chars = std::env::var("OBSERVABILITY_PROMPT_PREVIEW_CHARS")
                    .unwrap_or_else(|_| "200".to_string())
                    .parse()
                    .unwrap_or(200);
                PromptStorageMode::Preview { chars }
            }
            "full" => PromptStorageMode::Full,
            _ => PromptStorageMode::HashOnly,
        };

        let sanitization_str = std::env::var("OBSERVABILITY_PROMPT_SANITIZATION_LEVEL")
            .unwrap_or_else(|_| "standard".to_string());

        let sanitization = match sanitization_str.as_str() {
            "none" => PIISanitizationLevel::None,
            "basic" => PIISanitizationLevel::Basic,
            "standard" => PIISanitizationLevel::Standard,
            "aggressive" => PIISanitizationLevel::Aggressive,
            "hipaa" => PIISanitizationLevel::HIPAA,
            _ => PIISanitizationLevel::Standard,
        };

        Self {
            enabled: std::env::var("OBSERVABILITY_PROMPT_STORAGE_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),

            mode,
            sanitization,

            retention_days: std::env::var("OBSERVABILITY_PROMPT_RETENTION_DAYS")
                .unwrap_or_else(|_| "90".to_string())
                .parse()
                .unwrap_or(90),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PromptStorageMode {
    Disabled,
    HashOnly,
    Preview { chars: usize },
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PIISanitizationLevel {
    None,
    Basic,
    Standard,
    Aggressive,
    HIPAA,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestApiConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

impl Default for RestApiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "127.0.0.1".to_string(), // Localhost only by default
            port: 9090,
        }
    }
}

impl RestApiConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("OBSERVABILITY_REST_API_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),

            host: std::env::var("OBSERVABILITY_REST_API_HOST")
                .unwrap_or_else(|_| "127.0.0.1".to_string()),

            port: std::env::var("OBSERVABILITY_REST_API_PORT")
                .unwrap_or_else(|_| "9100".to_string())
                .parse()
                .unwrap_or(9100),
        }
    }
}
