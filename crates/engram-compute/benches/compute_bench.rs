/// Benchmarks for CPU SIMD, GPU, and NPU compute backends.
///
/// Runs batch cosine distance at multiple scales and dimensions,
/// printing timing and throughput for each backend.

use std::time::Instant;

fn random_vectors(count: usize, dim: usize) -> Vec<Vec<f32>> {
    // Simple LCG to avoid external deps
    let mut seed: u64 = 42;
    (0..count)
        .map(|_| {
            (0..dim)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    ((seed >> 33) as f32) / (u32::MAX as f32) * 2.0 - 1.0
                })
                .collect()
        })
        .collect()
}

fn bench_cpu_simd(query: &[f32], vectors: &[&[f32]], limit: usize) -> (Vec<(usize, f32)>, std::time::Duration) {
    let start = Instant::now();
    let result = engram_compute::simd::batch_cosine_distances(query, vectors, limit);
    let elapsed = start.elapsed();
    (result, elapsed)
}

fn bench_gpu(
    gpu: &engram_compute::gpu::GpuDevice,
    query: &[f32],
    flat: &[f32],
    dim: usize,
    count: usize,
) -> (Option<Vec<f32>>, std::time::Duration) {
    let start = Instant::now();
    let result = gpu.batch_cosine_distances(query, flat, dim, count);
    let elapsed = start.elapsed();
    (result, elapsed)
}

fn bench_npu(
    npu: &engram_compute::npu::NpuDevice,
    query: &[f32],
    flat: &[f32],
    dim: usize,
    count: usize,
) -> (Option<Vec<f32>>, std::time::Duration) {
    let start = Instant::now();
    let result = npu.batch_cosine_distances(query, flat, dim, count);
    let elapsed = start.elapsed();
    (result, elapsed)
}

fn main() {
    println!("=== engram compute backend benchmark ===\n");

    // Detect hardware
    let hw = engram_compute::planner::HardwareInfo::detect();
    println!("Hardware:");
    println!("  CPU: {} cores, AVX2={}, NEON={}", hw.cpu_cores, hw.has_avx2, hw.has_neon);
    println!("  GPU: {} ({}) available={}", hw.gpu_name, hw.gpu_backend, hw.has_gpu);
    println!("  NPU: {} available={}", hw.npu_name, hw.has_npu);
    println!();

    // Probe devices
    let gpu = engram_compute::gpu::GpuDevice::probe();
    let npu = engram_compute::npu::NpuDevice::probe();

    let dimensions = [384, 768, 1536];
    let counts = [1_000, 10_000, 50_000, 100_000, 250_000];
    let limit = 10;
    let warmup_runs = 2;
    let bench_runs = 5;

    println!(
        "{:<8} {:<8} {:<15} {:<15} {:<15} {:<15}",
        "Vectors", "Dim", "CPU SIMD", "GPU", "NPU", "Speedup (GPU)"
    );
    println!("{}", "-".repeat(80));

    for &dim in &dimensions {
        for &count in &counts {
            let vecs = random_vectors(count, dim);
            let query = &vecs[0];
            let vec_refs: Vec<&[f32]> = vecs.iter().map(|v| v.as_slice()).collect();
            let flat: Vec<f32> = vecs.iter().flat_map(|v| v.iter().copied()).collect();

            // --- CPU SIMD ---
            // Warmup
            for _ in 0..warmup_runs {
                let _ = bench_cpu_simd(query, &vec_refs, limit);
            }
            let mut cpu_times = Vec::new();
            for _ in 0..bench_runs {
                let (_, elapsed) = bench_cpu_simd(query, &vec_refs, limit);
                cpu_times.push(elapsed);
            }
            cpu_times.sort();
            let cpu_median = cpu_times[bench_runs / 2];

            // --- GPU (chunked dispatch handles large buffers automatically) ---
            let gpu_median = if let Some(ref g) = gpu {
                for _ in 0..warmup_runs {
                    let _ = bench_gpu(g, query, &flat, dim, count);
                }
                let mut times = Vec::new();
                for _ in 0..bench_runs {
                    let (_, elapsed) = bench_gpu(g, query, &flat, dim, count);
                    times.push(elapsed);
                }
                times.sort();
                Some(times[bench_runs / 2])
            } else {
                None
            };

            // --- NPU (delegates to GPU with chunking) ---
            let npu_median = if let Some(ref n) = npu {
                for _ in 0..warmup_runs {
                    let _ = bench_npu(n, query, &flat, dim, count);
                }
                let mut times = Vec::new();
                for _ in 0..bench_runs {
                    let (_, elapsed) = bench_npu(n, query, &flat, dim, count);
                    times.push(elapsed);
                }
                times.sort();
                Some(times[bench_runs / 2])
            } else {
                None
            };

            let gpu_str = match gpu_median {
                Some(d) => format!("{:.2?}", d),
                None => "N/A".to_string(),
            };
            let npu_str = match npu_median {
                Some(d) => format!("{:.2?}", d),
                None => "N/A".to_string(),
            };
            let speedup = gpu_median
                .map(|g| format!("{:.1}x", cpu_median.as_secs_f64() / g.as_secs_f64()))
                .unwrap_or_else(|| "N/A".to_string());

            println!(
                "{:<8} {:<8} {:<15} {:<15} {:<15} {:<15}",
                count,
                dim,
                format!("{:.2?}", cpu_median),
                gpu_str,
                npu_str,
                speedup,
            );
        }
        println!();
    }

    // Verify correctness: GPU and NPU should produce same top-k as CPU
    if gpu.is_some() || npu.is_some() {
        println!("=== Correctness check (384-dim, 1K vectors) ===");
        let vecs = random_vectors(1000, 384);
        let query = &vecs[0];
        let vec_refs: Vec<&[f32]> = vecs.iter().map(|v| v.as_slice()).collect();
        let flat: Vec<f32> = vecs.iter().flat_map(|v| v.iter().copied()).collect();

        let (cpu_result, _) = bench_cpu_simd(query, &vec_refs, 5);
        println!("CPU top-5: {:?}", cpu_result.iter().map(|(i, d)| (*i, format!("{:.4}", d))).collect::<Vec<_>>());

        if let Some(ref g) = gpu {
            let (Some(dists), _) = bench_gpu(g, query, &flat, 384, 1000) else {
                println!("GPU: failed");
                return;
            };
            let mut ranked: Vec<(usize, f32)> = dists.into_iter().enumerate().collect();
            ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            ranked.truncate(5);
            println!("GPU top-5: {:?}", ranked.iter().map(|(i, d)| (*i, format!("{:.4}", d))).collect::<Vec<_>>());
        }

        if let Some(ref n) = npu {
            let (Some(dists), _) = bench_npu(n, query, &flat, 384, 1000) else {
                println!("NPU: failed");
                return;
            };
            let mut ranked: Vec<(usize, f32)> = dists.into_iter().enumerate().collect();
            ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            ranked.truncate(5);
            println!("NPU top-5: {:?}", ranked.iter().map(|(i, d)| (*i, format!("{:.4}", d))).collect::<Vec<_>>());
        }
    }
}
