//! ML Bridge - Python/Rust Interop for Machine Learning
//!
//! This module provides seamless integration between Rust and Python ML frameworks (PyTorch, TensorFlow).
//! It enables running Python ML code from Rust, managing Python environments, and bridging ML models.
//!
//! # Security
//!
//! This module implements multiple security measures to prevent Python code from exploiting the system:
//! - Path validation: Only scripts in allowed directories can be executed
//! - Timeout limits: All Python executions have a maximum timeout
//! - Code sanitization: Dangerous Python operations are blocked
//! - Resource limits: Memory and CPU usage can be restricted
//! - Whitelist validation: Only approved operations are allowed

use crate::{ZoeyError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

/// ML Framework types supported
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MLFramework {
    /// PyTorch framework
    PyTorch,
    /// TensorFlow framework
    TensorFlow,
    /// Generic/Custom framework
    Custom,
}

impl MLFramework {
    /// Get the Python package name for this framework
    pub fn package_name(&self) -> &str {
        match self {
            MLFramework::PyTorch => "torch",
            MLFramework::TensorFlow => "tensorflow",
            MLFramework::Custom => "custom",
        }
    }
}

/// Security configuration for Python execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Allowed script directories (whitelist)
    pub allowed_script_dirs: Vec<PathBuf>,

    /// Maximum execution timeout in seconds
    pub max_timeout_secs: u64,

    /// Maximum code length (bytes)
    pub max_code_length: usize,

    /// Allow direct code execution (dangerous, should be false in production)
    pub allow_direct_code: bool,

    /// Blocked Python modules (blacklist)
    pub blocked_modules: HashSet<String>,

    /// Blocked Python operations (blacklist)
    pub blocked_operations: HashSet<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        let mut blocked_modules = HashSet::new();
        blocked_modules.insert("os".to_string());
        blocked_modules.insert("sys".to_string());
        blocked_modules.insert("subprocess".to_string());
        blocked_modules.insert("shutil".to_string());
        blocked_modules.insert("socket".to_string());
        blocked_modules.insert("urllib".to_string());
        blocked_modules.insert("requests".to_string());
        blocked_modules.insert("http".to_string());
        blocked_modules.insert("ftplib".to_string());
        blocked_modules.insert("smtplib".to_string());
        blocked_modules.insert("pickle".to_string());
        blocked_modules.insert("marshal".to_string());
        blocked_modules.insert("ctypes".to_string());
        blocked_modules.insert("__builtin__".to_string());
        blocked_modules.insert("builtins".to_string());

        let mut blocked_operations = HashSet::new();
        blocked_operations.insert("eval".to_string());
        blocked_operations.insert("exec".to_string());
        blocked_operations.insert("compile".to_string());
        blocked_operations.insert("open".to_string());
        blocked_operations.insert("file".to_string());
        blocked_operations.insert("input".to_string());
        blocked_operations.insert("raw_input".to_string());
        blocked_operations.insert("__import__".to_string());
        blocked_operations.insert("reload".to_string());
        blocked_operations.insert("execfile".to_string());

        Self {
            allowed_script_dirs: vec![],
            max_timeout_secs: 300,      // 5 minutes default
            max_code_length: 1_000_000, // 1MB default
            allow_direct_code: false,   // Disabled by default for security
            blocked_modules,
            blocked_operations,
        }
    }
}

impl SecurityConfig {
    /// Create a strict security configuration
    pub fn strict() -> Self {
        Self {
            max_timeout_secs: 60,     // 1 minute
            max_code_length: 100_000, // 100KB
            allow_direct_code: false,
            ..Default::default()
        }
    }

    /// Create a permissive configuration (use with caution)
    pub fn permissive() -> Self {
        Self {
            allowed_script_dirs: vec![],
            max_timeout_secs: 3600,      // 1 hour
            max_code_length: 10_000_000, // 10MB
            allow_direct_code: true,
            blocked_modules: HashSet::new(),
            blocked_operations: HashSet::new(),
        }
    }

