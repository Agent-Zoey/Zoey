//! Secrets management utilities

use crate::types::Character;
use std::collections::HashMap;

/// Environment variables that should NEVER be imported into character settings
/// These contain sensitive system information or credentials
const BLOCKED_ENV_VARS: &[&str] = &[
    // Authentication and credentials
    "SSH_AUTH_SOCK",
    "SSH_AGENT_PID",
    "GPG_AGENT_INFO",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "AZURE_CLIENT_SECRET",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "GITLAB_TOKEN",
    "NPM_TOKEN",
    "DOCKER_PASSWORD",
    "REGISTRY_PASSWORD",
    // System paths that could leak information
    "HOME",
    "USER",
    "LOGNAME",
    "MAIL",
    "SHELL",
    "HISTFILE",
    "HISTSIZE",
    "HISTCONTROL",
    // Process/system info
    "PWD",
    "OLDPWD",
    "SHLVL",
    "TERM",
    "TERM_PROGRAM",
    "COLORTERM",
    "DISPLAY",
    "WINDOWID",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_SESSION_ID",
    "XDG_RUNTIME_DIR",
    // Potentially sensitive configuration
    "DATABASE_URL",
    "REDIS_URL",
    "MONGODB_URI",
    "POSTGRES_PASSWORD",
    "MYSQL_ROOT_PASSWORD",
    "JWT_SECRET",
    "SESSION_SECRET",
    "ENCRYPTION_KEY",
    "MASTER_KEY",
    "PRIVATE_KEY",
    "SECRET_KEY",
];

/// Prefixes for environment variables that should be blocked
const BLOCKED_ENV_PREFIXES: &[&str] = &[
    "AWS_", "AZURE_", "GCP_", "GOOGLE_", "GITHUB_", "GITLAB_", "SSH_", "GPG_",
    "_", // Internal/system variables often start with underscore
];

/// Check if an environment variable should be blocked from import
fn is_blocked_env_var(key: &str) -> bool {
    // Check exact matches
    if BLOCKED_ENV_VARS
        .iter()
        .any(|&blocked| blocked.eq_ignore_ascii_case(key))
    {
        return true;
    }

    // Check prefixes
    if BLOCKED_ENV_PREFIXES
        .iter()
        .any(|&prefix| key.to_uppercase().starts_with(prefix))
    {
        return true;
    }

    // Block variables containing sensitive keywords
    let key_upper = key.to_uppercase();
    let sensitive_keywords = [
        "PASSWORD",
        "SECRET",
        "TOKEN",
        "KEY",
        "CREDENTIAL",
        "AUTH",
        "PRIVATE",
    ];
    if sensitive_keywords.iter().any(|&kw| key_upper.contains(kw)) {
        return true;
    }

    false
}

/// Validates if a character has secrets configured
///
/// # Arguments
/// * `character` - The character to check
///
/// # Returns
/// True if the character has secrets configured
pub fn has_character_secrets(character: &Character) -> bool {
    if let Some(secrets_value) = character.settings.get("secrets") {
        if let Some(secrets_obj) = secrets_value.as_object() {
            return !secrets_obj.is_empty();
        }
    }
    false
}

/// Sets default secrets from environment variables if character doesn't have any
///
/// This function merges SAFE environment variables into the character's settings and secrets.
/// Priority: process.env (defaults) < character.settings/secrets (overrides)
///
/// # Arguments
/// * `character` - The character to update
///
/// # Returns
/// True if secrets were set, false otherwise
///
/// # Security
/// This function filters out sensitive environment variables to prevent credential leakage:
/// - System credentials (AWS, Azure, GCP, SSH keys, etc.)
/// - Database connection strings
/// - User information (HOME, USER, etc.)
/// - Variables containing PASSWORD, SECRET, TOKEN, KEY, etc.
///
/// Use `set_secret()` directly for intentionally loading specific credentials.
pub fn set_default_secrets_from_env(character: &mut Character) -> bool {
    // Collect safe environment variables only
    let env_vars: HashMap<String, String> = std::env::vars()
        .filter(|(key, _)| !is_blocked_env_var(key))
        .collect();

    let filtered_count = std::env::vars().count() - env_vars.len();
    if filtered_count > 0 {
        tracing::debug!(
            "Filtered {} sensitive environment variables from character settings",
            filtered_count
        );
    }

    // Get existing secrets
    let existing_secrets = character
        .settings
        .get("secrets")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // Merge safe environment variables into settings (lower priority)
    for (key, value) in &env_vars {
        if !character.settings.contains_key(key) {
            character
                .settings
                .insert(key.clone(), serde_json::Value::String(value.clone()));
        }
    }

    // Merge safe environment variables into secrets (lower priority)
    let mut secrets_map = serde_json::Map::new();
    for (key, value) in &env_vars {
        secrets_map.insert(key.clone(), serde_json::Value::String(value.clone()));
    }

    // Overlay existing secrets (higher priority)
    for (key, value) in existing_secrets {
        secrets_map.insert(key, value);
    }

    // Update settings.secrets
    character.settings.insert(
        "secrets".to_string(),
        serde_json::Value::Object(secrets_map),
    );

    true
}

/// Explicitly load a specific secret from an environment variable
///
/// Use this for intentionally loading credentials like API keys.
/// Unlike `set_default_secrets_from_env`, this bypasses the blocklist filter.
///
/// # Arguments
/// * `character` - The character to update
/// * `key` - The secret key name to use in character settings
/// * `env_var` - The environment variable name to read from
///
/// # Returns
/// True if the environment variable was found and set
pub fn load_secret_from_env(character: &mut Character, key: &str, env_var: &str) -> bool {
    if let Ok(value) = std::env::var(env_var) {
        set_secret(character, key, &value);
        true
    } else {
        false
    }
}

