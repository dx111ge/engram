/// GPU compute via wgpu — real compute shaders for batch similarity search.
///
/// Uses WGSL compute shaders dispatched through wgpu, which auto-selects
/// the best available graphics API:
///   - DX12 on Windows
///   - Metal on macOS
///   - Vulkan on Linux (and Windows with Vulkan SDK)
///
/// The same shader code works on NVIDIA, AMD, Intel, and Apple GPUs.

#[cfg(feature = "gpu")]
mod inner {
    use std::sync::mpsc;

    const COSINE_SHADER: &str = r#"
struct Params {
    dim: u32,
    count: u32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<storage, read> query: array<f32>;
@group(0) @binding(1) var<storage, read> vectors: array<f32>;
@group(0) @binding(2) var<storage, read_write> distances: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= params.count) {
        return;
    }

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
    if (denom < 0.0000001) {
        distances[idx] = 1.0;
    } else {
        distances[idx] = 1.0 - dot / denom;
    }
}
"#;

    #[repr(C)]
    #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
    struct GpuParams {
        dim: u32,
        count: u32,
        _pad: [u32; 2],
    }

    /// A GPU compute device backed by wgpu.
    pub struct GpuDevice {
        /// GPU device name (e.g. "NVIDIA GeForce RTX 4090")
        pub name: String,
        /// Graphics API backend (e.g. "Vulkan", "Dx12", "Metal")
        pub backend: String,
        /// Device type as reported by wgpu
        pub device_type: String,
        /// Estimated VRAM in bytes (0 if unknown)
        pub vram_bytes: u64,

        device: wgpu::Device,
        queue: wgpu::Queue,
        pipeline: wgpu::ComputePipeline,
    }

    impl GpuDevice {
        /// Probe for a high-performance GPU. Returns None if unavailable.
        pub fn probe() -> Option<Self> {
            Self::try_new(wgpu::PowerPreference::HighPerformance)
        }

        /// Probe for a low-power compute device (integrated GPU / NPU-like).
        pub fn probe_low_power() -> Option<Self> {
            Self::try_new(wgpu::PowerPreference::LowPower)
        }

        fn try_new(power_preference: wgpu::PowerPreference) -> Option<Self> {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });

            let adapter = pollster::block_on(instance.request_adapter(
                &wgpu::RequestAdapterOptions {
                    power_preference,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                },
            ))?;

            let info = adapter.get_info();
            let name = info.name.clone();
            let backend = format!("{:?}", info.backend);
            let device_type = format!("{:?}", info.device_type);

            let (device, queue) = pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("engram compute"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            ))
            .ok()?;

            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("cosine_distance"),
                source: wgpu::ShaderSource::Wgsl(COSINE_SHADER.into()),
            });

            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("cosine_pipeline"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

            Some(GpuDevice {
                name,
                backend,
                device_type,
                vram_bytes: 0, // wgpu doesn't expose VRAM size directly
                device,
                queue,
                pipeline,
            })
        }

        /// Max vectors buffer size per dispatch (leave headroom below 128MB limit).
        const MAX_VECTORS_BUFFER_BYTES: usize = 120 * 1024 * 1024;

        /// Compute cosine distances from `query` to each vector on the GPU.
        ///
        /// `vectors_flat` is a contiguous array of N vectors, each of dimension `dim`.
        /// Returns a Vec of distances (1.0 - cosine_similarity), or None on failure.
        ///
        /// For large inputs that exceed the GPU buffer size limit, automatically
        /// splits into chunks and dispatches multiple times.
        pub fn batch_cosine_distances(
            &self,
            query: &[f32],
            vectors_flat: &[f32],
            dim: usize,
            count: usize,
        ) -> Option<Vec<f32>> {
            if count == 0 || dim == 0 {
                return Some(Vec::new());
            }

            let bytes_per_vector = dim * std::mem::size_of::<f32>();
            let total_bytes = count * bytes_per_vector;

            if total_bytes <= Self::MAX_VECTORS_BUFFER_BYTES {
                // Single dispatch — fits in one buffer
                return self.dispatch_chunk(query, vectors_flat, dim, count);
            }

            // Chunked dispatch — split into chunks that fit within buffer limits
            let max_vectors_per_chunk = Self::MAX_VECTORS_BUFFER_BYTES / bytes_per_vector;
            let mut all_distances = Vec::with_capacity(count);
            let mut offset = 0;

            while offset < count {
                let chunk_count = (count - offset).min(max_vectors_per_chunk);
                let chunk_start = offset * dim;
                let chunk_end = chunk_start + chunk_count * dim;
                let chunk_flat = &vectors_flat[chunk_start..chunk_end];

                let chunk_distances = self.dispatch_chunk(query, chunk_flat, dim, chunk_count)?;
                all_distances.extend(chunk_distances);
                offset += chunk_count;
            }

            Some(all_distances)
        }

        /// Dispatch a single GPU compute pass for a chunk of vectors.
        fn dispatch_chunk(
            &self,
            query: &[f32],
            vectors_flat: &[f32],
            dim: usize,
            count: usize,
        ) -> Option<Vec<f32>> {
            let params = GpuParams {
                dim: dim as u32,
                count: count as u32,
                _pad: [0; 2],
            };

            use wgpu::util::DeviceExt;

            // Create buffers
            let query_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("query"),
                contents: bytemuck::cast_slice(query),
                usage: wgpu::BufferUsages::STORAGE,
            });

            let vectors_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vectors"),
                contents: bytemuck::cast_slice(vectors_flat),
                usage: wgpu::BufferUsages::STORAGE,
            });

            let result_size = (count * std::mem::size_of::<f32>()) as u64;
            let result_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("distances"),
                size: result_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            let params_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });

            let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("staging"),
                size: result_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Create bind group
            let bind_group_layout = self.pipeline.get_bind_group_layout(0);
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: query_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: vectors_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: result_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: params_buf.as_entire_binding(),
                    },
                ],
            });

            // Dispatch compute
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cosine_encoder"),
                });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("cosine_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups(((count as u32) + 63) / 64, 1, 1);
            }

            // Copy results to staging buffer for readback
            encoder.copy_buffer_to_buffer(&result_buf, 0, &staging_buf, 0, result_size);
            self.queue.submit(std::iter::once(encoder.finish()));

            // Read back results
            let buffer_slice = staging_buf.slice(..);
            let (tx, rx) = mpsc::sync_channel(1);
            buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            self.device.poll(wgpu::Maintain::Wait);

            if rx.recv().ok()?.is_err() {
                return None;
            }

            let data = buffer_slice.get_mapped_range();
            let distances: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
            drop(data);
            staging_buf.unmap();

            Some(distances)
        }

        /// Enumerate all available GPU adapters with their info.
        pub fn enumerate_adapters() -> Vec<AdapterInfo> {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });

            instance
                .enumerate_adapters(wgpu::Backends::all())
                .into_iter()
                .map(|adapter| {
                    let info = adapter.get_info();
                    AdapterInfo {
                        name: info.name,
                        backend: format!("{:?}", info.backend),
                        device_type: format!("{:?}", info.device_type),
                        driver: info.driver.clone(),
                    }
                })
                .collect()
        }
    }

    /// Info about a discovered GPU adapter.
    #[derive(Debug, Clone)]
    pub struct AdapterInfo {
        pub name: String,
        pub backend: String,
        pub device_type: String,
        pub driver: String,
    }

    impl std::fmt::Display for AdapterInfo {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{} ({}, {}, {})",
                self.name, self.backend, self.device_type, self.driver
            )
        }
    }
}

