//! GPU capability reporting — feature detection and limit queries.
//!
//! [`GpuCapabilities`] provides a snapshot of what the current GPU supports.
//! Consumers use this to select code paths (CPU fallback vs GPU compute),
//! validate resource sizes, and report hardware info.

use serde::{Deserialize, Serialize};

/// GPU capabilities report — what the current device supports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuCapabilities {
    /// Adapter name (e.g., "NVIDIA GeForce RTX 4090").
    pub adapter_name: String,
    /// Backend (Vulkan, Metal, DX12, etc.).
    pub backend: String,
    /// Whether timestamp queries are supported (for GPU profiling).
    pub timestamp_query: bool,
    /// Whether compute shaders are supported (always true for wgpu).
    pub compute_shaders: bool,
    /// Maximum texture dimension (1D/2D).
    pub max_texture_dimension_2d: u32,
    /// Maximum uniform buffer binding size.
    pub max_uniform_buffer_size: u32,
    /// Maximum storage buffer binding size.
    pub max_storage_buffer_size: u32,
    /// Maximum buffer size in bytes.
    pub max_buffer_size: u64,
    /// Maximum number of bind groups.
    pub max_bind_groups: u32,
    /// Maximum vertex buffers per pipeline.
    pub max_vertex_buffers: u32,
    /// Maximum compute workgroup size per dimension.
    pub max_compute_workgroup_size: [u32; 3],
    /// Maximum compute workgroups per dispatch dimension.
    pub max_compute_workgroups_per_dimension: u32,
    /// Whether the device supports multi-draw indirect.
    pub multi_draw_indirect: bool,
}

impl GpuCapabilities {
    /// Query capabilities from a [`GpuContext`](crate::context::GpuContext).
    pub fn from_context(ctx: &crate::context::GpuContext) -> Self {
        tracing::debug!("querying GPU capabilities");
        let info = ctx.adapter.get_info();
        let features = ctx.device.features();
        let limits = ctx.device.limits();

        Self {
            adapter_name: info.name.clone(),
            backend: match info.backend {
                wgpu::Backend::Vulkan => "Vulkan",
                wgpu::Backend::Metal => "Metal",
                wgpu::Backend::Dx12 => "DX12",
                wgpu::Backend::Gl => "GL",
                wgpu::Backend::BrowserWebGpu => "WebGPU",
                _ => "Unknown",
            }
            .to_string(),
            timestamp_query: features.contains(wgpu::Features::TIMESTAMP_QUERY),
            compute_shaders: true,
            max_texture_dimension_2d: limits.max_texture_dimension_2d,
            max_uniform_buffer_size: limits.max_uniform_buffer_binding_size as u32,
            max_storage_buffer_size: limits.max_storage_buffer_binding_size as u32,
            max_buffer_size: limits.max_buffer_size,
            max_bind_groups: limits.max_bind_groups,
            max_vertex_buffers: limits.max_vertex_buffers,
            max_compute_workgroup_size: [
                limits.max_compute_workgroup_size_x,
                limits.max_compute_workgroup_size_y,
                limits.max_compute_workgroup_size_z,
            ],
            max_compute_workgroups_per_dimension: limits.max_compute_workgroups_per_dimension,
            multi_draw_indirect: features.contains(wgpu::Features::MULTI_DRAW_INDIRECT_COUNT),
        }
    }

    /// Check if a uniform buffer size fits within device limits.
    #[must_use]
    #[inline]
    pub fn uniform_fits(&self, size: u32) -> bool {
        size <= self.max_uniform_buffer_size
    }

    /// Check if a storage buffer size fits within device limits.
    #[must_use]
    #[inline]
    pub fn storage_fits(&self, size: u64) -> bool {
        size <= self.max_storage_buffer_size as u64
    }

    /// Format as a human-readable report.
    #[must_use]
    pub fn report(&self) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(512);
        let _ = write!(out, "GPU: {} ({})", self.adapter_name, self.backend);
        let _ = write!(out, "\nTimestamp queries: {}", self.timestamp_query);
        let _ = write!(out, "\nMax texture 2D: {}", self.max_texture_dimension_2d);
        let _ = write!(
            out,
            "\nMax uniform buffer: {} bytes",
            self.max_uniform_buffer_size
        );
        let _ = write!(
            out,
            "\nMax storage buffer: {} bytes",
            self.max_storage_buffer_size
        );
        let _ = write!(out, "\nMax buffer size: {} bytes", self.max_buffer_size);
        let _ = write!(out, "\nMax bind groups: {}", self.max_bind_groups);
        let _ = write!(out, "\nMax vertex buffers: {}", self.max_vertex_buffers);
        let _ = write!(
            out,
            "\nMax compute workgroup: {:?}",
            self.max_compute_workgroup_size
        );
        let _ = write!(
            out,
            "\nMax compute workgroups/dim: {}",
            self.max_compute_workgroups_per_dimension
        );
        let _ = write!(out, "\nMulti-draw indirect: {}", self.multi_draw_indirect);
        out
    }
}

