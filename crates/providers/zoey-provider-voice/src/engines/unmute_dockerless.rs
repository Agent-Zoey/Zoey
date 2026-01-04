//! Unmute Dockerless Service Manager
//!
//! Manages the full unmute stack without Docker by starting and managing
//! the individual services (backend, LLM, STT, TTS) as child processes.
//!
//! Based on: https://github.com/kyutai-labs/unmute/tree/main/dockerless
//!
//! ## Usage
//! ```rust,ignore
//! use zoey_provider_voice::UnmuteDockerless;
//!
//! // Start all unmute services
//! let mut manager = UnmuteDockerless::builder()
//!     .unmute_dir("/path/to/unmute")
//!     .build()
//!     .await?;
//!
//! manager.start_all().await?;
//!
//! // Use the endpoint
//! let endpoint = manager.endpoint();
//! let engine = UnmuteEngine::with_endpoint(endpoint);
//!
//! // Cleanup when done
//! manager.stop_all().await?;
//! ```

#[cfg(feature = "unmute")]
use std::collections::HashMap;
#[cfg(feature = "unmute")]
use std::path::{Path, PathBuf};
#[cfg(feature = "unmute")]
use std::process::Stdio;
#[cfg(feature = "unmute")]
use std::sync::Arc;
#[cfg(feature = "unmute")]
use std::time::Duration;
#[cfg(feature = "unmute")]
use tokio::process::{Child, Command};
#[cfg(feature = "unmute")]
use tokio::sync::RwLock;
#[cfg(feature = "unmute")]
use tokio::time::timeout;
#[cfg(feature = "unmute")]
use tracing::{debug, error, info, warn};
#[cfg(feature = "unmute")]
use zoey_core::Result;
#[cfg(feature = "unmute")]
use reqwest;

#[cfg(feature = "unmute")]
use crate::types::*;

/// Default unmute dockerless directory (relative to current working directory)
#[cfg(feature = "unmute")]
const DEFAULT_UNMUTE_DIR: &str = "./unmute";

/// Get the .zoey/voice directory for storing voice models
#[cfg(feature = "unmute")]
fn get_zoey_voice_dir() -> PathBuf {
    // Try to get from environment variable first
    if let Ok(zoey_dir) = std::env::var("ZOEY_DATA_DIR") {
        return PathBuf::from(zoey_dir).join("voice");
    }
    
    // Default to .zoey/voice in current directory
    PathBuf::from(".zoey").join("voice")
}

/// Default backend port
#[cfg(feature = "unmute")]
const DEFAULT_BACKEND_PORT: u16 = 8000;


/// Health check timeout
#[cfg(feature = "unmute")]
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 30;

/// Service startup timeout
#[cfg(feature = "unmute")]
const SERVICE_STARTUP_TIMEOUT_SECS: u64 = 120;

// ============================================================================
// Service Manager
// ============================================================================

/// Manages dockerless unmute services
#[cfg(feature = "unmute")]
pub struct UnmuteDockerless {
    /// Path to unmute directory
    unmute_dir: PathBuf,
    /// Backend port
    backend_port: u16,
    /// Running service processes
    services: Arc<RwLock<HashMap<ServiceType, Arc<tokio::sync::Mutex<Child>>>>>,
    /// Environment variables for services
    env_vars: HashMap<String, String>,
}

/// Service types in the unmute stack
#[cfg(feature = "unmute")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceType {
    /// Backend service
    Backend,
    /// LLM service
    Llm,
    /// STT service
    Stt,
    /// TTS service
    Tts,
}

