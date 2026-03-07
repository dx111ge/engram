/// Compute planner — auto-selects CPU/GPU/NPU based on workload size
/// and available hardware.
///
/// Decision matrix:
///   - Small data (< 10K vectors): CPU with SIMD
///   - Medium data (10K-100K): NPU if available, else CPU
///   - Large data (> 100K): GPU if available, else NPU, else CPU
///
/// The planner detects hardware at construction and caches device handles.

use crate::simd;

/// Available compute backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// CPU with optional SIMD (always available)
    Cpu,
    /// GPU via wgpu compute shaders
    Gpu,
    /// NPU via wgpu low-power adapter
    Npu,
}

/// Hardware capabilities detected at runtime.
#[derive(Debug, Clone)]
pub struct HardwareInfo {
    /// CPU supports AVX2+FMA (x86_64)
    pub has_avx2: bool,
    /// CPU supports NEON (aarch64)
    pub has_neon: bool,
    /// GPU compute device available
    pub has_gpu: bool,
    /// GPU device name
    pub gpu_name: String,
    /// GPU backend API (Vulkan, DX12, Metal)
    pub gpu_backend: String,
    /// NPU / low-power compute device available
    pub has_npu: bool,
    /// NPU device name
    pub npu_name: String,
    /// Dedicated NPU hardware detected (informational)
    pub dedicated_npu: Vec<String>,
    /// CPU core count
    pub cpu_cores: usize,
}

impl HardwareInfo {
    /// Detect available hardware at runtime.
    pub fn detect() -> Self {
        let mut info = HardwareInfo {
            has_avx2: detect_avx2(),
            has_neon: detect_neon(),
            has_gpu: false,
            gpu_name: String::new(),
            gpu_backend: String::new(),
            has_npu: false,
            npu_name: String::new(),
            dedicated_npu: Vec::new(),
            cpu_cores: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
        };

        // Detect dedicated NPU hardware
        let npu_infos = crate::npu::NpuInfo::detect();
        info.dedicated_npu = npu_infos
            .iter()
            .map(|n| format!("{} ({})", n.name, n.vendor))
            .collect();

        // GPU detection via wgpu
        #[cfg(feature = "gpu")]
        {
            if let Some(gpu) = crate::gpu::GpuDevice::probe() {
                info.gpu_name = gpu.name.clone();
                info.gpu_backend = gpu.backend.clone();
                info.has_gpu = true;
                // Store GPU for later use
                // (The planner creates its own device handles)
            }

            if let Some(npu) = crate::npu::NpuDevice::probe() {
                info.npu_name = npu.name().to_string();
                info.has_npu = true;
            }
        }

        info
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

fn detect_neon() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        true // NEON is baseline on aarch64
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        false
    }
}

/// The compute planner routes operations to the best backend.
pub struct ComputePlanner {
    pub hw: HardwareInfo,
    #[cfg(feature = "gpu")]
    gpu: Option<crate::gpu::GpuDevice>,
    #[cfg(feature = "gpu")]
    npu: Option<crate::npu::NpuDevice>,
}

impl ComputePlanner {
    pub fn new() -> Self {
        let hw = HardwareInfo::detect();

        #[cfg(feature = "gpu")]
        let gpu = crate::gpu::GpuDevice::probe();
        #[cfg(feature = "gpu")]
        let npu = crate::npu::NpuDevice::probe();

        ComputePlanner {
            hw,
            #[cfg(feature = "gpu")]
            gpu,
            #[cfg(feature = "gpu")]
            npu,
        }
    }

    /// Select the best backend for a similarity search workload.
    pub fn select_similarity_backend(&self, vector_count: usize) -> Backend {
        let has_gpu;
        let has_npu;
        #[cfg(feature = "gpu")]
        {
            has_gpu = self.gpu.is_some();
            has_npu = self.npu.is_some();
        }
        #[cfg(not(feature = "gpu"))]
        {
            has_gpu = false;
            has_npu = false;
        }

        if vector_count > 100_000 && has_gpu {
            Backend::Gpu
        } else if vector_count > 10_000 && has_npu {
            Backend::Npu
        } else if vector_count > 100_000 && has_npu {
            // Fallback: NPU for large workloads when no discrete GPU
            Backend::Npu
        } else {
            Backend::Cpu
        }
    }

    /// Select the best backend for graph traversal.
    pub fn select_traversal_backend(&self, node_count: usize) -> Backend {
        let has_gpu;
        #[cfg(feature = "gpu")]
        {
            has_gpu = self.gpu.is_some();
        }
        #[cfg(not(feature = "gpu"))]
        {
            has_gpu = false;
        }

        if node_count > 1_000_000 && has_gpu {
            Backend::Gpu
        } else {
            Backend::Cpu
        }
    }

    /// Select the best backend for confidence propagation.
    pub fn select_propagation_backend(&self, update_count: usize) -> Backend {
        let has_gpu;
        #[cfg(feature = "gpu")]
        {
            has_gpu = self.gpu.is_some();
        }
        #[cfg(not(feature = "gpu"))]
        {
            has_gpu = false;
        }

        if update_count > 10_000 && has_gpu {
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

        #[cfg(feature = "gpu")]
        {
            if backend == Backend::Gpu || backend == Backend::Npu {
                let dim = query.len();
                let count = vectors.len();
                let flat: Vec<f32> =
                    vectors.iter().flat_map(|v| v.iter().copied()).collect();

                let distances = if backend == Backend::Gpu {
                    self.gpu
                        .as_ref()
                        .and_then(|g| g.batch_cosine_distances(query, &flat, dim, count))
                } else {
                    self.npu
                        .as_ref()
                        .and_then(|n| n.batch_cosine_distances(query, &flat, dim, count))
                };

                if let Some(dists) = distances {
                    let mut ranked: Vec<(usize, f32)> =
                        dists.into_iter().enumerate().collect();
                    ranked.sort_by(|a, b| {
                        a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    ranked.truncate(limit);
                    return ranked;
                }
                // Fall through to CPU on failure
            }
        }

        simd::batch_cosine_distances(query, vectors, limit)
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
        println!("CPU cores: {}", hw.cpu_cores);
        println!("AVX2+FMA: {}", hw.has_avx2);
        println!("NEON: {}", hw.has_neon);
        println!("GPU: {} ({}{})", hw.has_gpu, hw.gpu_name, if !hw.gpu_backend.is_empty() { format!(", {}", hw.gpu_backend) } else { String::new() });
        println!("NPU compute: {} ({})", hw.has_npu, hw.npu_name);
        if !hw.dedicated_npu.is_empty() {
            println!("Dedicated NPU hardware:");
            for npu in &hw.dedicated_npu {
                println!("  - {npu}");
            }
        }
    }

    #[test]
    fn planner_routes_correctly() {
        let planner = ComputePlanner::new();
        // Small workloads always go to CPU
        assert_eq!(planner.select_similarity_backend(100), Backend::Cpu);
        assert_eq!(planner.select_traversal_backend(100), Backend::Cpu);
        assert_eq!(planner.select_propagation_backend(100), Backend::Cpu);
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
