/// engram-compute: hardware-accelerated compute for similarity search,
/// graph traversal, and confidence propagation.
///
/// Compute backends:
///   - CPU scalar (always available)
///   - CPU SIMD (AVX2 on x86_64, NEON on aarch64)
///   - GPU (Vulkan compute shaders — future)
///   - NPU (ONNX Runtime with OpenVINO EP — future)

pub mod planner;
pub mod simd;
pub mod vulkan;
