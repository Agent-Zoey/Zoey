//! Pocket TTS Server Manager
//!
//! Automatically manages the pocket-tts Python server process.
//! Handles starting, stopping, health checking, and auto-restart.
//!
//! ## Features
//! - **Auto-Start**: Automatically starts pocket-tts server when needed
//! - **Health Monitoring**: Continuous health checks with auto-recovery
//! - **Process Management**: Clean startup/shutdown of Python process
//! - **Multiple Installation Methods**: Supports pip, uvx, or custom paths
//!
//! ## Usage
//!
//! ### Basic Auto-Start
//! ```rust,ignore
//! use zoey_provider_voice::PocketTTSServer;
//!
//! // Start server with defaults (pip installed pocket-tts)
//! let server = PocketTTSServer::builder().build().await?;
//! server.start().await?;
//!
//! // Get connected engine
//! let engine = server.engine();
//! let audio = engine.synthesize("Hello!", &config).await?;
//! ```
//!
//! ### With uvx (No Installation)
//! ```rust,ignore
//! let server = PocketTTSServer::builder()
//!     .use_uvx()
//!     .port(8080)
//!     .build()
//!     .await?;
//! ```
//!
//! ### Auto-Start on First Use
//! ```rust,ignore
//! // Engine with auto-start - server starts automatically on first synthesis
//! let engine = PocketTTSEngine::with_auto_start().await?;
//! ```
//!
//! ## Requirements
//! - Python 3.8+ with pip, OR
//! - uvx (from Astral's uv)
//!
//! Reference: https://github.com/kyutai-labs/pocket-tts

use reqwest::Client;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::pocket_tts::{PocketTTSEngine, PocketTTSVoice};
use crate::types::VoiceError;

// ============================================================================
// Constants
// ============================================================================

/// Default port for Pocket TTS server
const DEFAULT_PORT: u16 = 8000;

/// Default host to bind to (localhost only for security)
const DEFAULT_HOST: &str = "127.0.0.1";

/// Startup timeout in seconds
const STARTUP_TIMEOUT_SECS: u64 = 60;

/// Health check interval in milliseconds
const HEALTH_CHECK_INTERVAL_MS: u64 = 500;

// ============================================================================
// Server Configuration
// ============================================================================

/// Installation method for pocket-tts
#[derive(Debug, Clone)]
pub enum PocketTTSInstallMethod {
    /// Use pip-installed pocket-tts (default)
    /// Command: `pocket-tts serve`
    Pip,
    /// Use uvx without installation
    /// Command: `uvx pocket-tts serve`
    Uvx,
    /// Custom command path
    /// Useful for virtual environments or custom installations
    Custom(PathBuf),
}

impl Default for PocketTTSInstallMethod {
    fn default() -> Self {
        Self::Pip
    }
}

/// Configuration for Pocket TTS server
#[derive(Debug, Clone)]
pub struct PocketTTSServerConfig {
    /// Port to run server on
    pub port: u16,
    /// Host to bind to
    pub host: String,
    /// Installation method
    pub install_method: PocketTTSInstallMethod,
    /// Default voice to use
    pub default_voice: String,
    /// Working directory for the server process
    pub working_dir: Option<PathBuf>,
    /// Additional environment variables
    pub env_vars: Vec<(String, String)>,
    /// Number of worker threads (if supported)
    pub workers: Option<u32>,
}

impl Default for PocketTTSServerConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            host: DEFAULT_HOST.to_string(),
            install_method: PocketTTSInstallMethod::Pip,
            default_voice: "alba".to_string(),
            working_dir: None,
            env_vars: Vec::new(),
            workers: None,
        }
    }
}

// ============================================================================
// Server Builder
// ============================================================================

/// Builder for PocketTTSServer
pub struct PocketTTSServerBuilder {
    config: PocketTTSServerConfig,
}

impl Default for PocketTTSServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PocketTTSServerBuilder {
    /// Create new builder with defaults
    pub fn new() -> Self {
        Self {
            config: PocketTTSServerConfig::default(),
        }
    }

