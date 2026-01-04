//! Hardware Detection and Optimization Plugin
//!
//! Automatically detects system hardware (CPU, GPU, RAM) and provides
//! optimization recommendations for local model deployment.
//!
//! # Features
//! - CPU detection (cores, threads, architecture)
//! - GPU detection (CUDA, ROCm, Metal)
//! - Memory detection and recommendations
//! - Model size recommendations based on available resources
//! - Automatic backend selection

use async_trait::async_trait;
use zoey_core::{Plugin, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

mod detector;
pub use detector::{CpuInfo, GpuBackend, GpuInfo, HardwareDetector, HardwareInfo};

mod optimizer;
pub use optimizer::{HardwareOptimizer, ModelRecommendation, OptimizationConfig};

// ============================================================================
// ANSI Art Banner Rendering
// ============================================================================

/// Represents a configuration setting row for display
struct SettingRow {
    name: String,
    value: String,
    is_default: bool,
    env_var: String,
}

/// Pad string to width, truncating if necessary
fn pad(s: &str, w: usize) -> String {
    let mut out = s.to_string();
    if out.len() > w {
        out.truncate(w);
    }
    let pad_len = if w > out.len() { w - out.len() } else { 0 };
    out + &" ".repeat(pad_len)
}

/// Render the Hardware plugin banner with settings
fn render_hardware_banner(rows: Vec<SettingRow>) {
    let orange = "\x1b[38;5;208m";
    let yellow = "\x1b[33m";
    let green = "\x1b[32m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // Top border
    println!("{orange}+{line}+{reset}", line = "=".repeat(78), orange = orange, reset = reset);

    // ASCII Art Header - HARDWARE with chip/circuit aesthetic
    println!(
        "{orange}|{bold}  _   _    _    ____  ______        ___    ____  _____          {reset}{orange} {dim}[==]{reset}{orange}  |{reset}",
        orange = orange, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{orange}|{bold} | | | |  / \\  |  _ \\|  _ \\ \\      / / \\  |  _ \\| ____|         {reset}{orange}{dim}|  |{reset}{orange}  |{reset}",
        orange = orange, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{orange}|{bold} | |_| | / _ \\ | |_) | | | \\ \\ /\\ / / _ \\ | |_) |  _|           {reset}{orange}{dim}|##|{reset}{orange}  |{reset}",
        orange = orange, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{orange}|{bold} |  _  |/ ___ \\|  _ <| |_| |\\ V  V / ___ \\|  _ <| |___          {reset}{orange}{dim}|  |{reset}{orange}  |{reset}",
        orange = orange, bold = bold, dim = dim, reset = reset
    );
    println!(
        "{orange}|{bold} |_| |_/_/   \\_\\_| \\_\\____/  \\_/\\_/_/   \\_\\_| \\_\\_____|         {reset}{orange}{dim}[==]{reset}{orange}  |{reset}",
        orange = orange, bold = bold, dim = dim, reset = reset
    );

    // Tagline
    println!("{orange}|{reset}", orange = orange, reset = reset);
    println!(
        "{orange}|{inner}|{reset}",
        inner = pad(&format!("    {yellow}CPU{reset}  {dim}^{reset}  {yellow}GPU{reset}  {dim}^{reset}  {yellow}Memory{reset}  {dim}^{reset}  {yellow}Optimization{reset}  {dim}^{reset}  {yellow}Auto-Detection{reset}",
            yellow = yellow, dim = dim, reset = reset), 78),
        orange = orange, reset = reset
    );

    // Separator
    println!("{orange}+{line}+{reset}", line = "-".repeat(78), orange = orange, reset = reset);

    // Settings header
    println!(
        "{orange}|{a}|{b}|{c}|{d}|{reset}",
        a = pad(&format!(" {bold}ENV VARIABLE{reset}", bold = bold, reset = reset), 34),
        b = pad(&format!(" {bold}VALUE{reset}", bold = bold, reset = reset), 20),
        c = pad(&format!(" {bold}STATUS{reset}", bold = bold, reset = reset), 12),
        d = pad(" ", 12),
        orange = orange, reset = reset
    );
    println!("{orange}+{line}+{reset}", line = "-".repeat(78), orange = orange, reset = reset);

    // Settings rows
    if rows.is_empty() {
        println!(
            "{orange}|{inner}|{reset}",
            inner = pad(&format!("  {dim}Hardware auto-detected on first use{reset}", dim = dim, reset = reset), 78),
            orange = orange, reset = reset
        );
    } else {
        for row in &rows {
            let status_color = if row.is_default { dim } else { green };
            let status_text = if row.is_default { "default" } else { "custom" };
            let status_icon = if row.is_default { " " } else { "^" };

            println!(
                "{orange}|{icon} {name}|{value}|{status}|{pad}|{reset}",
                icon = status_icon,
                name = pad(&row.env_var, 32),
                value = pad(&row.value, 20),
                status = pad(&format!("{status_color}{status_text}{reset}", status_color = status_color, status_text = status_text, reset = reset), 22),
                pad = pad("", 12),
                orange = orange, reset = reset
            );
        }
    }

    // Legend
    println!("{orange}+{line}+{reset}", line = "-".repeat(78), orange = orange, reset = reset);
    println!(
        "{orange}|{inner}|{reset}",
        inner = pad(&format!("  {green}^{reset} custom  {dim}o{reset} default  {dim}o{reset} unset  {yellow}*{reset} required  {dim}+ Set in .env{reset}",
            green = green, yellow = yellow, dim = dim, reset = reset), 78),
        orange = orange, reset = reset
    );
    println!("{orange}+{line}+{reset}", line = "=".repeat(78), orange = orange, reset = reset);
}