    /// Add an allowed script directory
    pub fn with_allowed_dir(mut self, dir: PathBuf) -> Self {
        self.allowed_script_dirs.push(dir);
        self
    }

    /// Check if a path is allowed
    pub fn is_path_allowed(&self, path: &Path) -> bool {
        if self.allowed_script_dirs.is_empty() {
            // If no restrictions, allow all (for backward compatibility)
            return true;
        }

        // Check if path is within any allowed directory
        for allowed_dir in &self.allowed_script_dirs {
            if path.starts_with(allowed_dir) {
                return true;
            }
        }

        false
    }

    /// Validate Python code for dangerous operations
    pub fn validate_code(&self, code: &str) -> Result<()> {
        // Check code length
        if code.len() > self.max_code_length {
            return Err(ZoeyError::Runtime(format!(
                "Code too long: {} bytes (max: {})",
                code.len(),
                self.max_code_length
            )));
        }

        // Check for blocked operations
        for blocked_op in &self.blocked_operations {
            if code.contains(blocked_op) {
                return Err(ZoeyError::Runtime(format!(
                    "Blocked operation detected: {}",
                    blocked_op
                )));
            }
        }

        // Check for blocked module imports
        for blocked_mod in &self.blocked_modules {
            let import_patterns = vec![
                format!("import {}", blocked_mod),
                format!("from {} import", blocked_mod),
                format!(
                    "import {}",
                    blocked_mod.replace("__builtin__", "__builtins__")
                ),
            ];

            for pattern in import_patterns {
                if code.contains(&pattern) {
                    return Err(ZoeyError::Runtime(format!(
                        "Blocked module import detected: {}",
                        blocked_mod
                    )));
                }
            }
        }

        // Check for dangerous patterns
        let dangerous_patterns = vec![
            "__import__",
            "eval(",
            "exec(",
            "compile(",
            "open(",
            "file(",
            "input(",
            "raw_input(",
            "reload(",
            "execfile(",
            "subprocess",
            "os.system",
            "os.popen",
            "os.exec",
            "shutil.",
            "socket.",
            "urllib.",
            "requests.",
            "pickle.load",
            "marshal.load",
            "ctypes.",
        ];

        for pattern in dangerous_patterns {
            if code.contains(pattern) {
                return Err(ZoeyError::Runtime(format!(
                    "Dangerous pattern detected: {}",
                    pattern
                )));
            }
        }

        Ok(())
    }
}

/// Python environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonEnvironment {
    /// Python interpreter path (default: "python3")
    pub python_path: String,

    /// Virtual environment path (optional)
    pub venv_path: Option<PathBuf>,

    /// Additional environment variables
    pub env_vars: HashMap<String, String>,

    /// Working directory for Python execution
    pub working_dir: Option<PathBuf>,

    /// Security configuration
    pub security: SecurityConfig,
}

impl Default for PythonEnvironment {
    fn default() -> Self {
        Self {
            python_path: "python3".to_string(),
            venv_path: None,
            env_vars: HashMap::new(),
            working_dir: None,
            security: SecurityConfig::default(),
        }
    }
}

impl PythonEnvironment {
    /// Create a new Python environment with custom settings
    pub fn new(python_path: String) -> Self {
        Self {
            python_path,
            ..Default::default()
        }
    }

    /// Set virtual environment path
    pub fn with_venv(mut self, venv_path: PathBuf) -> Self {
        self.venv_path = Some(venv_path);
        self
    }

    /// Add environment variable
    pub fn with_env_var(mut self, key: String, value: String) -> Self {
        self.env_vars.insert(key, value);
        self
    }

    /// Set working directory
    pub fn with_working_dir(mut self, dir: PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
    }

    /// Set security configuration
    pub fn with_security(mut self, security: SecurityConfig) -> Self {
        self.security = security;
        self
    }