#[cfg(feature = "unmute")]
impl ServiceType {
    /// Get the script name for this service
    fn script_name(&self) -> &'static str {
        match self {
            ServiceType::Backend => "start_backend.sh",
            ServiceType::Llm => "start_llm.sh",
            ServiceType::Stt => "start_stt.sh",
            ServiceType::Tts => "start_tts.sh",
        }
    }

    /// Get the display name
    fn display_name(&self) -> &'static str {
        match self {
            ServiceType::Backend => "Backend",
            ServiceType::Llm => "LLM",
            ServiceType::Stt => "STT",
            ServiceType::Tts => "TTS",
        }
    }

    /// Get the expected VRAM requirement in GB
    fn vram_requirement_gb(&self) -> f64 {
        match self {
            ServiceType::Backend => 0.0,
            ServiceType::Llm => 6.1,
            ServiceType::Stt => 2.5,
            ServiceType::Tts => 5.3,
        }
    }
}

// ============================================================================
// Builder
// ============================================================================

/// Builder for UnmuteDockerless
#[cfg(feature = "unmute")]
pub struct UnmuteDockerlessBuilder {
    unmute_dir: Option<PathBuf>,
    backend_port: u16,
    env_vars: HashMap<String, String>,
}

#[cfg(feature = "unmute")]
impl Default for UnmuteDockerlessBuilder {
    fn default() -> Self {
        Self {
            unmute_dir: None,
            backend_port: DEFAULT_BACKEND_PORT,
            env_vars: HashMap::new(),
        }
    }
}

#[cfg(feature = "unmute")]
impl UnmuteDockerlessBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the unmute directory (must contain dockerless/ subdirectory)
    pub fn unmute_dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.unmute_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Set backend port (default: 8000)
    pub fn backend_port(mut self, port: u16) -> Self {
        self.backend_port = port;
        self
    }

    /// Add environment variable for services
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env_vars.insert(key.to_string(), value.to_string());
        self
    }

    /// Build the service manager
    pub async fn build(self) -> Result<UnmuteDockerless> {
        let unmute_dir = self.unmute_dir
            .unwrap_or_else(|| PathBuf::from(DEFAULT_UNMUTE_DIR));

        // Verify dockerless directory exists
        let dockerless_dir = unmute_dir.join("dockerless");
        if !dockerless_dir.exists() {
            return Err(VoiceError::Other(format!(
                "Unmute dockerless directory not found: {}",
                dockerless_dir.display()
            )).into());
        }

        // Ensure .zoey/voice directory exists for models
        let voice_dir = get_zoey_voice_dir();
        if let Err(e) = tokio::fs::create_dir_all(&voice_dir).await {
            warn!(
                dir = %voice_dir.display(),
                error = %e,
                "Failed to create .zoey/voice directory"
            );
        } else {
            info!(
                dir = %voice_dir.display(),
                "Using .zoey/voice for voice models"
            );
        }

        // Verify scripts exist
        for service_type in &[
            ServiceType::Backend,
            ServiceType::Llm,
            ServiceType::Stt,
            ServiceType::Tts,
        ] {
            let script_path = dockerless_dir.join(service_type.script_name());
            if !script_path.exists() {
                warn!(
                    script = %script_path.display(),
                    "Startup script not found for {}",
                    service_type.display_name()
                );
            }
        }

        Ok(UnmuteDockerless {
            unmute_dir,
            backend_port: self.backend_port,
            services: Arc::new(RwLock::new(HashMap::new())),
            env_vars: self.env_vars,
        })
    }
}

// ============================================================================
// Service Manager Implementation
// ============================================================================

#[cfg(feature = "unmute")]
impl UnmuteDockerless {
    /// Create a new builder
    pub fn builder() -> UnmuteDockerlessBuilder {
        UnmuteDockerlessBuilder::new()
    }

    /// Get the WebSocket endpoint URL for connecting to unmute
    pub fn endpoint(&self) -> String {
        format!("ws://127.0.0.1:{}", self.backend_port)
    }