/// Hardware detection and optimization plugin
pub struct HardwarePlugin {
    detector: HardwareDetector,
    optimizer: HardwareOptimizer,
    cached_info: Option<Arc<HardwareInfo>>,
}

impl HardwarePlugin {
    /// Create a new hardware plugin
    pub fn new() -> Self {
        Self {
            detector: HardwareDetector::new(),
            optimizer: HardwareOptimizer::new(),
            cached_info: None,
        }
    }

    /// Detect hardware and cache the results
    pub async fn detect_hardware(&mut self) -> Result<Arc<HardwareInfo>> {
        if let Some(cached) = &self.cached_info {
            return Ok(Arc::clone(cached));
        }

        info!("Detecting system hardware...");
        let info = self.detector.detect()?;
        let arc_info = Arc::new(info);
        self.cached_info = Some(Arc::clone(&arc_info));

        Ok(arc_info)
    }

    /// Get cached hardware info (returns None if not yet detected)
    pub fn get_hardware_info(&self) -> Option<Arc<HardwareInfo>> {
        self.cached_info.clone()
    }

    /// Get model recommendations based on hardware
    pub async fn get_model_recommendations(&mut self) -> Result<Vec<ModelRecommendation>> {
        let hardware = self.detect_hardware().await?;
        Ok(self.optimizer.recommend_models(&hardware))
    }

    /// Get optimal backend for the detected hardware
    pub async fn get_optimal_backend(&mut self) -> Result<String> {
        let hardware = self.detect_hardware().await?;
        Ok(self.optimizer.select_backend(&hardware))
    }

    /// Get optimization configuration
    pub async fn get_optimization_config(&mut self) -> Result<OptimizationConfig> {
        let hardware = self.detect_hardware().await?;
        Ok(self.optimizer.generate_config(&hardware))
    }
}

impl Default for HardwarePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for HardwarePlugin {
    fn name(&self) -> &str {
        "hardware"
    }

    fn description(&self) -> &str {
        "Automatic hardware detection and optimization for local models"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        let rows = vec![
            SettingRow {
                name: "HARDWARE_FORCE_BACKEND".to_string(),
                value: std::env::var("HARDWARE_FORCE_BACKEND").unwrap_or_else(|_| "auto".to_string()),
                is_default: std::env::var("HARDWARE_FORCE_BACKEND").is_err(),
                env_var: "HARDWARE_FORCE_BACKEND".to_string(),
            },
            SettingRow {
                name: "HARDWARE_GPU_LAYERS".to_string(),
                value: std::env::var("HARDWARE_GPU_LAYERS").unwrap_or_else(|_| "auto".to_string()),
                is_default: std::env::var("HARDWARE_GPU_LAYERS").is_err(),
                env_var: "HARDWARE_GPU_LAYERS".to_string(),
            },
            SettingRow {
                name: "HARDWARE_MEMORY_LIMIT_GB".to_string(),
                value: std::env::var("HARDWARE_MEMORY_LIMIT_GB").unwrap_or_else(|_| "auto".to_string()),
                is_default: std::env::var("HARDWARE_MEMORY_LIMIT_GB").is_err(),
                env_var: "HARDWARE_MEMORY_LIMIT_GB".to_string(),
            },
            SettingRow {
                name: "HARDWARE_THREAD_COUNT".to_string(),
                value: std::env::var("HARDWARE_THREAD_COUNT").unwrap_or_else(|_| "auto".to_string()),
                is_default: std::env::var("HARDWARE_THREAD_COUNT").is_err(),
                env_var: "HARDWARE_THREAD_COUNT".to_string(),
            },
        ];
        render_hardware_banner(rows);

        info!("Hardware plugin initialized. Hardware will be detected on first use.");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hardware_detection() {
        let mut plugin = HardwarePlugin::new();
        let hardware = plugin.detect_hardware().await.unwrap();

        // Basic sanity checks
        assert!(hardware.cpu.physical_cores > 0);
        assert!(
            hardware.cpu.logical_cores >= hardware.cpu.physical_cores
                || hardware.cpu.logical_cores == 0
        );
        assert!(hardware.total_memory_gb > 0.0);
    }

    #[tokio::test]
    async fn test_model_recommendations() {
        let mut plugin = HardwarePlugin::new();
        let recommendations = plugin.get_model_recommendations().await.unwrap();

        assert!(!recommendations.is_empty());
    }

    #[tokio::test]
    async fn test_backend_selection() {
        let mut plugin = HardwarePlugin::new();
        let backend = plugin.get_optimal_backend().await.unwrap();

        assert!(!backend.is_empty());
    }
}
