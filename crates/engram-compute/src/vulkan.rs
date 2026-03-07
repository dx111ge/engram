/// Vulkan compute shader infrastructure — device setup, shader management, memory.
///
/// This module provides the GPU compute path for:
///   - Parallel BFS traversal
///   - Batch cosine similarity
///   - Confidence propagation
///
/// Implementation status: stubs with defined interfaces.
/// Requires `ash` (raw Vulkan) or `vulkano` (safe wrapper) crate.
/// Shader source in GLSL is defined in DESIGN.md, compiled to SPIR-V.

/// GPU device handle and capabilities.
#[derive(Debug)]
pub struct VulkanDevice {
    /// Device name (e.g. "NVIDIA GeForce RTX 4090")
    pub name: String,
    /// VRAM in bytes
    pub vram_bytes: u64,
    /// Max compute workgroup size
    pub max_workgroup_size: u32,
    /// Whether the device is available and initialized
    pub available: bool,
}

impl VulkanDevice {
    /// Probe for a Vulkan-capable GPU. Returns None if not available.
    pub fn probe() -> Option<Self> {
        // TODO: Use ash/vulkano to enumerate physical devices
        // For now, return None (no GPU)
        None
    }
}

/// GPU memory buffer for data transfer between RAM and VRAM.
pub struct GpuBuffer {
    /// Size in bytes
    pub size: u64,
    /// Buffer type
    pub usage: BufferUsage,
}

/// How a GPU buffer is used.
#[derive(Debug, Clone, Copy)]
pub enum BufferUsage {
    /// Read-only data uploaded from host (nodes, edges, embeddings)
    Storage,
    /// Read-write working buffer (frontiers, scores)
    Scratch,
    /// Results read back to host
    Output,
}

/// A compiled Vulkan compute shader.
pub struct ComputeShader {
    pub name: String,
    pub workgroup_size: u32,
}

/// GPU compute kernel definitions.
pub enum Kernel {
    /// Parallel BFS traversal
    Traversal {
        max_depth: u32,
        min_confidence: f32,
    },
    /// Batch cosine similarity against all stored embeddings
    CosineSimilarity {
        embed_dim: u32,
    },
    /// Confidence propagation to neighbors
    ConfidencePropagation {
        damping: f32,
    },
}

/// VRAM budget tracker.
pub struct MemoryManager {
    /// Total VRAM available
    pub total_bytes: u64,
    /// Currently allocated
    pub used_bytes: u64,
}

impl MemoryManager {
    pub fn new(total_bytes: u64) -> Self {
        MemoryManager {
            total_bytes,
            used_bytes: 0,
        }
    }

    /// Check if there's enough VRAM for an allocation.
    pub fn can_allocate(&self, bytes: u64) -> bool {
        self.used_bytes + bytes <= self.total_bytes
    }

    /// Track an allocation.
    pub fn allocate(&mut self, bytes: u64) -> bool {
        if self.can_allocate(bytes) {
            self.used_bytes += bytes;
            true
        } else {
            false
        }
    }

    /// Free an allocation.
    pub fn free(&mut self, bytes: u64) {
        self.used_bytes = self.used_bytes.saturating_sub(bytes);
    }

    /// Available VRAM.
    pub fn available(&self) -> u64 {
        self.total_bytes.saturating_sub(self.used_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_vulkan_device() {
        // On most CI, no GPU
        let device = VulkanDevice::probe();
        // May or may not be Some, just ensure it doesn't crash
        if let Some(d) = &device {
            println!("Found GPU: {} ({} MB VRAM)", d.name, d.vram_bytes / 1_048_576);
        }
    }

    #[test]
    fn memory_manager_tracking() {
        let mut mm = MemoryManager::new(1024);
        assert!(mm.can_allocate(512));
        assert!(mm.allocate(512));
        assert_eq!(mm.available(), 512);
        assert!(!mm.can_allocate(600));
        mm.free(256);
        assert_eq!(mm.available(), 768);
    }

    #[test]
    fn memory_manager_overflow_protection() {
        let mut mm = MemoryManager::new(100);
        assert!(!mm.allocate(200));
        assert_eq!(mm.used_bytes, 0);
        mm.free(999); // saturating sub
        assert_eq!(mm.used_bytes, 0);
    }
}
