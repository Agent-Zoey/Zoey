//! Hardware detection module

use zoey_core::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;
use sysinfo::System;
use tracing::debug;

/// Hardware information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu: CpuInfo,
    pub gpu: Option<GpuInfo>,
    pub total_memory_gb: f64,
    pub available_memory_gb: f64,
    pub os: String,
    pub arch: String,
}

/// CPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub brand: String,
    pub frequency_mhz: u64,
}

/// GPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub backend: GpuBackend,
    pub memory_gb: Option<f64>,
    pub compute_capability: Option<String>,
}

/// GPU backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuBackend {
    Cuda,
    Rocm,
    Metal,
    Vulkan,
    OpenCL,
}

/// Hardware detector
pub struct HardwareDetector {
    system: System,
}

impl HardwareDetector {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
        }
    }

    /// Detect all hardware
    pub fn detect(&mut self) -> Result<HardwareInfo> {
        self.system.refresh_all();

        let cpu = self.detect_cpu();
        let gpu = self.detect_gpu();
        let (total_memory_gb, available_memory_gb) = self.detect_memory();
        let os = self.detect_os();
        let arch = self.detect_arch();

        Ok(HardwareInfo {
            cpu,
            gpu,
            total_memory_gb,
            available_memory_gb,
            os,
            arch,
        })
    }

    fn detect_cpu(&self) -> CpuInfo {
        let mut physical_cores = num_cpus::get_physical();
        let mut logical_cores = num_cpus::get();

        // Some environments may report 0 physical cores; fallback to 1
        if physical_cores == 0 {
            physical_cores = 1;
        }

        // Ensure logical cores is at least physical cores when non-zero
        if logical_cores > 0 && logical_cores < physical_cores {
            logical_cores = physical_cores;
        }

        let brand = self
            .system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let frequency_mhz = self
            .system
            .cpus()
            .first()
            .map(|cpu| cpu.frequency())
            .unwrap_or(0);

        CpuInfo {
            physical_cores,
            logical_cores,
            brand,
            frequency_mhz,
        }
    }

    fn detect_gpu(&self) -> Option<GpuInfo> {
        // Try CUDA first
        if let Some(cuda_info) = self.detect_cuda() {
            return Some(cuda_info);
        }

        // Try ROCm
        if let Some(rocm_info) = self.detect_rocm() {
            return Some(rocm_info);
        }

        // Try Metal (macOS)
        if let Some(metal_info) = self.detect_metal() {
            return Some(metal_info);
        }

        // Fallback: try to detect any GPU
        self.detect_generic_gpu()
    }

    fn detect_cuda(&self) -> Option<GpuInfo> {
        // Try nvidia-smi
        let output = Command::new("nvidia-smi")
            .args(["--query-gpu=name,memory.total", "--format=csv,noheader"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = stdout.trim().split(',').collect();

                if parts.len() >= 2 {
                    let name = parts[0].trim().to_string();
                    let memory_str = parts[1].trim().replace(" MiB", "");

                    let memory_gb = memory_str.parse::<f64>().ok().map(|mb| mb / 1024.0);

                    debug!("Detected CUDA GPU: {}", name);

                    return Some(GpuInfo {
                        name,
                        backend: GpuBackend::Cuda,
                        memory_gb,
                        compute_capability: self.get_cuda_compute_capability(),
                    });
                }
            }
        }

        None
    }

    fn get_cuda_compute_capability(&self) -> Option<String> {
        let output = Command::new("nvidia-smi")
            .args(["--query-gpu=compute_cap", "--format=csv,noheader"])
            .output()
            .ok()?;

        if output.status.success() {
            let cap = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Some(cap);
        }

        None
    }

    fn detect_rocm(&self) -> Option<GpuInfo> {
        // Try rocm-smi
        let output = Command::new("rocm-smi")
            .args(["--showproductname"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);

                // Parse GPU name from output
                for line in stdout.lines() {
                    if line.contains("Card series:") || line.contains("Card model:") {
                        let name = line
                            .split(':')
                            .nth(1)
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|| "AMD GPU".to_string());

                        debug!("Detected ROCm GPU: {}", name);

                        return Some(GpuInfo {
                            name,
                            backend: GpuBackend::Rocm,
                            memory_gb: None, // ROCm memory detection is more complex
                            compute_capability: None,
                        });
                    }
                }
            }
        }

        None
    }

    fn detect_metal(&self) -> Option<GpuInfo> {
        // Metal is macOS only
        if cfg!(target_os = "macos") {
            let output = Command::new("system_profiler")
                .args(["SPDisplaysDataType"])
                .output();

            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);

                    // Parse GPU name from output
                    for line in stdout.lines() {
                        if line.contains("Chipset Model:") {
                            let name = line
                                .split(':')
                                .nth(1)
                                .map(|s| s.trim().to_string())
                                .unwrap_or_else(|| "Apple GPU".to_string());

                            debug!("Detected Metal GPU: {}", name);

                            return Some(GpuInfo {
                                name,
                                backend: GpuBackend::Metal,
                                memory_gb: None,
                                compute_capability: None,
                            });
                        }
                    }
                }
            }
        }

        None
    }

    fn detect_generic_gpu(&self) -> Option<GpuInfo> {
        // Try lspci on Linux
        if cfg!(target_os = "linux") {
            let output = Command::new("lspci").output();

            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);

                    for line in stdout.lines() {
                        if line.contains("VGA compatible controller")
                            || line.contains("3D controller")
                        {
                            let name = line
                                .split(':')
                                .last()
                                .map(|s| s.trim().to_string())
                                .unwrap_or_else(|| "Unknown GPU".to_string());

                            debug!("Detected generic GPU: {}", name);

                            return Some(GpuInfo {
                                name,
                                backend: GpuBackend::Vulkan, // Assume Vulkan support
                                memory_gb: None,
                                compute_capability: None,
                            });
                        }
                    }
                }
            }
        }

        None
    }

    fn detect_memory(&self) -> (f64, f64) {
        let total = self.system.total_memory() as f64 / 1_073_741_824.0; // Convert to GB
        let available = self.system.available_memory() as f64 / 1_073_741_824.0;

        (total, available)
    }

    fn detect_os(&self) -> String {
        System::name().unwrap_or_else(|| "Unknown".to_string())
    }

    fn detect_arch(&self) -> String {
        std::env::consts::ARCH.to_string()
    }
}

impl Default for HardwareDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_cpu() {
        let detector = HardwareDetector::new();
        let cpu = detector.detect_cpu();

        assert!(cpu.physical_cores > 0);
        assert!(cpu.logical_cores >= cpu.physical_cores || cpu.logical_cores == 0);
        assert!(!cpu.brand.is_empty());
    }

    #[test]
    fn test_detect_memory() {
        let detector = HardwareDetector::new();
        let (total, available) = detector.detect_memory();

        assert!(total > 0.0);
        assert!(available > 0.0);
        assert!(available <= total);
    }

    #[test]
    fn test_detect_hardware() {
        let mut detector = HardwareDetector::new();
        let hardware = detector.detect().unwrap();

        assert!(hardware.cpu.physical_cores > 0);
        assert!(hardware.total_memory_gb > 0.0);
        assert!(!hardware.os.is_empty());
        assert!(!hardware.arch.is_empty());
    }
}