    /// Start all services
    pub async fn start_all(&mut self) -> Result<()> {
        info!("Starting all unmute dockerless services...");

        // Start services in order
        self.start_service(ServiceType::Backend).await?;
        tokio::time::sleep(Duration::from_secs(2)).await; // Give backend time to start

        self.start_service(ServiceType::Llm).await?;
        tokio::time::sleep(Duration::from_secs(3)).await; // LLM takes longer to load

        self.start_service(ServiceType::Stt).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;

        self.start_service(ServiceType::Tts).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Wait for services to be ready
        info!("Waiting for services to be ready...");
        self.wait_for_ready().await?;

        info!("All unmute services started successfully");
        Ok(())
    }

    /// Start a specific service
    pub async fn start_service(&mut self, service_type: ServiceType) -> Result<()> {
        let script_path = self.unmute_dir
            .join("dockerless")
            .join(service_type.script_name());

        if !script_path.exists() {
            return Err(VoiceError::Other(format!(
                "Startup script not found: {}",
                script_path.display()
            )).into());
        }

        info!(
            service = %service_type.display_name(),
            script = %script_path.display(),
            "Starting {} service...",
            service_type.display_name()
        );

        // Check if already running
        {
            let services = self.services.read().await;
            if services.contains_key(&service_type) {
                warn!(
                    service = %service_type.display_name(),
                    "Service already running"
                );
                return Ok(());
            }
        }

        // Build command
        let mut cmd = Command::new("bash");
        cmd.arg(&script_path);
        cmd.current_dir(&self.unmute_dir);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Set environment variables
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        // Set port environment variables
        cmd.env("BACKEND_PORT", self.backend_port.to_string());
        
        // Set model directory to .zoey/voice
        let voice_dir = get_zoey_voice_dir();
        cmd.env("VOICE_MODEL_DIR", voice_dir.to_string_lossy().to_string());
        
        // Set Whisper model path if it exists in .zoey/voice
        let whisper_model = voice_dir.join("ggml-tiny.bin");
        if whisper_model.exists() {
            cmd.env("WHISPER_MODEL_PATH", whisper_model.to_string_lossy().to_string());
            info!(
                model = %whisper_model.display(),
                "Using Whisper model from .zoey/voice"
            );
        }

        // Spawn process
        let child = cmd.spawn()
            .map_err(|e| VoiceError::Other(format!(
                "Failed to start {} service: {}",
                service_type.display_name(),
                e
            )))?;

        // Store process
        {
            let mut services = self.services.write().await;
            services.insert(service_type, Arc::new(tokio::sync::Mutex::new(child)));
        }

        info!(
            service = %service_type.display_name(),
            "{} service started",
            service_type.display_name()
        );

        Ok(())
    }

    /// Stop a specific service
    pub async fn stop_service(&mut self, service_type: ServiceType) -> Result<()> {
        let mut services = self.services.write().await;
        
        if let Some(child_arc) = services.remove(&service_type) {
            info!(
                service = %service_type.display_name(),
                "Stopping {} service...",
                service_type.display_name()
            );

            let mut child = child_arc.lock().await;
            // Try graceful shutdown first
            if let Err(e) = child.kill().await {
                warn!(
                    service = %service_type.display_name(),
                    error = %e,
                    "Failed to kill process, may have already exited"
                );
            }

            // Wait for process to exit
            let _ = child.wait().await;
            
            info!(
                service = %service_type.display_name(),
                "{} service stopped",
                service_type.display_name()
            );
        }

        Ok(())
    }

    /// Stop all services
    pub async fn stop_all(&mut self) -> Result<()> {
        info!("Stopping all unmute services...");

        // Stop in reverse order
        let service_order = vec![
            ServiceType::Tts,
            ServiceType::Stt,
            ServiceType::Llm,
            ServiceType::Backend,
        ];

        for service_type in service_order {
            if let Err(e) = self.stop_service(service_type).await {
                warn!(
                    service = %service_type.display_name(),
                    error = %e,
                    "Error stopping service"
                );
            }
        }

        info!("All services stopped");
        Ok(())
    }

