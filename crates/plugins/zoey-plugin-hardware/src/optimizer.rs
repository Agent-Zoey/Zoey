//! Hardware optimization module

use crate::{GpuBackend, HardwareInfo};
use serde::{Deserialize, Serialize};

/// Hardware optimizer
pub struct HardwareOptimizer;

impl HardwareOptimizer {
    pub fn new() -> Self {
        Self
    }

    /// Recommend models based on hardware capabilities
    pub fn recommend_models(&self, hardware: &HardwareInfo) -> Vec<ModelRecommendation> {
        let mut recommendations = Vec::new();

        // Determine available memory for models
        let available_memory = if let Some(gpu) = &hardware.gpu {
            // If GPU available, use VRAM
            gpu.memory_gb.unwrap_or(hardware.available_memory_gb)
        } else {
            // Otherwise use system RAM (but leave some for OS)
            (hardware.available_memory_gb * 0.7).max(2.0)
        };

        // Small models (< 4GB)
        if available_memory >= 2.0 {
            recommendations.push(ModelRecommendation {
                model_name: "phi3:mini".to_string(),
                size_category: "small".to_string(),
                estimated_memory_gb: 2.3,
                recommended_backend: self.select_backend(hardware),
                context_length: 4096,
                speed_rating: "fast".to_string(),
            });

            recommendations.push(ModelRecommendation {
                model_name: "qwen2.5:3b".to_string(),
                size_category: "small".to_string(),
                estimated_memory_gb: 2.0,
                recommended_backend: self.select_backend(hardware),
                context_length: 32768,
                speed_rating: "fast".to_string(),
            });
        }

        // Medium models (4-8GB)
        if available_memory >= 4.0 {
            recommendations.push(ModelRecommendation {
                model_name: "llama3.2:3b".to_string(),
                size_category: "medium".to_string(),
                estimated_memory_gb: 4.0,
                recommended_backend: self.select_backend(hardware),
                context_length: 8192,
                speed_rating: "medium".to_string(),
            });

            recommendations.push(ModelRecommendation {
                model_name: "mistral:7b".to_string(),
                size_category: "medium".to_string(),
                estimated_memory_gb: 4.5,
                recommended_backend: self.select_backend(hardware),
                context_length: 8192,
                speed_rating: "medium".to_string(),
            });
        }

        // Large models (8-16GB)
        if available_memory >= 8.0 {
            recommendations.push(ModelRecommendation {
                model_name: "llama3.1:8b".to_string(),
                size_category: "large".to_string(),
                estimated_memory_gb: 8.0,
                recommended_backend: self.select_backend(hardware),
                context_length: 131072,
                speed_rating: "slow".to_string(),
            });

            recommendations.push(ModelRecommendation {
                model_name: "gemma2:9b".to_string(),
                size_category: "large".to_string(),
                estimated_memory_gb: 9.0,
                recommended_backend: self.select_backend(hardware),
                context_length: 8192,
                speed_rating: "slow".to_string(),
            });
        }

        // Extra large models (16GB+)
        if available_memory >= 16.0 {
            recommendations.push(ModelRecommendation {
                model_name: "llama3.1:70b".to_string(),
                size_category: "extra-large".to_string(),
                estimated_memory_gb: 40.0,
                recommended_backend: self.select_backend(hardware),
                context_length: 131072,
                speed_rating: "very-slow".to_string(),
            });
        }

        // Sort by estimated memory (smallest first)
        recommendations.sort_by(|a, b| {
            a.estimated_memory_gb
                .partial_cmp(&b.estimated_memory_gb)
                .unwrap()
        });

        recommendations
    }

    /// Select the optimal backend based on hardware
    pub fn select_backend(&self, hardware: &HardwareInfo) -> String {
        if let Some(gpu) = &hardware.gpu {
            match gpu.backend {
                GpuBackend::Cuda => "ollama".to_string(), // Ollama has best CUDA support
                GpuBackend::Rocm => "ollama".to_string(), // Ollama supports ROCm
                GpuBackend::Metal => "ollama".to_string(), // Ollama supports Metal
                GpuBackend::Vulkan => "llama.cpp".to_string(), // llama.cpp has Vulkan support
                GpuBackend::OpenCL => "llama.cpp".to_string(),
            }
        } else {
            // CPU-only
            "llama.cpp".to_string() // llama.cpp is more efficient for CPU
        }
    }