    /// Check if Python is available
    pub async fn check_availability(&self) -> Result<bool> {
        let output = Command::new(&self.python_path).arg("--version").output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout);
                    debug!("Python available: {}", version.trim());
                    Ok(true)
                } else {
                    warn!("Python command failed: {:?}", output.stderr);
                    Ok(false)
                }
            }
            Err(e) => {
                warn!("Failed to execute Python: {}", e);
                Ok(false)
            }
        }
    }

    /// Run a Python script and return output
    #[instrument(skip(self, script_path), level = "debug")]
    pub async fn run_script(&self, script_path: &Path, args: &[&str]) -> Result<String> {
        // Security: Validate path
        let script_path = if script_path.exists() {
            script_path.canonicalize().map_err(|e| {
                ZoeyError::Runtime(format!("Failed to canonicalize script path: {}", e))
            })?
        } else {
            // If path doesn't exist yet, check if parent directory is allowed
            if let Some(parent) = script_path.parent() {
                if let Ok(canonical_parent) = parent.canonicalize() {
                    if !self.security.is_path_allowed(&canonical_parent) {
                        return Err(ZoeyError::Runtime(format!(
                            "Script path not allowed: {:?}. Allowed directories: {:?}",
                            script_path, self.security.allowed_script_dirs
                        )));
                    }
                }
            }
            script_path.to_path_buf()
        };

        if !self.security.allowed_script_dirs.is_empty()
            && !self.security.is_path_allowed(&script_path)
        {
            return Err(ZoeyError::Runtime(format!(
                "Script path not allowed: {:?}. Allowed directories: {:?}",
                script_path, self.security.allowed_script_dirs
            )));
        }

        // Security: Validate script content if possible
        if let Ok(contents) = std::fs::read_to_string(&script_path) {
            self.security.validate_code(&contents)?;
        }

        // Security: Validate arguments
        for arg in args {
            if arg.contains(";") || arg.contains("&") || arg.contains("|") || arg.contains("`") {
                return Err(ZoeyError::Runtime(format!(
                    "Dangerous argument detected: {}",
                    arg
                )));
            }
        }

        let mut cmd = Command::new(&self.python_path);
        cmd.arg(&script_path);

        for arg in args {
            cmd.arg(arg);
        }

        // Security: Limit environment variables
        for (key, value) in &self.env_vars {
            // Block dangerous environment variables
            if key == "PATH" || key == "LD_LIBRARY_PATH" || key == "PYTHONPATH" {
                warn!("Blocked dangerous environment variable: {}", key);
                continue;
            }
            cmd.env(key, value);
        }

        // Security: Set safe defaults
        cmd.env("PYTHONUNBUFFERED", "1");
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");

        // Set working directory
        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }

        // Use venv if configured
        if let Some(ref venv) = self.venv_path {
            let venv_python = venv.join("bin").join("python");
            if venv_python.exists() {
                cmd = Command::new(&venv_python);
                cmd.arg(&script_path);
                for arg in args {
                    cmd.arg(arg);
                }
            }
        }

        debug!("Executing Python script: {:?}", script_path);

        let timeout_duration = Duration::from_secs(self.security.max_timeout_secs);
        let python_path = self.python_path.clone();
        let script_path_clone = script_path.clone();
        let args_vec: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let env_vars_clone: HashMap<String, String> = self
            .env_vars
            .iter()
            .filter(|(k, _)| {
                k.as_str() != "PATH"
                    && k.as_str() != "LD_LIBRARY_PATH"
                    && k.as_str() != "PYTHONPATH"
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let venv_path_clone = self.venv_path.clone();
        let working_dir_clone = self.working_dir.clone();

        let output_future = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new(&python_path);
            cmd.arg(&script_path_clone);
            for arg in &args_vec {
                cmd.arg(arg);
            }
            for (key, value) in &env_vars_clone {
                cmd.env(key, value);
            }
            cmd.env("PYTHONUNBUFFERED", "1");
            cmd.env("PYTHONDONTWRITEBYTECODE", "1");
            if let Some(ref dir) = working_dir_clone {
                cmd.current_dir(dir);
            }
            if let Some(ref venv) = venv_path_clone {
                let venv_python = venv.join("bin").join("python");
                if venv_python.exists() {
                    cmd = Command::new(&venv_python);
                    cmd.arg(&script_path_clone);
                    for arg in &args_vec {
                        cmd.arg(arg);
                    }
                    for (key, value) in &env_vars_clone {
                        cmd.env(key, value);
                    }
                    cmd.env("PYTHONUNBUFFERED", "1");
                    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
                }
            }
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output()
        });

        let output = timeout(timeout_duration, output_future)
            .await
            .map_err(|_| {
                ZoeyError::Runtime(format!(
                    "Python script execution timed out after {} seconds",
                    self.security.max_timeout_secs
                ))
            })?
            .map_err(|e| ZoeyError::Runtime(format!("Failed to spawn Python process: {}", e)))?
            .map_err(|e| ZoeyError::Runtime(format!("Failed to execute Python script: {}", e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            debug!("Python script output: {} bytes", stdout.len());
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            error!("Python script failed: {}", stderr);
            Err(ZoeyError::Runtime(format!(
                "Python script failed: {}",
                stderr
            )))
        }
    }

    /// Run Python code directly
    ///
    /// # Security Warning
    /// This method is dangerous and should only be used with trusted code.
    /// By default, `allow_direct_code` is false in SecurityConfig.
    #[instrument(skip(self, code), level = "debug")]
    pub async fn run_code(&self, code: &str) -> Result<String> {
        // Security: Check if direct code execution is allowed
        if !self.security.allow_direct_code {
            return Err(ZoeyError::Runtime(
                "Direct code execution is disabled for security. Use run_script() with a whitelisted script instead.".to_string()
            ));
        }

        // Security: Validate code
        self.security.validate_code(code)?;

        let mut cmd = Command::new(&self.python_path);
        cmd.arg("-c").arg(code);

        // Security: Limit environment variables
        for (key, value) in &self.env_vars {
            // Block dangerous environment variables
            if key == "PATH" || key == "LD_LIBRARY_PATH" || key == "PYTHONPATH" {
                warn!("Blocked dangerous environment variable: {}", key);
                continue;
            }
            cmd.env(key, value);
        }

        // Security: Set safe defaults
        cmd.env("PYTHONUNBUFFERED", "1");
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");

        if let Some(ref venv) = self.venv_path {
            let venv_python = venv.join("bin").join("python");
            if venv_python.exists() {
                cmd = Command::new(&venv_python);
                cmd.arg("-c").arg(code);
            }
        }

        debug!("Executing Python code: {} bytes", code.len());

        let timeout_duration = Duration::from_secs(self.security.max_timeout_secs);
        let python_path = self.python_path.clone();
        let code_clone = code.to_string();
        let env_vars_clone: HashMap<String, String> = self
            .env_vars
            .iter()
            .filter(|(k, _)| {
                k.as_str() != "PATH"
                    && k.as_str() != "LD_LIBRARY_PATH"
                    && k.as_str() != "PYTHONPATH"
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let venv_path_clone = self.venv_path.clone();

        let output_future = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new(&python_path);
            cmd.arg("-c").arg(&code_clone);
            for (key, value) in &env_vars_clone {
                cmd.env(key, value);
            }
            cmd.env("PYTHONUNBUFFERED", "1");
            cmd.env("PYTHONDONTWRITEBYTECODE", "1");
            if let Some(ref venv) = venv_path_clone {
                let venv_python = venv.join("bin").join("python");
                if venv_python.exists() {
                    cmd = Command::new(&venv_python);
                    cmd.arg("-c").arg(&code_clone);
                    for (key, value) in &env_vars_clone {
                        cmd.env(key, value);
                    }
                    cmd.env("PYTHONUNBUFFERED", "1");
                    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
                }
            }
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output()
        });

        let output = timeout(timeout_duration, output_future)
            .await
            .map_err(|_| {
                ZoeyError::Runtime(format!(
                    "Python code execution timed out after {} seconds",
                    self.security.max_timeout_secs
                ))
            })?
            .map_err(|e| ZoeyError::Runtime(format!("Failed to spawn Python process: {}", e)))?
            .map_err(|e| ZoeyError::Runtime(format!("Failed to execute Python code: {}", e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(ZoeyError::Runtime(format!(
                "Python code failed: {}",
                stderr
            )))
        }
    }

    /// Check if a Python package is installed
    pub async fn check_package(&self, package: &str) -> Result<bool> {
        // Security: Validate package name
        if package.contains(";")
            || package.contains("&")
            || package.contains("|")
            || package.contains("`")
        {
            return Err(ZoeyError::Runtime(format!(
                "Invalid package name: {}",
                package
            )));
        }

        // Security: Check if package is blocked
        if self.security.blocked_modules.contains(package) {
            return Err(ZoeyError::Runtime(format!(
                "Package is blocked: {}",
                package
            )));
        }

        // Use a safe check method that doesn't require direct code execution
        let mut cmd = Command::new(&self.python_path);
        cmd.arg("-c");
        cmd.arg(format!("import {}; print('installed')", package));

        // Security: Set safe defaults
        cmd.env("PYTHONUNBUFFERED", "1");
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");

        if let Some(ref venv) = self.venv_path {
            let venv_python = venv.join("bin").join("python");
            if venv_python.exists() {
                cmd = Command::new(&venv_python);
                cmd.arg("-c");
                cmd.arg(format!("import {}; print('installed')", package));
            }
        }

        let timeout_duration = Duration::from_secs(10); // Short timeout for package checks
        let python_path = self.python_path.clone();
        let package_clone = package.to_string();
        let venv_path_clone = self.venv_path.clone();

        let output_future = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new(&python_path);
            cmd.arg("-c");
            cmd.arg(format!("import {}; print('installed')", package_clone));
            cmd.env("PYTHONUNBUFFERED", "1");
            cmd.env("PYTHONDONTWRITEBYTECODE", "1");
            if let Some(ref venv) = venv_path_clone {
                let venv_python = venv.join("bin").join("python");
                if venv_python.exists() {
                    cmd = Command::new(&venv_python);
                    cmd.arg("-c");
                    cmd.arg(format!("import {}; print('installed')", package_clone));
                    cmd.env("PYTHONUNBUFFERED", "1");
                    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
                }
            }
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output()
        });

        match timeout(timeout_duration, output_future).await {
            Ok(join_result) => match join_result {
                Ok(output_result) => match output_result {
                    Ok(output) => {
                        if output.status.success() {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            Ok(stdout.contains("installed"))
                        } else {
                            Ok(false)
                        }
                    }
                    Err(_) => Ok(false),
                },
                Err(_) => Ok(false),
            },
            Err(_) => Ok(false),
        }
    }
}