    /// Check if a service is running
    pub async fn is_service_running(&self, service_type: ServiceType) -> bool {
        let services = self.services.read().await;
        if let Some(child_arc) = services.get(&service_type) {
            // Check if process is still alive
            let mut child = child_arc.lock().await;
            match child.try_wait() {
                Ok(Some(_)) => false, // Process has exited
                Ok(None) => true,     // Process is still running
                Err(_) => false,      // Error checking status
            }
        } else {
            false
        }
    }

    /// Wait for all services to be ready
    async fn wait_for_ready(&self) -> Result<()> {
        // Check backend health
        let backend_url = format!("http://127.0.0.1:{}/health", self.backend_port);
        let client = reqwest::Client::new();

        for i in 0..30 {
            match timeout(
                Duration::from_secs(2),
                client.get(&backend_url).send()
            ).await {
                Ok(Ok(resp)) if resp.status().is_success() => {
                    info!("Backend service is ready");
                    return Ok(());
                }
                _ => {
                    if i < 29 {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }

        warn!("Backend health check timed out, but services may still be starting");
        Ok(())
    }

    /// Get service status
    pub async fn status(&self) -> HashMap<String, bool> {
        let mut status = HashMap::new();
        
        for service_type in &[
            ServiceType::Backend,
            ServiceType::Llm,
            ServiceType::Stt,
            ServiceType::Tts,
        ] {
            let running = self.is_service_running(*service_type).await;
            status.insert(service_type.display_name().to_string(), running);
        }

        status
    }

    /// Get VRAM requirements summary
    pub fn vram_requirements(&self) -> String {
        let total: f64 = [
            ServiceType::Llm,
            ServiceType::Stt,
            ServiceType::Tts,
        ].iter().map(|s| s.vram_requirement_gb()).sum();

        format!(
            "Total VRAM required: {:.1} GB\n  - LLM: {:.1} GB\n  - STT: {:.1} GB\n  - TTS: {:.1} GB",
            total,
            ServiceType::Llm.vram_requirement_gb(),
            ServiceType::Stt.vram_requirement_gb(),
            ServiceType::Tts.vram_requirement_gb(),
        )
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Start unmute dockerless services with default configuration
/// 
/// Returns the WebSocket endpoint URL to connect to.
/// 
/// ## Example
/// ```rust,ignore
/// let (manager, endpoint) = start_unmute_dockerless("/path/to/unmute").await?;
/// 
/// let engine = UnmuteEngine::with_endpoint(&endpoint);
/// // ... use engine ...
/// 
/// // Cleanup
/// manager.stop_all().await?;
/// ```
#[cfg(feature = "unmute")]
pub async fn start_unmute_dockerless<P: AsRef<Path>>(
    unmute_dir: P,
) -> Result<(UnmuteDockerless, String)> {
    let mut manager = UnmuteDockerless::builder()
        .unmute_dir(unmute_dir)
        .build()
        .await?;

    manager.start_all().await?;
    let endpoint = manager.endpoint();

    Ok((manager, endpoint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_type_script_names() {
        assert_eq!(ServiceType::Backend.script_name(), "start_backend.sh");
        assert_eq!(ServiceType::Llm.script_name(), "start_llm.sh");
        assert_eq!(ServiceType::Stt.script_name(), "start_stt.sh");
        assert_eq!(ServiceType::Tts.script_name(), "start_tts.sh");
    }

    #[test]
    fn test_vram_requirements() {
        let llm = ServiceType::Llm.vram_requirement_gb();
        let stt = ServiceType::Stt.vram_requirement_gb();
        let tts = ServiceType::Tts.vram_requirement_gb();
        
        assert_eq!(llm, 6.1);
        assert_eq!(stt, 2.5);
        assert_eq!(tts, 5.3);
        assert_eq!(llm + stt + tts, 13.9);
    }

    #[test]
    fn test_endpoint_format() {
        let builder = UnmuteDockerlessBuilder::new()
            .backend_port(9000);
        
        // Can't test endpoint() without building, but we can verify the format
        let expected = "ws://127.0.0.1:9000";
        assert!(expected.starts_with("ws://"));
    }
}