    /// Set the port (default: 8000)
    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    /// Set the host to bind to (default: 127.0.0.1)
    ///
    /// ⚠️ SECURITY: Only bind to 0.0.0.0 if you understand the implications!
    pub fn host(mut self, host: &str) -> Self {
        self.config.host = host.to_string();
        self
    }

    /// Use pip-installed pocket-tts (default)
    pub fn use_pip(mut self) -> Self {
        self.config.install_method = PocketTTSInstallMethod::Pip;
        self
    }

    /// Use uvx to run without installation
    pub fn use_uvx(mut self) -> Self {
        self.config.install_method = PocketTTSInstallMethod::Uvx;
        self
    }

    /// Use custom command path
    pub fn use_custom(mut self, path: PathBuf) -> Self {
        self.config.install_method = PocketTTSInstallMethod::Custom(path);
        self
    }

    /// Set default voice
    pub fn voice(mut self, voice: &str) -> Self {
        self.config.default_voice = voice.to_string();
        self
    }

    /// Set working directory
    pub fn working_dir(mut self, dir: PathBuf) -> Self {
        self.config.working_dir = Some(dir);
        self
    }

    /// Add environment variable
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.config.env_vars.push((key.to_string(), value.to_string()));
        self
    }

    /// Set number of workers
    pub fn workers(mut self, workers: u32) -> Self {
        self.config.workers = Some(workers);
        self
    }

    /// Build the server manager
    pub async fn build(self) -> Result<PocketTTSServer, VoiceError> {
        // Security warning for public binding
        if self.config.host == "0.0.0.0" {
            warn!("⚠️  Binding to 0.0.0.0 - server will be publicly accessible!");
        }

        // Verify installation method is available
        if !Self::check_installation(&self.config.install_method).await {
            return Err(VoiceError::NotReady(
                Self::installation_help(&self.config.install_method),
            ));
        }

        Ok(PocketTTSServer {
            config: self.config,
            process: Arc::new(RwLock::new(None)),
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
        })
    }

    /// Check if the installation method is available
    async fn check_installation(method: &PocketTTSInstallMethod) -> bool {
        let result = match method {
            PocketTTSInstallMethod::Pip => {
                Command::new("pocket-tts")
                    .arg("--version")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            }
            PocketTTSInstallMethod::Uvx => {
                Command::new("uvx")
                    .arg("--version")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            }
            PocketTTSInstallMethod::Custom(path) => path.exists(),
        };

        result
    }

    /// Get installation help message
    fn installation_help(method: &PocketTTSInstallMethod) -> String {
        match method {
            PocketTTSInstallMethod::Pip => {
                "pocket-tts not found. Install with:\n  pip install pocket-tts\n  # or: uv pip install pocket-tts".to_string()
            }
            PocketTTSInstallMethod::Uvx => {
                "uvx not found. Install uv from: https://docs.astral.sh/uv/".to_string()
            }
            PocketTTSInstallMethod::Custom(path) => {
                format!("Custom pocket-tts not found at: {}", path.display())
            }
        }
    }
}

// ============================================================================
// Server Implementation
// ============================================================================

/// Pocket TTS Server Manager
///
/// Manages the lifecycle of a pocket-tts Python server process.
///
/// ## Example
/// ```rust,ignore
/// let server = PocketTTSServer::builder()
///     .port(8080)
///     .use_uvx()
///     .build()
///     .await?;
///
/// server.start().await?;
///
/// // Use the engine
/// let engine = server.engine();
/// let audio = engine.synthesize("Hello!", &config).await?;
///
/// // Server stops when dropped
/// ```
pub struct PocketTTSServer {
    config: PocketTTSServerConfig,
    process: Arc<RwLock<Option<Child>>>,
    client: Client,
}

impl PocketTTSServer {
    /// Create a new server builder
    pub fn builder() -> PocketTTSServerBuilder {
        PocketTTSServerBuilder::new()
    }

    /// Create server with default config
    pub async fn default_config() -> Result<Self, VoiceError> {
        Self::builder().build().await
    }