#[cfg(feature = "gpu")]
pub use inner::*;

// Stub types when gpu feature is disabled
#[cfg(not(feature = "gpu"))]
pub struct GpuDevice;

#[cfg(not(feature = "gpu"))]
impl GpuDevice {
    pub fn probe() -> Option<Self> {
        None
    }
    pub fn probe_low_power() -> Option<Self> {
        None
    }
}

#[cfg(test)]
#[cfg(feature = "gpu")]
mod tests {
    use super::*;

    #[test]
    fn enumerate_gpu_adapters() {
        let adapters = GpuDevice::enumerate_adapters();
        for (i, adapter) in adapters.iter().enumerate() {
            println!("Adapter {i}: {adapter}");
        }
    }

    #[test]
    fn gpu_probe_does_not_crash() {
        // May return Some or None depending on hardware
        let device = GpuDevice::probe();
        if let Some(d) = &device {
            println!("GPU: {} ({}, {})", d.name, d.backend, d.device_type);
        } else {
            println!("No high-performance GPU found");
        }
    }

    #[test]
    fn gpu_cosine_distance_correctness() {
        let device = match GpuDevice::probe() {
            Some(d) => d,
            None => {
                println!("Skipping GPU test: no GPU available");
                return;
            }
        };

        let query = vec![1.0, 0.0, 0.0, 0.0];
        let vectors_flat = vec![
            1.0, 0.0, 0.0, 0.0, // identical -> distance 0
            0.0, 1.0, 0.0, 0.0, // orthogonal -> distance 1
            -1.0, 0.0, 0.0, 0.0, // opposite -> distance 2
            0.7, 0.7, 0.0, 0.0, // similar -> distance ~0.29
        ];
        let dim = 4;
        let count = 4;

        let distances = device
            .batch_cosine_distances(&query, &vectors_flat, dim, count)
            .expect("GPU compute failed");

        assert_eq!(distances.len(), 4);
        assert!(distances[0].abs() < 0.01, "identical: {}", distances[0]);
        assert!(
            (distances[1] - 1.0).abs() < 0.01,
            "orthogonal: {}",
            distances[1]
        );
        assert!(
            (distances[2] - 2.0).abs() < 0.01,
            "opposite: {}",
            distances[2]
        );
        assert!(
            distances[3] > 0.2 && distances[3] < 0.4,
            "similar: {}",
            distances[3]
        );
        println!("GPU cosine distances: {:?}", distances);
    }

    #[test]
    fn gpu_large_batch() {
        let device = match GpuDevice::probe() {
            Some(d) => d,
            None => {
                println!("Skipping GPU test: no GPU available");
                return;
            }
        };

        let dim = 384;
        let count = 10_000;
        let query: Vec<f32> = (0..dim).map(|i| ((i * 3) as f32).sin()).collect();
        let vectors_flat: Vec<f32> = (0..count * dim)
            .map(|i| ((i * 7 + 1) as f32).sin())
            .collect();

        let start = std::time::Instant::now();
        let distances = device
            .batch_cosine_distances(&query, &vectors_flat, dim, count)
            .expect("GPU compute failed");
        let elapsed = start.elapsed();

        assert_eq!(distances.len(), count);
        assert!(distances.iter().all(|d| *d >= 0.0 && *d <= 2.0));
        println!(
            "GPU: {}x{}-dim cosine distances in {}ms",
            count,
            dim,
            elapsed.as_millis()
        );
    }
}
