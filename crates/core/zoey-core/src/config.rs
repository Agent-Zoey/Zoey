//! Configuration management and environment variable loading

use crate::{ZoeyError, Result};
use std::env;
use std::path::Path;

/// Load environment variables from .env file
///
/// This function loads variables from a .env file in the current directory
/// or a parent directory. It's safe to call multiple times (only loads once).
///
/// # Example
///
/// ```no_run
/// use zoey_core::load_env;
///
/// // Load .env file
/// load_env().ok();
///
/// // Now you can use environment variables
/// let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
/// ```
pub fn load_env() -> Result<()> {
    match dotenvy::dotenv() {
        Ok(path) => {
            tracing::info!("✓ Loaded environment from: {}", path.display());
            Ok(())
        }
        Err(dotenvy::Error::LineParse(line, pos)) => Err(ZoeyError::config(format!(
            "Failed to parse .env file at line {}, position {}",
            line, pos
        ))),
        Err(dotenvy::Error::Io(_)) => {
            tracing::warn!("No .env file found - using system environment variables only");
            Ok(())
        }
        Err(e) => Err(ZoeyError::config(format!(
            "Failed to load .env file: {}",
            e
        ))),
    }
}

/// Load environment variables from a specific file
///
/// # Example
///
/// ```no_run
/// use zoey_core::load_env_from_path;
///
/// load_env_from_path(".env.production").ok();
/// ```
pub fn load_env_from_path<P: AsRef<Path>>(path: P) -> Result<()> {
    match dotenvy::from_path(path.as_ref()) {
        Ok(_) => {
            tracing::info!("✓ Loaded environment from: {}", path.as_ref().display());
            Ok(())
        }
        Err(e) => Err(ZoeyError::config(format!(
            "Failed to load {} environment file: {}",
            path.as_ref().display(),
            e
        ))),
    }
}

/// Get required environment variable
///
/// Returns an error if the variable is not set
pub fn get_required_env(key: &str) -> Result<String> {
    env::var(key).map_err(|_| {
        ZoeyError::config(format!(
            "Required environment variable '{}' is not set. \
             Check your .env file or system environment.",
            key
        ))
    })
}

/// Get optional environment variable with default
pub fn get_env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Get environment variable as boolean
pub fn get_env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .and_then(|v| match v.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

/// Get environment variable as integer
pub fn get_env_int<T>(key: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<T>().ok())
        .unwrap_or(default)
}

/// Get environment variable as float
pub fn get_env_float(key: &str, default: f32) -> f32 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(default)
}

/// Validate that required environment variables are set
pub fn validate_env(required_vars: &[&str]) -> Result<()> {
    let mut missing = Vec::new();

    for var in required_vars {
        if env::var(var).is_err() {
            missing.push(*var);
        }
    }

    if !missing.is_empty() {
        return Err(ZoeyError::config(format!(
            "Missing required environment variables: {}\n\
             Run 'cargo run --bin generate-config' to create a .env file",
            missing.join(", ")
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_env_bool() {
        env::set_var("TEST_BOOL_TRUE", "true");
        env::set_var("TEST_BOOL_FALSE", "false");
        env::set_var("TEST_BOOL_1", "1");
        env::set_var("TEST_BOOL_0", "0");

        assert_eq!(get_env_bool("TEST_BOOL_TRUE", false), true);
        assert_eq!(get_env_bool("TEST_BOOL_FALSE", true), false);
        assert_eq!(get_env_bool("TEST_BOOL_1", false), true);
        assert_eq!(get_env_bool("TEST_BOOL_0", true), false);
        assert_eq!(get_env_bool("NONEXISTENT", true), true);
        assert_eq!(get_env_bool("NONEXISTENT", false), false);

        env::remove_var("TEST_BOOL_TRUE");
        env::remove_var("TEST_BOOL_FALSE");
        env::remove_var("TEST_BOOL_1");
        env::remove_var("TEST_BOOL_0");
    }

    #[test]
    fn test_get_env_int() {
        env::set_var("TEST_INT", "42");
        assert_eq!(get_env_int("TEST_INT", 0), 42);
        assert_eq!(get_env_int("NONEXISTENT", 99), 99);
        env::remove_var("TEST_INT");
    }

    #[test]
    fn test_get_env_float() {
        env::set_var("TEST_FLOAT", "0.7");
        assert_eq!(get_env_float("TEST_FLOAT", 0.0), 0.7);
        assert_eq!(get_env_float("NONEXISTENT", 1.5), 1.5);
        env::remove_var("TEST_FLOAT");
    }

    #[test]
    fn test_get_env_or() {
        env::set_var("TEST_STRING", "hello");
        assert_eq!(get_env_or("TEST_STRING", "default"), "hello");
        assert_eq!(get_env_or("NONEXISTENT", "default"), "default");
        env::remove_var("TEST_STRING");
    }
}
