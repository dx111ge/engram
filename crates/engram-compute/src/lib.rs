/// engram-compute: hardware-accelerated compute for similarity search,
/// graph traversal, and confidence propagation.
///
/// Compute backends:
///   - CPU scalar (always available)
///   - CPU SIMD (AVX2+FMA on x86_64, NEON on aarch64)
///   - GPU (wgpu compute shaders — DX12/Vulkan/Metal)
///   - NPU (low-power adapter via wgpu + platform-specific detection)

pub mod gpu;
pub mod npu;
pub mod planner;
pub mod simd;