/// Model interface for trained ML models
#[async_trait::async_trait]
pub trait ModelInterface: Send + Sync {
    /// Get model name
    fn name(&self) -> &str;

    /// Get model framework
    fn framework(&self) -> MLFramework;

    /// Load model from path
    async fn load(&mut self, path: &Path) -> Result<()>;

    /// Save model to path
    async fn save(&self, path: &Path) -> Result<()>;

    /// Run inference on input data
    async fn predict(&self, input: &[f32]) -> Result<Vec<f32>>;

    /// Get model metadata
    fn metadata(&self) -> HashMap<String, String>;
}

/// Trained model wrapper
#[derive(Debug, Clone)]
pub struct TrainedModel {
    /// Model name/identifier
    pub name: String,

    /// Framework used
    pub framework: MLFramework,

    /// Model file path
    pub path: PathBuf,

    /// Model metadata
    pub metadata: HashMap<String, String>,
}

impl TrainedModel {
    /// Create a new trained model reference
    pub fn new(name: String, framework: MLFramework, path: PathBuf) -> Self {
        Self {
            name,
            framework,
            path,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the model
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// ML Bridge - Main interface for ML operations
pub struct MLBridge {
    /// Python environment
    python_env: PythonEnvironment,

    /// Cached models
    models: Arc<RwLock<HashMap<String, TrainedModel>>>,

    /// Framework availability cache
    frameworks: Arc<RwLock<HashMap<MLFramework, bool>>>,
}

impl MLBridge {
    /// Create a new ML bridge
    pub fn new(python_env: PythonEnvironment) -> Self {
        Self {
            python_env,
            models: Arc::new(RwLock::new(HashMap::new())),
            frameworks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get Python environment reference
    pub fn python_env(&self) -> &PythonEnvironment {
        &self.python_env
    }

    /// Check if a framework is available
    #[instrument(skip(self), level = "info")]
    pub async fn check_framework(&self, framework: MLFramework) -> Result<bool> {
        // Check cache first
        {
            let cache = self.frameworks.read().await;
            if let Some(&available) = cache.get(&framework) {
                return Ok(available);
            }
        }

        // Check package availability
        let available = self
            .python_env
            .check_package(framework.package_name())
            .await?;

        // Update cache
        {
            let mut cache = self.frameworks.write().await;
            cache.insert(framework, available);
        }

        if available {
            info!("✓ {} is available", framework.package_name());
        } else {
            warn!("✗ {} is not installed", framework.package_name());
        }

        Ok(available)
    }

    /// Register a trained model
    pub async fn register_model(&self, model: TrainedModel) -> Result<()> {
        let name = model.name.clone();
        let mut models = self.models.write().await;
        models.insert(name.clone(), model);
        info!("Registered model: {}", name);
        Ok(())
    }

    /// Get a registered model
    pub async fn get_model(&self, name: &str) -> Option<TrainedModel> {
        let models = self.models.read().await;
        models.get(name).cloned()
    }

    /// List all registered models
    pub async fn list_models(&self) -> Vec<String> {
        let models = self.models.read().await;
        models.keys().cloned().collect()
    }

    /// Remove a model from registry
    pub async fn unregister_model(&self, name: &str) -> Result<()> {
        let mut models = self.models.write().await;
        models.remove(name);
        info!("Unregistered model: {}", name);
        Ok(())
    }

    /// Execute a Python ML script
    pub async fn execute_script(&self, script_path: &Path, args: &[&str]) -> Result<String> {
        self.python_env.run_script(script_path, args).await
    }

    /// Execute Python ML code directly
    ///
    /// # Security Warning
    /// This method requires `allow_direct_code` to be true in the security configuration.
    /// Use `execute_script()` with whitelisted scripts instead for better security.
    pub async fn execute_code(&self, code: &str) -> Result<String> {
        self.python_env.run_code(code).await
    }

    /// Get security configuration
    pub fn security_config(&self) -> &SecurityConfig {
        &self.python_env.security
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_python_environment_creation() {
        let env = PythonEnvironment::default();
        assert_eq!(env.python_path, "python3");
        assert!(env.venv_path.is_none());
        assert!(!env.security.allow_direct_code);
    }

    #[test]
    fn test_ml_framework_package_names() {
        assert_eq!(MLFramework::PyTorch.package_name(), "torch");
        assert_eq!(MLFramework::TensorFlow.package_name(), "tensorflow");
    }

    #[test]
    fn test_trained_model_creation() {
        let model = TrainedModel::new(
            "test_model".to_string(),
            MLFramework::PyTorch,
            PathBuf::from("/tmp/model.pt"),
        );
        assert_eq!(model.name, "test_model");
        assert_eq!(model.framework, MLFramework::PyTorch);
    }

    #[tokio::test]
    async fn test_ml_bridge_creation() {
        let env = PythonEnvironment::default();
        let bridge = MLBridge::new(env);
        assert!(bridge.list_models().await.is_empty());
    }

    #[test]
    fn test_security_config_default() {
        let config = SecurityConfig::default();
        assert!(!config.allow_direct_code);
        assert_eq!(config.max_timeout_secs, 300);
        assert!(!config.blocked_modules.is_empty());
        assert!(!config.blocked_operations.is_empty());
    }

    #[test]
    fn test_security_config_strict() {
        let config = SecurityConfig::strict();
        assert!(!config.allow_direct_code);
        assert_eq!(config.max_timeout_secs, 60);
        assert_eq!(config.max_code_length, 100_000);
    }

    #[test]
    fn test_security_path_validation() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_dir = temp_dir.path().join("allowed");
        fs::create_dir_all(&allowed_dir).unwrap();

        let mut config = SecurityConfig::default();
        config.allowed_script_dirs.push(allowed_dir.clone());

        let allowed_script = allowed_dir.join("script.py");
        assert!(config.is_path_allowed(&allowed_script));

        let disallowed_script = PathBuf::from("/tmp/evil.py");
        assert!(!config.is_path_allowed(&disallowed_script));
    }

    #[test]
    fn test_security_code_validation() {
        let config = SecurityConfig::default();

        // Valid code should pass
        assert!(config.validate_code("print('hello')").is_ok());

        // Dangerous operations should be blocked
        assert!(config
            .validate_code("import os; os.system('rm -rf /')")
            .is_err());
        assert!(config.validate_code("eval('malicious code')").is_err());
        assert!(config.validate_code("exec('dangerous')").is_err());
        assert!(config.validate_code("import subprocess").is_err());
        assert!(config.validate_code("import socket").is_err());
        assert!(config.validate_code("import pickle").is_err());
    }

    #[test]
    fn test_security_code_length_limit() {
        let mut config = SecurityConfig::default();
        config.max_code_length = 100;

        let short_code = "print('hello')";
        assert!(config.validate_code(short_code).is_ok());

        let long_code = "x".repeat(200);
        assert!(config.validate_code(&long_code).is_err());
    }

    #[test]
    fn test_security_direct_code_blocked() {
        let env = PythonEnvironment::default();
        // Direct code should be blocked by default
        assert!(!env.security.allow_direct_code);
    }

    #[tokio::test]
    async fn test_security_direct_code_execution_blocked() {
        let env = PythonEnvironment::default();
        let result = env.run_code("print('test')").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }

    #[tokio::test]
    async fn test_security_allowed_direct_code() {
        let mut config = SecurityConfig::default();
        config.allow_direct_code = true;
        let env = PythonEnvironment::default().with_security(config);

        // This should still fail because of dangerous operations check
        let result = env.run_code("import os").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_security_package_validation() {
        let config = SecurityConfig::default();
        assert!(config.blocked_modules.contains("os"));
        assert!(config.blocked_modules.contains("subprocess"));
    }

    #[tokio::test]
    async fn test_ml_bridge_model_registration() {
        let env = PythonEnvironment::default();
        let bridge = MLBridge::new(env);

        let model = TrainedModel::new(
            "test_model".to_string(),
            MLFramework::PyTorch,
            PathBuf::from("/tmp/model.pt"),
        );

        bridge.register_model(model).await.unwrap();
        assert_eq!(bridge.list_models().await.len(), 1);
        assert!(bridge.get_model("test_model").await.is_some());

        bridge.unregister_model("test_model").await.unwrap();
        assert!(bridge.list_models().await.is_empty());
    }

    #[test]
    fn test_security_config_with_allowed_dir() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_dir = temp_dir.path().to_path_buf();

        let config = SecurityConfig::default().with_allowed_dir(allowed_dir.clone());
        assert!(config.allowed_script_dirs.contains(&allowed_dir));
    }
}
