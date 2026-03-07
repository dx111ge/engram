# Engram Compute Backends

Hardware-accelerated compute for similarity search, graph traversal, and confidence propagation. Engram auto-detects available hardware and routes workloads to the fastest backend.

## Backends Overview

| Backend | Technology | Platforms | Use Case |
|---------|-----------|-----------|----------|
| **CPU Scalar** | Pure Rust | All | Always available fallback |
| **CPU AVX2+FMA** | x86_64 SIMD | Windows, Linux (Intel/AMD) | 8-wide f32, ~4x scalar |
| **CPU NEON** | aarch64 SIMD | macOS (Apple Silicon), Linux (ARM) | 4-wide f32, ~3x scalar |
| **GPU (DX12)** | wgpu compute shaders | Windows 10/11 | NVIDIA, AMD, Intel GPUs |
| **GPU (Vulkan)** | wgpu compute shaders | Windows, Linux | NVIDIA, AMD, Intel GPUs |
| **GPU (Metal)** | wgpu compute shaders | macOS | Apple M-series, AMD GPUs |
| **NPU** | wgpu low-power adapter | All | Integrated GPU / NPU-like devices |

## How It Works

Engram uses [wgpu](https://wgpu.rs/) as a unified GPU compute abstraction. wgpu auto-selects the best graphics API per platform:

```
Windows  -->  DX12 (default) or Vulkan (with SDK)
macOS    -->  Metal
Linux    -->  Vulkan
```

The same WGSL compute shader runs identically on NVIDIA, AMD, Intel, and Apple GPUs. No vendor-specific code, no shader variants.

## Hardware Detection

At startup, the `ComputePlanner` probes all available hardware:

```rust
use engram_compute::planner::{ComputePlanner, HardwareInfo};

let hw = HardwareInfo::detect();
println!("CPU cores: {}", hw.cpu_cores);
println!("AVX2+FMA: {}", hw.has_avx2);
println!("NEON: {}", hw.has_neon);
println!("GPU: {} ({})", hw.gpu_name, hw.gpu_backend);
println!("NPU: {}", hw.npu_name);
```

Example output on a Windows workstation:

```
CPU cores: 20
AVX2+FMA: true
NEON: false
GPU: NVIDIA GeForce RTX 5070 (Vulkan)
NPU compute: Intel(R) Graphics
```

Example output on an Apple MacBook Pro:

```
CPU cores: 12
AVX2+FMA: false
NEON: true
GPU: Apple M3 Pro (Metal)
NPU compute: Apple M3 Pro (Metal)
```

### Adapter Enumeration

All compute-capable devices are enumerated via wgpu:

```rust
use engram_compute::gpu::GpuDevice;

for adapter in GpuDevice::enumerate_adapters() {
    println!("{}", adapter);
}
```

Real output from a Windows machine with discrete + integrated GPU:

```
NVIDIA GeForce RTX 5070 (Vulkan, DiscreteGpu, NVIDIA)
Intel(R) Graphics (Vulkan, IntegratedGpu, Intel Corporation)
NVIDIA GeForce RTX 5070 (Dx12, DiscreteGpu, 32.0.15.9186)
Intel(R) Graphics (Dx12, IntegratedGpu, 32.0.101.8509)
Microsoft Basic Render Driver (Dx12, Cpu, 10.0.26100.7309)
```

## Workload Routing

The `ComputePlanner` selects the best backend based on workload size:

| Workload Size | Primary | Fallback |
|--------------|---------|----------|
| < 10K vectors | CPU (SIMD) | -- |
| 10K - 100K vectors | NPU | CPU |
| > 100K vectors | GPU | NPU, then CPU |

```rust
use engram_compute::planner::ComputePlanner;

let planner = ComputePlanner::new();

// Auto-routes to GPU for large workloads, CPU for small ones
let results = planner.similarity_search(&query, &vectors, 10);
```

The planner always falls back gracefully. If GPU compute fails mid-operation, it retries on CPU. No workload is ever dropped.

## CPU Backend

### AVX2+FMA (x86_64)

Processes 8 floats per instruction using 256-bit registers. Available on Intel Haswell (2013+) and AMD Zen (2017+). Runtime-detected via `is_x86_feature_detected!`.

Operations:
- `_mm256_fmadd_ps` -- fused multiply-add (a*b+c in one cycle)
- `_mm256_loadu_ps` -- unaligned 8-wide load
- Horizontal sum via `_mm256_extractf128_ps` + `_mm_add_ps` cascade

### NEON (aarch64)

Processes 4 floats per instruction using 128-bit registers. Always available on AArch64 (ARM v8+). This covers Apple M1/M2/M3/M4 and ARM Linux servers.

Operations:
- `vfmaq_f32` -- fused multiply-add
- `vld1q_f32` -- 4-wide load
- `vaddvq_f32` -- horizontal sum (single instruction on ARMv8)

### Performance

Measured on a 20-core Intel workstation (AVX2+FMA):

| Operation | Dimension | Count | Time |
|-----------|----------|-------|------|
| Cosine similarity batch | 384 (MiniLM) | 10,000 | 22ms |
| Cosine similarity batch | 768 (BERT) | 10,000 | ~44ms |
| Cosine similarity batch | 1536 (OpenAI) | 10,000 | ~88ms |

## GPU Backend

### Architecture

The GPU backend uses a WGSL compute shader that runs one workgroup invocation per vector:

```wgsl
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= params.count) { return; }

    let base = idx * params.dim;
    var dot: f32 = 0.0;
    var nq: f32 = 0.0;
    var nv: f32 = 0.0;

    for (var i = 0u; i < params.dim; i++) {
        let q = query[i];
        let v = vectors[base + i];
        dot += q * v;
        nq += q * q;
        nv += v * v;
    }

    let denom = sqrt(nq * nv);
    distances[idx] = select(1.0 - dot / denom, 1.0, denom < 0.0000001);
}
```

Each of the GPU's thousands of cores computes one cosine distance in parallel. For 10K vectors, that is 10,000 independent parallel computations.

### Buffer Layout

```
Binding 0: query     [D]     -- storage, read
Binding 1: vectors   [N*D]   -- storage, read  (flattened NxD matrix)
Binding 2: distances [N]     -- storage, write  (output)
Binding 3: params    {dim,N} -- uniform
```

Data flows: CPU RAM --> GPU VRAM (upload) --> Compute --> GPU VRAM --> CPU RAM (readback).

### Graphics API Support

| API | Platform | GPU Vendors | Notes |
|-----|----------|------------|-------|
| **DX12** | Windows 10/11 | NVIDIA, AMD, Intel | Default on Windows, no SDK needed |
| **Vulkan** | Windows, Linux | NVIDIA, AMD, Intel | Requires Vulkan runtime |
| **Metal** | macOS 10.13+ | Apple, AMD | Default on macOS |

wgpu selects the best API automatically. You can force a specific backend:

```rust
let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
    backends: wgpu::Backends::VULKAN, // Force Vulkan
    ..Default::default()
});
```

### Direct GPU Usage

```rust
use engram_compute::gpu::GpuDevice;

let gpu = GpuDevice::probe().expect("no GPU found");
println!("Using: {} ({})", gpu.name, gpu.backend);

let query: Vec<f32> = embed("PostgreSQL");         // 384-dim
let vectors_flat: Vec<f32> = all_embeddings();      // N * 384
let dim = 384;
let count = vectors_flat.len() / dim;

let distances = gpu
    .batch_cosine_distances(&query, &vectors_flat, dim, count)
    .expect("GPU compute failed");

// distances[i] = cosine distance from query to vector i
// Lower = more similar. 0.0 = identical, 1.0 = orthogonal, 2.0 = opposite.
```

### Performance

Measured on NVIDIA GeForce RTX 5070 (Vulkan):

| Operation | Dimension | Count | GPU Time | CPU Time | Speedup |
|-----------|----------|-------|----------|----------|---------|
| Cosine similarity batch | 384 | 10,000 | 3ms | 22ms | 7x |
| Cosine similarity batch | 384 | 100,000 | ~15ms | ~220ms | 15x |
| Cosine similarity batch | 384 | 1,000,000 | ~120ms | ~2200ms | 18x |

GPU advantage grows with vector count. Below ~5K vectors, CPU SIMD is faster due to GPU upload/readback overhead.

## NPU Backend

### What Is an NPU?

A Neural Processing Unit is dedicated silicon for neural network inference, separate from the CPU and GPU. Examples:

| Vendor | NPU | Found In |
|--------|-----|----------|
| Intel | AI Boost (Meteor Lake+) | Intel Core Ultra laptops |
| Apple | Neural Engine | M1/M2/M3/M4 Macs, iPhones |
| Qualcomm | Hexagon | Snapdragon X Elite laptops |
| AMD | XDNA / Ryzen AI | Ryzen 7040+ laptops |

### Engram's NPU Strategy

NPUs are designed for neural network inference, not general-purpose math. Engram's approach:

1. **Low-power compute via wgpu**: The integrated GPU (which shares die with the NPU on modern SoCs) is targeted as a low-power compute device for medium workloads.

2. **Platform-specific detection**: Engram detects dedicated NPU hardware by checking for platform-specific drivers:
   - Windows: `intel_vpu.sys` / `intel_npu.sys` (Intel), `qcdxkm.sys` (Qualcomm)
   - Linux: `/dev/accel/accel0` (Intel NPU via `intel_vpu` driver)
   - macOS: Apple Neural Engine (always present on Apple Silicon)

3. **Workload routing**: Medium workloads (10K-100K vectors) are routed to the NPU/integrated GPU, reserving the discrete GPU for large workloads and keeping the CPU free for graph operations.

### NPU Device Usage

```rust
use engram_compute::npu::{NpuDevice, NpuInfo};

// Detect dedicated NPU hardware
let npus = NpuInfo::detect();
for npu in &npus {
    println!("NPU: {} ({})", npu.name, npu.vendor);
}

// Get a low-power compute device
let npu = NpuDevice::probe().expect("no NPU device");
println!("Compute device: {} ({})", npu.name(), npu.backend());

// Run cosine distances on the low-power device
let distances = npu
    .batch_cosine_distances(&query, &vectors_flat, dim, count)
    .expect("NPU compute failed");
```

## Feature Flags

GPU and NPU compute are behind the `gpu` cargo feature (enabled by default):

```toml
[dependencies]
engram-compute = { version = "0.1", features = ["gpu"] }  # default
engram-compute = { version = "0.1", default-features = false }  # CPU-only, no wgpu
```

The CPU-only build has zero additional dependencies. The `gpu` feature adds `wgpu`, `pollster`, and `bytemuck`.

## Cross-Platform Matrix

| Feature | Windows (x86_64) | macOS (aarch64) | Linux (x86_64) | Linux (aarch64) |
|---------|:-:|:-:|:-:|:-:|
| CPU Scalar | x | x | x | x |
| AVX2+FMA | x | -- | x | -- |
| NEON | -- | x | -- | x |
| GPU (DX12) | x | -- | -- | -- |
| GPU (Vulkan) | x | -- | x | x |
| GPU (Metal) | -- | x | -- | -- |
| NPU Detection | x | x | x | x |
| NPU Compute | x | x | x | x |

All backends produce identical results. The WGSL compute shader is the same on every platform. CPU SIMD implementations are validated against the scalar reference in tests.

## Planner Decision Flow

```
similarity_search(query, vectors, limit)
    |
    v
count > 100K && GPU available?
    |-- yes --> GPU compute shader
    |           |-- success --> return ranked results
    |           |-- failure --> fall through
    |
count > 10K && NPU available?
    |-- yes --> NPU compute shader (low-power adapter)
    |           |-- success --> return ranked results
    |           |-- failure --> fall through
    |
CPU SIMD
    |-- AVX2+FMA (x86_64) or NEON (aarch64) or scalar
    |-- always succeeds
    v
return ranked results
```

## Module Reference

| Module | Description |
|--------|-------------|
| `engram_compute::simd` | CPU SIMD operations (AVX2, NEON, scalar) |
| `engram_compute::gpu` | GPU compute via wgpu (DX12/Vulkan/Metal) |
| `engram_compute::npu` | NPU detection and low-power compute |
| `engram_compute::planner` | Hardware detection and workload routing |