/// WebGPU compatibility limits (compile-time constants for validation).
pub mod webgpu {
    /// WebGPU maximum uniform buffer size (64KB).
    pub const MAX_UNIFORM_BUFFER: u32 = 65536;

    /// WebGPU maximum storage buffer size (128MB typical).
    pub const MAX_STORAGE_BUFFER: u32 = 134_217_728;

    /// WebGPU maximum bind groups (4).
    pub const MAX_BIND_GROUPS: u32 = 4;

    /// WebGPU maximum texture dimension (8192 typical).
    pub const MAX_TEXTURE_2D: u32 = 8192;

    /// Check if a uniform buffer size is WebGPU-compatible.
    #[must_use]
    pub fn uniform_fits(size: u32) -> bool {
        size <= MAX_UNIFORM_BUFFER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webgpu_uniform_fits() {
        assert!(webgpu::uniform_fits(1024));
        assert!(webgpu::uniform_fits(65536));
        assert!(!webgpu::uniform_fits(65537));
    }

    #[test]
    fn webgpu_constants() {
        assert_eq!(webgpu::MAX_BIND_GROUPS, 4);
        assert_eq!(webgpu::MAX_UNIFORM_BUFFER, 65536);
    }

    #[test]
    fn gpu_capabilities_types() {
        let _size = std::mem::size_of::<GpuCapabilities>();
    }

    #[test]
    fn capabilities_report_format() {
        let caps = GpuCapabilities {
            adapter_name: "Test GPU".into(),
            backend: "Vulkan".into(),
            timestamp_query: true,
            compute_shaders: true,
            max_texture_dimension_2d: 16384,
            max_uniform_buffer_size: 65536,
            max_storage_buffer_size: 134_217_728,
            max_buffer_size: 268_435_456,
            max_bind_groups: 4,
            max_vertex_buffers: 8,
            max_compute_workgroup_size: [256, 256, 64],
            max_compute_workgroups_per_dimension: 65535,
            multi_draw_indirect: false,
        };
        let report = caps.report();
        assert!(report.contains("Test GPU"));
        assert!(report.contains("Vulkan"));
        assert!(report.contains("16384"));
        assert!(report.contains("65536"));
    }

    #[test]
    fn capabilities_uniform_storage_fits() {
        let caps = GpuCapabilities {
            adapter_name: "Test".into(),
            backend: "Vulkan".into(),
            timestamp_query: false,
            compute_shaders: true,
            max_texture_dimension_2d: 8192,
            max_uniform_buffer_size: 65536,
            max_storage_buffer_size: 134_217_728,
            max_buffer_size: 268_435_456,
            max_bind_groups: 4,
            max_vertex_buffers: 8,
            max_compute_workgroup_size: [256, 256, 64],
            max_compute_workgroups_per_dimension: 65535,
            multi_draw_indirect: false,
        };
        assert!(caps.uniform_fits(1024));
        assert!(caps.uniform_fits(65536));
        assert!(!caps.uniform_fits(65537));
        assert!(caps.storage_fits(1024));
        assert!(!caps.storage_fits(134_217_729));
    }

    #[test]
    fn capabilities_serde_roundtrip() {
        let caps = GpuCapabilities {
            adapter_name: "Test GPU".into(),
            backend: "Vulkan".into(),
            timestamp_query: true,
            compute_shaders: true,
            max_texture_dimension_2d: 16384,
            max_uniform_buffer_size: 65536,
            max_storage_buffer_size: 134_217_728,
            max_buffer_size: 268_435_456,
            max_bind_groups: 4,
            max_vertex_buffers: 8,
            max_compute_workgroup_size: [256, 256, 64],
            max_compute_workgroups_per_dimension: 65535,
            multi_draw_indirect: false,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: GpuCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps.adapter_name, decoded.adapter_name);
        assert_eq!(caps.max_buffer_size, decoded.max_buffer_size);
    }
}
