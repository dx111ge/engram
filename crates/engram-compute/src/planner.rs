/// Compute planner — auto-selects CPU/GPU/NPU based on workload size.
///
/// Decision matrix:
///   - Small data (< 10K vectors): CPU with SIMD
///   - Medium data (10K-100K): NPU if available, else CPU
///   - Large data (> 100K): GPU if available, else CPU
///
/// The planner exposes a unified interface that routes to the
/// best available backend.

use crate::simd;

/// Available compute backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// CPU with optional SIMD (always available)
    Cpu,
    /// GPU via Vulkan compute shaders
    Gpu,
    /// NPU via ONNX Runtime
    Npu,
}

/// Hardware capabilities detected at runtime.
#[derive(Debug, Clone)]
pub struct HardwareInfo {
    /// CPU supports AVX2+FMA
    pub has_avx2: bool,
    /// Vulkan GPU device available
    pub has_gpu: bool,
    /// NPU (OpenVINO) available
    pub has_npu: bool,
    /// CPU core count
    pub cpu_cores: usize,
}

impl HardwareInfo {
    /// Detect available hardware.
    pub fn detect() -> Self {
        HardwareInfo {
            has_avx2: detect_avx2(),
            has_gpu: false,  // TODO: Vulkan probe
            has_npu: false,  // TODO: OpenVINO probe
            cpu_cores: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
        }
    }
}

fn detect_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// The compute planner routes operations to the best backend.
pub struct ComputePlanner {
    pub hw: HardwareInfo,
}

impl ComputePlanner {
    pub fn new() -> Self {
        ComputePlanner {
            hw: HardwareInfo::detect(),
        }
    }

    /// Select the best backend for a similarity search workload.
    pub fn select_similarity_backend(&self, vector_count: usize) -> Backend {
        if vector_count > 100_000 && self.hw.has_gpu {
            Backend::Gpu
        } else if vector_count > 10_000 && self.hw.has_npu {
            Backend::Npu
        } else {
            Backend::Cpu
        }
    }

    /// Select the best backend for graph traversal.
    pub fn select_traversal_backend(&self, node_count: usize) -> Backend {
        if node_count > 1_000_000 && self.hw.has_gpu {
            Backend::Gpu
        } else {
            Backend::Cpu
        }
    }

    /// Select the best backend for confidence propagation.
    pub fn select_propagation_backend(&self, update_count: usize) -> Backend {
        if update_count > 10_000 && self.hw.has_gpu {
            Backend::Gpu
        } else {
            Backend::Cpu
        }
    }

    /// Compute cosine distances from `query` to all `vectors`, return top `limit`.
    /// Routes to the best available backend automatically.
    pub fn similarity_search(
        &self,
        query: &[f32],
        vectors: &[&[f32]],
        limit: usize,
    ) -> Vec<(usize, f32)> {
        let backend = self.select_similarity_backend(vectors.len());
        match backend {
            Backend::Cpu => simd::batch_cosine_distances(query, vectors, limit),
            Backend::Gpu => {
                // TODO: dispatch to Vulkan compute shader
                simd::batch_cosine_distances(query, vectors, limit)
            }
            Backend::Npu => {
                // TODO: dispatch to ONNX Runtime
                simd::batch_cosine_distances(query, vectors, limit)
            }
        }
    }

    /// Compute single cosine similarity.
    pub fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        simd::cosine_similarity(a, b)
    }

    /// Compute single cosine distance.
    pub fn cosine_distance(&self, a: &[f32], b: &[f32]) -> f32 {
        simd::cosine_distance(a, b)
    }
}

impl Default for ComputePlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_hardware() {
        let hw = HardwareInfo::detect();
        assert!(hw.cpu_cores >= 1);
        // On x86_64 CI, AVX2 is typically available
        #[cfg(target_arch = "x86_64")]
        {
            // Don't assert has_avx2 since some VMs don't have it
            println!("AVX2: {}", hw.has_avx2);
        }
    }

    #[test]
    fn planner_defaults_to_cpu() {
        let planner = ComputePlanner::new();
        // Without GPU/NPU, should always select CPU
        assert_eq!(planner.select_similarity_backend(1000), Backend::Cpu);
        assert_eq!(planner.select_traversal_backend(1000), Backend::Cpu);
        assert_eq!(planner.select_propagation_backend(1000), Backend::Cpu);
    }

    #[test]
    fn planner_similarity_search() {
        let planner = ComputePlanner::new();
        let query = vec![1.0, 0.0, 0.0];
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![0.0, 1.0, 0.0];
        let vectors: Vec<&[f32]> = vec![&v1, &v2];

        let results = planner.similarity_search(&query, &vectors, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 0); // v1 is closest
    }

    #[test]
    fn cosine_via_planner() {
        let planner = ComputePlanner::new();
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = planner.cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }
}