/// Get a secret from character settings
///
/// # Arguments
/// * `character` - The character to get the secret from
/// * `key` - The secret key to retrieve
///
/// # Returns
/// The secret value if found
pub fn get_secret(character: &Character, key: &str) -> Option<String> {
    // First check secrets
    if let Some(secrets_value) = character.settings.get("secrets") {
        if let Some(secrets_obj) = secrets_value.as_object() {
            if let Some(value) = secrets_obj.get(key) {
                if let Some(s) = value.as_str() {
                    return Some(s.to_string());
                }
            }
        }
    }

    // Fall back to top-level settings
    if let Some(value) = character.settings.get(key) {
        if let Some(s) = value.as_str() {
            return Some(s.to_string());
        }
    }

    None
}

/// Set a secret in character settings
///
/// # Arguments
/// * `character` - The character to update
/// * `key` - The secret key
/// * `value` - The secret value
pub fn set_secret(character: &mut Character, key: &str, value: &str) {
    // Get or create secrets object
    let mut secrets_obj = character
        .settings
        .get("secrets")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // Set the secret
    secrets_obj.insert(
        key.to_string(),
        serde_json::Value::String(value.to_string()),
    );

    // Update settings
    character.settings.insert(
        "secrets".to_string(),
        serde_json::Value::Object(secrets_obj),
    );
}

/// Remove a secret from character settings
///
/// # Arguments
/// * `character` - The character to update
/// * `key` - The secret key to remove
///
/// # Returns
/// True if the secret was removed, false if it didn't exist
pub fn remove_secret(character: &mut Character, key: &str) -> bool {
    if let Some(secrets_value) = character.settings.get_mut("secrets") {
        if let Some(secrets_obj) = secrets_value.as_object_mut() {
            return secrets_obj.remove(key).is_some();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Metadata;
    use serde_json::json;

    fn create_test_character() -> Character {
        Character {
            id: None,
            name: "Test".to_string(),
            username: None,
            bio: vec![],
            lore: vec![],
            knowledge: vec![],
            message_examples: vec![],
            post_examples: vec![],
            topics: vec![],
            style: Default::default(),
            adjectives: vec![],
            settings: Metadata::new(),
            templates: None,
            plugins: vec![],
            clients: vec![],
            model_provider: None,
        }
    }

    #[test]
    fn test_has_character_secrets() {
        let mut character = create_test_character();
        assert!(!has_character_secrets(&character));

        character
            .settings
            .insert("secrets".to_string(), json!({"API_KEY": "test"}));
        assert!(has_character_secrets(&character));
    }

    #[test]
    fn test_get_set_secret() {
        let mut character = create_test_character();

        set_secret(&mut character, "TEST_KEY", "test_value");
        assert_eq!(
            get_secret(&character, "TEST_KEY"),
            Some("test_value".to_string())
        );
    }

    #[test]
    fn test_remove_secret() {
        let mut character = create_test_character();

        set_secret(&mut character, "TEST_KEY", "test_value");
        assert!(get_secret(&character, "TEST_KEY").is_some());

        assert!(remove_secret(&mut character, "TEST_KEY"));
        assert!(get_secret(&character, "TEST_KEY").is_none());

        // Removing non-existent key should return false
        assert!(!remove_secret(&mut character, "NONEXISTENT"));
    }

    #[test]
    fn test_set_default_secrets_from_env() {
        let mut character = create_test_character();

        // Set an environment variable for testing
        std::env::set_var("TEST_ENV_VAR", "test_value");

        assert!(set_default_secrets_from_env(&mut character));

        // Check that env var was added to secrets
        assert!(has_character_secrets(&character));

        // Clean up
        std::env::remove_var("TEST_ENV_VAR");
    }

    #[test]
    fn test_env_var_filtering() {
        // Test that sensitive variables are blocked
        assert!(is_blocked_env_var("AWS_SECRET_ACCESS_KEY"));
        assert!(is_blocked_env_var("SSH_AUTH_SOCK"));
        assert!(is_blocked_env_var("HOME"));
        assert!(is_blocked_env_var("DATABASE_URL"));

        // Test prefix blocking
        assert!(is_blocked_env_var("AWS_REGION"));
        assert!(is_blocked_env_var("AZURE_SUBSCRIPTION_ID"));
        assert!(is_blocked_env_var("GITHUB_SHA"));

        // Test keyword blocking
        assert!(is_blocked_env_var("MY_PASSWORD"));
        assert!(is_blocked_env_var("API_SECRET"));
        assert!(is_blocked_env_var("ACCESS_TOKEN"));
        assert!(is_blocked_env_var("ENCRYPTION_KEY"));

        // Test that safe variables are allowed
        assert!(!is_blocked_env_var("RUST_LOG"));
        assert!(!is_blocked_env_var("CARGO_HOME"));
        assert!(!is_blocked_env_var("PATH")); // PATH is not in blocklist
        assert!(!is_blocked_env_var("LANG"));
    }

    #[test]
    fn test_load_secret_from_env() {
        let mut character = create_test_character();

        // Set env var
        std::env::set_var("MY_SPECIFIC_SECRET", "specific_value");

        // Load it explicitly
        assert!(load_secret_from_env(
            &mut character,
            "api_key",
            "MY_SPECIFIC_SECRET"
        ));
        assert_eq!(
            get_secret(&character, "api_key"),
            Some("specific_value".to_string())
        );

        // Non-existent var should return false
        assert!(!load_secret_from_env(
            &mut character,
            "missing",
            "NONEXISTENT_VAR"
        ));

        // Clean up
        std::env::remove_var("MY_SPECIFIC_SECRET");
    }
}
