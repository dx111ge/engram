/// NPU (Neural Processing Unit) detection and compute.
///
/// NPU-like devices are targeted via wgpu's low-power adapter selection:
///   - Intel integrated GPU (shares die with Intel NPU)
///   - Apple integrated GPU (shares die with Neural Engine)
///   - Qualcomm Adreno (shares die with Hexagon NPU)
///
/// Additionally, platform-specific detection identifies dedicated NPU hardware
/// for informational purposes (Intel AI Boost, Apple Neural Engine, etc.).

/// Information about detected NPU hardware.
#[derive(Debug, Clone)]
pub struct NpuInfo {
    /// NPU device name
    pub name: String,
    /// NPU type/vendor
    pub vendor: String,
    /// Whether compute can be dispatched to this device
    pub compute_available: bool,
}

impl NpuInfo {
    /// Detect NPU hardware on this system.
    pub fn detect() -> Vec<NpuInfo> {
        let mut npus = Vec::new();

        // Platform-specific NPU detection
        #[cfg(target_os = "windows")]
        {
            npus.extend(detect_windows_npu());
        }

        #[cfg(target_os = "macos")]
        {
            npus.push(NpuInfo {
                name: "Apple Neural Engine".to_string(),
                vendor: "Apple".to_string(),
                // ANE is accessible through CoreML, not directly via wgpu
                compute_available: false,
            });
        }

        #[cfg(target_os = "linux")]
        {
            npus.extend(detect_linux_npu());
        }

        npus
    }
}

/// Detect Intel/Qualcomm NPU on Windows via registry heuristics.
#[cfg(target_os = "windows")]
fn detect_windows_npu() -> Vec<NpuInfo> {
    let mut npus = Vec::new();

    // Check for Intel NPU driver (intel_vpu.sys or intel_npu.sys)
    let system_root =
        std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let driver_paths = [
        format!("{system_root}\\System32\\drivers\\intel_vpu.sys"),
        format!("{system_root}\\System32\\drivers\\intel_npu.sys"),
        format!("{system_root}\\System32\\drivers\\intelnpu.sys"),
    ];

    for path in &driver_paths {
        if std::path::Path::new(path).exists() {
            npus.push(NpuInfo {
                name: "Intel AI Boost NPU".to_string(),
                vendor: "Intel".to_string(),
                compute_available: false, // Direct NPU compute requires OpenVINO
            });
            break;
        }
    }

    // Check for Qualcomm NPU (qcnpu driver)
    let qc_paths = [
        format!("{system_root}\\System32\\drivers\\qcdxkm.sys"),
    ];

    for path in &qc_paths {
        if std::path::Path::new(path).exists() {
            npus.push(NpuInfo {
                name: "Qualcomm Hexagon NPU".to_string(),
                vendor: "Qualcomm".to_string(),
                compute_available: false,
            });
            break;
        }
    }

    npus
}

/// Detect NPU on Linux via /dev/accel* or sysfs.
#[cfg(target_os = "linux")]
fn detect_linux_npu() -> Vec<NpuInfo> {
    let mut npus = Vec::new();

    // Intel NPU exposes /dev/accel/accel0
    if std::path::Path::new("/dev/accel/accel0").exists()
        || std::path::Path::new("/dev/accel0").exists()
    {
        npus.push(NpuInfo {
            name: "Intel NPU (accel device)".to_string(),
            vendor: "Intel".to_string(),
            compute_available: false,
        });
    }

    // Check sysfs for NPU class devices
    if let Ok(entries) = std::fs::read_dir("/sys/class/accel") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            npus.push(NpuInfo {
                name: format!("NPU: {name}"),
                vendor: "Unknown".to_string(),
                compute_available: false,
            });
        }
    }

    npus
}

/// NPU-like compute device backed by wgpu (low-power adapter).
///
/// Uses the same compute shader infrastructure as the GPU module but
/// targets the system's low-power adapter (integrated GPU), which on
/// modern SoCs shares silicon with NPU-class hardware.
#[cfg(feature = "gpu")]
pub struct NpuDevice {
    inner: crate::gpu::GpuDevice,
    /// Detected NPU hardware info (may be empty)
    pub detected_npus: Vec<NpuInfo>,
}

#[cfg(feature = "gpu")]
impl NpuDevice {
    /// Probe for a low-power compute device suitable for NPU-like workloads.
    pub fn probe() -> Option<Self> {
        let inner = crate::gpu::GpuDevice::probe_low_power()?;
        let detected_npus = NpuInfo::detect();

        Some(NpuDevice {
            inner,
            detected_npus,
        })
    }

    /// Device name.
    pub fn name(&self) -> &str {
        &self.inner.name
    }

    /// Backend API (Vulkan, DX12, Metal).
    pub fn backend(&self) -> &str {
        &self.inner.backend
    }

    /// Compute cosine distances using the low-power device.
    pub fn batch_cosine_distances(
        &self,
        query: &[f32],
        vectors_flat: &[f32],
        dim: usize,
        count: usize,
    ) -> Option<Vec<f32>> {
        self.inner
            .batch_cosine_distances(query, vectors_flat, dim, count)
    }
}

#[cfg(not(feature = "gpu"))]
pub struct NpuDevice;

#[cfg(not(feature = "gpu"))]
impl NpuDevice {
    pub fn probe() -> Option<Self> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npu_detection() {
        let npus = NpuInfo::detect();
        if npus.is_empty() {
            println!("No dedicated NPU hardware detected");
        } else {
            for npu in &npus {
                println!("NPU: {} (vendor: {}, compute: {})", npu.name, npu.vendor, npu.compute_available);
            }
        }
    }

    #[test]
    #[cfg(feature = "gpu")]
    fn npu_device_probe() {
        let device = NpuDevice::probe();
        match &device {
            Some(d) => {
                println!("NPU compute device: {} ({})", d.name(), d.backend());
                for npu in &d.detected_npus {
                    println!("  Dedicated NPU: {} ({})", npu.name, npu.vendor);
                }
            }
            None => println!("No low-power compute device available"),
        }
    }

    #[test]
    #[cfg(feature = "gpu")]
    fn npu_cosine_distance() {
        let device = match NpuDevice::probe() {
            Some(d) => d,
            None => {
                println!("Skipping NPU test: no device available");
                return;
            }
        };

        let query = vec![1.0, 0.0, 0.0, 0.0];
        let vectors_flat = vec![
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
        ];

        let distances = device
            .batch_cosine_distances(&query, &vectors_flat, 4, 2)
            .expect("NPU compute failed");

        assert_eq!(distances.len(), 2);
        assert!(distances[0].abs() < 0.01, "identical: {}", distances[0]);
        assert!((distances[1] - 1.0).abs() < 0.01, "orthogonal: {}", distances[1]);
        println!("NPU cosine distances: {:?}", distances);
    }
}