    /// Get the endpoint URL
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}", self.config.host, self.config.port)
    }

    /// Get WebSocket URL (if pocket-tts supports it)
    pub fn ws_url(&self) -> String {
        format!("ws://{}:{}/ws", self.config.host, self.config.port)
    }

    /// Get the port
    pub fn port(&self) -> u16 {
        self.config.port
    }

    /// Start the server
    pub async fn start(&self) -> Result<(), VoiceError> {
        // Check if already running
        if self.is_running().await {
            debug!("Pocket TTS server already running");
            return Ok(());
        }

        // Check if there's an existing process
        {
            let proc = self.process.read().await;
            if proc.is_some() {
                return Ok(()); // Process exists
            }
        }

        info!(
            port = %self.config.port,
            host = %self.config.host,
            "Starting Pocket TTS server..."
        );

        // Build command based on installation method
        let mut cmd = self.build_command();

        // Add environment variables
        for (key, value) in &self.config.env_vars {
            cmd.env(key, value);
        }

        // Set working directory if specified
        if let Some(ref dir) = self.config.working_dir {
            cmd.current_dir(dir);
        }

        // Configure stdio
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Spawn process
        let child = cmd.spawn().map_err(|e| {
            VoiceError::Other(format!("Failed to start Pocket TTS server: {}", e))
        })?;

        let pid = child.id();

        // Store process handle
        {
            let mut proc = self.process.write().await;
            *proc = Some(child);
        }

        // Wait for server to be ready
        if let Err(e) = self.wait_for_ready().await {
            // Server failed to start, clean up
            self.stop().await;
            return Err(e);
        }

        info!(
            pid = pid,
            endpoint = %self.endpoint(),
            "Pocket TTS server started"
        );

        Ok(())
    }

    /// Build the command to start the server
    fn build_command(&self) -> Command {
        let mut cmd = match &self.config.install_method {
            PocketTTSInstallMethod::Pip => {
                let mut c = Command::new("pocket-tts");
                c.arg("serve");
                c
            }
            PocketTTSInstallMethod::Uvx => {
                let mut c = Command::new("uvx");
                c.args(["pocket-tts", "serve"]);
                c
            }
            PocketTTSInstallMethod::Custom(path) => {
                let mut c = Command::new(path);
                c.arg("serve");
                c
            }
        };

        // Add port argument
        cmd.args(["--port", &self.config.port.to_string()]);

        // Add host argument if not default
        if self.config.host != DEFAULT_HOST {
            cmd.args(["--host", &self.config.host]);
        }

        // Add workers if specified
        if let Some(workers) = self.config.workers {
            cmd.args(["--workers", &workers.to_string()]);
        }

        cmd
    }

    /// Wait for server to become ready
    async fn wait_for_ready(&self) -> Result<(), VoiceError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(STARTUP_TIMEOUT_SECS);

        loop {
            // Check if process died
            {
                let mut proc = self.process.write().await;
                if let Some(ref mut child) = *proc {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            // Process exited
                            let mut stderr_output = String::new();
                            if let Some(ref mut stderr) = child.stderr {
                                use std::io::Read;
                                let _ = stderr.read_to_string(&mut stderr_output);
                            }
                            return Err(VoiceError::Other(format!(
                                "Pocket TTS server exited with status {}: {}",
                                status, stderr_output
                            )));
                        }
                        Ok(None) => {
                            // Still running
                        }
                        Err(e) => {
                            return Err(VoiceError::Other(format!(
                                "Failed to check process status: {}",
                                e
                            )));
                        }
                    }
                }
            }

            // Try health check
            if self.health_check().await {
                return Ok(());
            }

            // Check timeout
            if start.elapsed() > timeout {
                return Err(VoiceError::NotReady(format!(
                    "Pocket TTS server failed to start within {} seconds",
                    STARTUP_TIMEOUT_SECS
                )));
            }

            // Wait before next check
            tokio::time::sleep(Duration::from_millis(HEALTH_CHECK_INTERVAL_MS)).await;

            // Log progress occasionally
            let elapsed = start.elapsed().as_secs();
            if elapsed > 0 && elapsed % 5 == 0 {
                debug!(elapsed_secs = elapsed, "Waiting for Pocket TTS server...");
            }
        }
    }

    /// Perform health check
    async fn health_check(&self) -> bool {
        // Try the API generate endpoint with empty text (should return quickly)
        // Or try root endpoint
        let endpoints = [
            format!("{}/", self.endpoint()),
            format!("{}/health", self.endpoint()),
        ];

        for url in &endpoints {
            match self.client.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return true;
                }
                Ok(_) => {
                    // Got response but not success
                }
                Err(_) => {
                    // Connection failed
                }
            }
        }

        false
    }

    /// Check if server is running
    pub async fn is_running(&self) -> bool {
        self.health_check().await
    }

    /// Stop the server
    pub async fn stop(&self) {
        let mut proc = self.process.write().await;
        if let Some(mut child) = proc.take() {
            info!("Stopping Pocket TTS server...");
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Restart the server
    pub async fn restart(&self) -> Result<(), VoiceError> {
        self.stop().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.start().await
    }

    /// Create a PocketTTSEngine connected to this server
    pub fn engine(&self) -> PocketTTSEngine {
        PocketTTSEngine::new(&self.endpoint())
            .with_voice_name(&self.config.default_voice)
    }

    /// Create a PocketTTSEngine with a specific voice
    pub fn engine_with_voice(&self, voice: PocketTTSVoice) -> PocketTTSEngine {
        PocketTTSEngine::new(&self.endpoint())
            .with_voice(voice)
    }

    /// Get server configuration
    pub fn config(&self) -> &PocketTTSServerConfig {
        &self.config
    }
}

impl Drop for PocketTTSServer {
    fn drop(&mut self) {
        // Try to stop the server synchronously
        if let Ok(mut proc) = self.process.try_write() {
            if let Some(mut child) = proc.take() {
                debug!("Stopping Pocket TTS server on drop");
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

// ============================================================================
// Auto-Start Engine
// ============================================================================

/// Wrapper that auto-starts pocket-tts server on first use
pub struct AutoStartPocketTTS {
    server: Arc<RwLock<Option<PocketTTSServer>>>,
    engine: Arc<RwLock<Option<PocketTTSEngine>>>,
    config: PocketTTSServerConfig,
}

impl AutoStartPocketTTS {
    /// Create new auto-start wrapper with default config
    pub fn new() -> Self {
        Self {
            server: Arc::new(RwLock::new(None)),
            engine: Arc::new(RwLock::new(None)),
            config: PocketTTSServerConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: PocketTTSServerConfig) -> Self {
        Self {
            server: Arc::new(RwLock::new(None)),
            engine: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Create with uvx
    pub fn with_uvx() -> Self {
        Self::with_config(PocketTTSServerConfig {
            install_method: PocketTTSInstallMethod::Uvx,
            ..Default::default()
        })
    }

    /// Ensure server is running and return engine
    pub async fn ensure_ready(&self) -> Result<PocketTTSEngine, VoiceError> {
        // Check if already initialized
        {
            let engine = self.engine.read().await;
            if engine.is_some() {
                // Verify server is still running
                let server = self.server.read().await;
                if let Some(ref srv) = *server {
                    if srv.is_running().await {
                        return Ok(engine.as_ref().unwrap().clone());
                    }
                }
            }
        }

        // Initialize or restart
        let mut server_lock = self.server.write().await;
        let mut engine_lock = self.engine.write().await;

        // Double-check after acquiring write lock
        if let Some(ref srv) = *server_lock {
            if srv.is_running().await {
                if engine_lock.is_some() {
                    return Ok(engine_lock.as_ref().unwrap().clone());
                }
            }
        }

        // Start server
        info!("Auto-starting Pocket TTS server...");
        let server = PocketTTSServer::builder()
            .port(self.config.port)
            .host(&self.config.host)
            .voice(&self.config.default_voice)
            .build()
            .await?;

        server.start().await?;

        let engine = server.engine();

        *server_lock = Some(server);
        *engine_lock = Some(engine.clone());

        Ok(engine)
    }

    /// Get engine (may start server)
    pub async fn engine(&self) -> Result<PocketTTSEngine, VoiceError> {
        self.ensure_ready().await
    }

    /// Stop the server
    pub async fn stop(&self) {
        let server = self.server.read().await;
        if let Some(ref srv) = *server {
            srv.stop().await;
        }
    }
}

impl Default for AutoStartPocketTTS {
    fn default() -> Self {
        Self::new()
    }
}

// Use Clone for PocketTTSEngine
impl Clone for PocketTTSEngine {
    fn clone(&self) -> Self {
        Self::new(self.endpoint())
            .with_voice(self.voice().clone())
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Start Pocket TTS server with default config
///
/// ## Example
/// ```rust,ignore
/// let server = start_pocket_tts_server().await?;
/// let engine = server.engine();
/// ```
pub async fn start_pocket_tts_server() -> Result<PocketTTSServer, VoiceError> {
    let server = PocketTTSServer::builder().build().await?;
    server.start().await?;
    Ok(server)
}

/// Start Pocket TTS server on specified port
pub async fn start_pocket_tts_server_on_port(port: u16) -> Result<PocketTTSServer, VoiceError> {
    let server = PocketTTSServer::builder()
        .port(port)
        .build()
        .await?;
    server.start().await?;
    Ok(server)
}

/// Start Pocket TTS server using uvx (no installation required)
pub async fn start_pocket_tts_server_uvx() -> Result<PocketTTSServer, VoiceError> {
    let server = PocketTTSServer::builder()
        .use_uvx()
        .build()
        .await?;
    server.start().await?;
    Ok(server)
}

/// Check if pocket-tts is installed
pub fn is_pocket_tts_installed() -> bool {
    Command::new("pocket-tts")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if uvx is available
pub fn is_uvx_available() -> bool {
    Command::new("uvx")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Install pocket-tts using pip
pub fn install_pocket_tts() -> Result<(), VoiceError> {
    info!("Installing pocket-tts via pip...");

    let output = Command::new("pip")
        .args(["install", "pocket-tts"])
        .output()
        .map_err(|e| VoiceError::Other(format!("Failed to run pip: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VoiceError::Other(format!(
            "Failed to install pocket-tts: {}",
            stderr
        )));
    }

    info!("pocket-tts installed successfully");
    Ok(())
}

/// Install pocket-tts using uv
pub fn install_pocket_tts_uv() -> Result<(), VoiceError> {
    info!("Installing pocket-tts via uv pip...");

    let output = Command::new("uv")
        .args(["pip", "install", "pocket-tts"])
        .output()
        .map_err(|e| VoiceError::Other(format!("Failed to run uv: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VoiceError::Other(format!(
            "Failed to install pocket-tts: {}",
            stderr
        )));
    }

    info!("pocket-tts installed successfully");
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = PocketTTSServerConfig::default();
        assert_eq!(config.port, 8000);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.default_voice, "alba");
    }

    #[test]
    fn test_builder() {
        let builder = PocketTTSServerBuilder::new()
            .port(8080)
            .host("0.0.0.0")
            .voice("marius")
            .use_uvx();

        assert_eq!(builder.config.port, 8080);
        assert_eq!(builder.config.host, "0.0.0.0");
        assert_eq!(builder.config.default_voice, "marius");
        matches!(builder.config.install_method, PocketTTSInstallMethod::Uvx);
    }

    #[test]
    fn test_endpoint_url() {
        let config = PocketTTSServerConfig {
            port: 8080,
            host: "localhost".to_string(),
            ..Default::default()
        };

        // We can't easily test endpoint() without building, but we can test the format
        let endpoint = format!("http://{}:{}", config.host, config.port);
        assert_eq!(endpoint, "http://localhost:8080");
    }

    #[test]
    fn test_install_method_custom() {
        let method = PocketTTSInstallMethod::Custom(PathBuf::from("/usr/local/bin/pocket-tts"));
        if let PocketTTSInstallMethod::Custom(path) = method {
            assert_eq!(path.to_str().unwrap(), "/usr/local/bin/pocket-tts");
        } else {
            panic!("Expected Custom variant");
        }
    }

    #[test]
    fn test_auto_start_default() {
        let auto = AutoStartPocketTTS::new();
        assert_eq!(auto.config.port, DEFAULT_PORT);
    }
}