    /// Generate optimization configuration
    pub fn generate_config(&self, hardware: &HardwareInfo) -> OptimizationConfig {
        let has_gpu = hardware.gpu.is_some();
        let gpu_backend = hardware.gpu.as_ref().map(|g| g.backend);

        // Calculate optimal thread count
        let num_threads = if has_gpu {
            // With GPU, use fewer CPU threads
            (hardware.cpu.physical_cores / 2).max(1)
        } else {
            // CPU-only, use most cores but leave some for system
            (hardware.cpu.physical_cores - 1).max(1)
        };

        // Calculate context length based on memory
        let max_context_length = if let Some(gpu) = &hardware.gpu {
            let vram = gpu.memory_gb.unwrap_or(4.0);
            if vram >= 16.0 {
                131072
            } else if vram >= 8.0 {
                32768
            } else {
                8192
            }
        } else {
            let ram = hardware.available_memory_gb;
            if ram >= 16.0 {
                32768
            } else if ram >= 8.0 {
                8192
            } else {
                4096
            }
        };

        // Batch size based on memory
        let batch_size = if has_gpu {
            if hardware
                .gpu
                .as_ref()
                .and_then(|g| g.memory_gb)
                .unwrap_or(0.0)
                >= 8.0
            {
                512
            } else {
                256
            }
        } else {
            128
        };

        OptimizationConfig {
            backend: self.select_backend(hardware),
            use_gpu: has_gpu,
            gpu_backend: gpu_backend.map(|b| format!("{:?}", b).to_lowercase()),
            num_threads,
            max_context_length,
            batch_size,
            use_mmap: true,
            use_mlock: hardware.total_memory_gb >= 16.0,
            gpu_layers: if has_gpu { -1 } else { 0 }, // -1 means use all layers on GPU
        }
    }
}

impl Default for HardwareOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Model recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecommendation {
    pub model_name: String,
    pub size_category: String,
    pub estimated_memory_gb: f64,
    pub recommended_backend: String,
    pub context_length: usize,
    pub speed_rating: String,
}

/// Optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    pub backend: String,
    pub use_gpu: bool,
    pub gpu_backend: Option<String>,
    pub num_threads: usize,
    pub max_context_length: usize,
    pub batch_size: usize,
    pub use_mmap: bool,
    pub use_mlock: bool,
    pub gpu_layers: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuInfo, GpuInfo};

    fn create_test_hardware(memory_gb: f64, gpu: Option<GpuInfo>) -> HardwareInfo {
        HardwareInfo {
            cpu: CpuInfo {
                physical_cores: 8,
                logical_cores: 16,
                brand: "Test CPU".to_string(),
                frequency_mhz: 3000,
            },
            gpu,
            total_memory_gb: memory_gb,
            available_memory_gb: memory_gb * 0.8,
            os: "Linux".to_string(),
            arch: "x86_64".to_string(),
        }
    }

    #[test]
    fn test_recommend_models_low_memory() {
        let optimizer = HardwareOptimizer::new();
        let hardware = create_test_hardware(4.0, None);
        let recommendations = optimizer.recommend_models(&hardware);

        assert!(!recommendations.is_empty());
        assert!(recommendations.iter().all(|r| r.estimated_memory_gb <= 4.0));
    }

    #[test]
    fn test_recommend_models_high_memory() {
        let optimizer = HardwareOptimizer::new();
        let hardware = create_test_hardware(32.0, None);
        let recommendations = optimizer.recommend_models(&hardware);

        assert!(recommendations.len() >= 5);
    }

    #[test]
    fn test_select_backend_cuda() {
        let optimizer = HardwareOptimizer::new();
        let gpu = GpuInfo {
            name: "NVIDIA RTX 3090".to_string(),
            backend: GpuBackend::Cuda,
            memory_gb: Some(24.0),
            compute_capability: Some("8.6".to_string()),
        };
        let hardware = create_test_hardware(32.0, Some(gpu));

        assert_eq!(optimizer.select_backend(&hardware), "ollama");
    }

    #[test]
    fn test_select_backend_cpu_only() {
        let optimizer = HardwareOptimizer::new();
        let hardware = create_test_hardware(16.0, None);

        assert_eq!(optimizer.select_backend(&hardware), "llama.cpp");
    }

    #[test]
    fn test_generate_config() {
        let optimizer = HardwareOptimizer::new();
        let hardware = create_test_hardware(16.0, None);
        let config = optimizer.generate_config(&hardware);

        assert!(!config.use_gpu);
        assert_eq!(config.backend, "llama.cpp");
        assert!(config.num_threads > 0);
    }
}
