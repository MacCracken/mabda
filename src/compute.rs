//! Compute shader pipeline — general-purpose GPU compute.
//!
//! Wraps `wgpu::ComputePipeline` with bind group layout management and
//! dispatch helpers. Supports single or multiple bind group layouts.

/// A compute pipeline wrapping `wgpu::ComputePipeline` with bind group management.
///
/// The default layout ([`new`](Self::new)) creates storage buffer bindings:
/// buffer 0 is read-write (output), buffers 1+ are read-only (inputs).
///
/// Use [`with_layout`](Self::with_layout) for a single custom bind group,
/// or [`with_layouts`](Self::with_layouts) for multiple bind groups
/// (e.g., storage buffers + uniform buffers + textures).
pub struct ComputePipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
}

impl ComputePipeline {
    /// Create a compute pipeline from WGSL source code.
    ///
    /// `entry_point`: the compute shader entry function name.
    /// `buffer_count`: number of storage buffers in bind group 0 (bindings 0..n).
    ///
    /// Buffer 0 is created as read-write (`read_only: false`) and buffers 1+
    /// are read-only. This matches the common pattern where a single output
    /// buffer is written by the shader while additional input buffers are
    /// consumed without modification.
    pub fn new(
        device: &wgpu::Device,
        wgsl_source: &str,
        entry_point: &str,
        buffer_count: u32,
    ) -> Self {
        let entries: Vec<wgpu::BindGroupLayoutEntry> = (0..buffer_count)
            .map(|i| wgpu::BindGroupLayoutEntry {
                binding: i,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: i > 0 },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();

        Self::with_layout(device, wgsl_source, entry_point, &entries)
    }

    /// Create a compute pipeline with a single custom bind group layout.
    ///
    /// Use this when you need uniform buffers, mixed read-write patterns,
    /// or texture bindings alongside storage buffers.
    pub fn with_layout(
        device: &wgpu::Device,
        wgsl_source: &str,
        entry_point: &str,
        entries: &[wgpu::BindGroupLayoutEntry],
    ) -> Self {
        Self::with_layouts(device, wgsl_source, entry_point, &[entries])
    }

    /// Create a compute pipeline with multiple bind group layouts.
    ///
    /// Each element in `groups` defines the entries for one bind group.
    /// Group 0 is the first, group 1 the second, etc.
    ///
    /// Use this when your shader needs separate bind groups for different
    /// resource types (e.g., group 0 for storage buffers, group 1 for
    /// uniform buffers, group 2 for textures).
    pub fn with_layouts(
        device: &wgpu::Device,
        wgsl_source: &str,
        entry_point: &str,
        groups: &[&[wgpu::BindGroupLayoutEntry]],
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compute_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

        let bind_group_layouts: Vec<wgpu::BindGroupLayout> = groups
            .iter()
            .enumerate()
            .map(|(i, entries)| {
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(&format!("compute_layout_{i}")),
                    entries,
                })
            })
            .collect();

        let layout_refs: Vec<Option<&wgpu::BindGroupLayout>> =
            bind_group_layouts.iter().map(Some).collect();

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("compute_pipeline_layout"),
            bind_group_layouts: &layout_refs,
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layouts,
        }
    }

    /// Get bind group layout by index.
    ///
    /// For pipelines created with [`new`](Self::new) or [`with_layout`](Self::with_layout),
    /// only index 0 is valid.
    #[must_use]
    #[inline]
    pub fn bind_group_layout(&self, index: usize) -> Option<&wgpu::BindGroupLayout> {
        self.bind_group_layouts.get(index)
    }

    /// Number of bind group layouts in this pipeline.
    #[must_use]
    #[inline]
    pub fn bind_group_layout_count(&self) -> usize {
        self.bind_group_layouts.len()
    }

    /// Get the underlying wgpu compute pipeline.
    #[must_use]
    #[inline]
    pub fn raw(&self) -> &wgpu::ComputePipeline {
        &self.pipeline
    }

    /// Dispatch the compute shader with a single bind group.
    ///
    /// Creates a command encoder, runs one compute pass, and submits.
    /// For batched dispatches, use [`encode_dispatch`](Self::encode_dispatch).
    pub fn dispatch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group: &wgpu::BindGroup,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) {
        self.dispatch_multi(
            device,
            queue,
            &[bind_group],
            workgroups_x,
            workgroups_y,
            workgroups_z,
        );
    }

    /// Dispatch the compute shader with multiple bind groups.
    ///
    /// Creates a command encoder, runs one compute pass, and submits.
    /// Each bind group is set at its corresponding index (0, 1, 2, ...).
    pub fn dispatch_multi(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_groups: &[&wgpu::BindGroup],
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) {
        tracing::debug!(
            workgroups_x,
            workgroups_y,
            workgroups_z,
            groups = bind_groups.len(),
            "compute dispatch"
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("compute_encoder"),
        });

        self.encode_dispatch_multi(
            &mut encoder,
            bind_groups,
            workgroups_x,
            workgroups_y,
            workgroups_z,
        );

        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Encode a compute dispatch with a single bind group into an existing encoder.
    pub fn encode_dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) {
        self.encode_dispatch_multi(
            encoder,
            &[bind_group],
            workgroups_x,
            workgroups_y,
            workgroups_z,
        );
    }

    /// Encode a compute dispatch with multiple bind groups into an existing encoder.
    pub fn encode_dispatch_multi(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_groups: &[&wgpu::BindGroup],
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("compute_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        for (i, bg) in bind_groups.iter().enumerate() {
            pass.set_bind_group(i as u32, *bg, &[]);
        }
        pass.dispatch_workgroups(workgroups_x, workgroups_y, workgroups_z);
    }

    /// Encode an indirect compute dispatch into an existing encoder.
    ///
    /// The `indirect_buffer` must contain a `DispatchIndirect` struct
    /// (3 × u32: workgroups_x, workgroups_y, workgroups_z) at `indirect_offset`.
    pub fn encode_dispatch_indirect(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_groups: &[&wgpu::BindGroup],
        indirect_buffer: &wgpu::Buffer,
        indirect_offset: u64,
    ) {
        tracing::debug!(indirect_offset, "compute indirect dispatch");
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("compute_pass_indirect"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        for (i, bg) in bind_groups.iter().enumerate() {
            pass.set_bind_group(i as u32, *bg, &[]);
        }
        pass.dispatch_workgroups_indirect(indirect_buffer, indirect_offset);
    }
}

/// A double-buffer pair for iterative compute patterns (ping-pong).
///
/// Common in FDTD simulation, iterative blur, fluid simulation, and any
/// algorithm that reads from one buffer and writes to another, then swaps.
pub struct PingPongBuffer {
    buffers: [wgpu::Buffer; 2],
    current: usize,
}

impl PingPongBuffer {
    /// Create a ping-pong buffer pair, each with `size` bytes.
    pub fn new(device: &wgpu::Device, size: u64, label: &str) -> Self {
        tracing::debug!(size, label, "creating ping-pong buffer pair");
        let buffers = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("{label}_a")),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("{label}_b")),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
        ];
        Self {
            buffers,
            current: 0,
        }
    }

    /// The buffer to read from (current source).
    #[must_use]
    #[inline]
    pub fn source(&self) -> &wgpu::Buffer {
        &self.buffers[self.current]
    }

    /// The buffer to write to (current destination).
    #[must_use]
    #[inline]
    pub fn dest(&self) -> &wgpu::Buffer {
        &self.buffers[1 - self.current]
    }

    /// Swap source and destination buffers.
    #[inline]
    pub fn swap(&mut self) {
        self.current = 1 - self.current;
    }

    /// Current iteration index (0 or 1).
    #[must_use]
    #[inline]
    pub fn index(&self) -> usize {
        self.current
    }
}

/// Validate workgroup counts against device limits.
///
/// Returns `Err(GpuError::WorkgroupLimitExceeded)` if any dimension exceeds
/// `max_compute_workgroups_per_dimension`.
pub fn validate_dispatch(
    limits: &wgpu::Limits,
    workgroups_x: u32,
    workgroups_y: u32,
    workgroups_z: u32,
) -> crate::error::Result<()> {
    use crate::error::GpuError;
    let max = limits.max_compute_workgroups_per_dimension;
    if workgroups_x > max {
        return Err(GpuError::WorkgroupLimitExceeded {
            axis: "x",
            actual: workgroups_x,
            limit: max,
        });
    }
    if workgroups_y > max {
        return Err(GpuError::WorkgroupLimitExceeded {
            axis: "y",
            actual: workgroups_y,
            limit: max,
        });
    }
    if workgroups_z > max {
        return Err(GpuError::WorkgroupLimitExceeded {
            axis: "z",
            actual: workgroups_z,
            limit: max,
        });
    }
    Ok(())
}

/// Calculate workgroup count for a 1D dispatch.
///
/// Returns `ceil(total / workgroup_size)`.
#[must_use]
#[inline]
pub fn workgroups_1d(total: u32, workgroup_size: u32) -> u32 {
    total.div_ceil(workgroup_size)
}

/// Calculate workgroup counts for a 2D dispatch.
///
/// Returns `(ceil(width / wg_x), ceil(height / wg_y))`.
#[must_use]
#[inline]
pub fn workgroups_2d(width: u32, height: u32, wg_x: u32, wg_y: u32) -> (u32, u32) {
    (width.div_ceil(wg_x), height.div_ceil(wg_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_pipeline_types() {
        let _size = std::mem::size_of::<ComputePipeline>();
    }

    #[test]
    fn workgroups_1d_exact() {
        assert_eq!(workgroups_1d(256, 256), 1);
        assert_eq!(workgroups_1d(512, 256), 2);
    }

    #[test]
    fn workgroups_1d_remainder() {
        assert_eq!(workgroups_1d(257, 256), 2);
        assert_eq!(workgroups_1d(1, 256), 1);
    }

    #[test]
    fn workgroups_2d_exact() {
        assert_eq!(workgroups_2d(32, 32, 16, 16), (2, 2));
    }

    #[test]
    fn workgroups_2d_remainder() {
        assert_eq!(workgroups_2d(33, 17, 16, 16), (3, 2));
    }

    #[test]
    fn workgroups_1d_single() {
        assert_eq!(workgroups_1d(1, 64), 1);
        assert_eq!(workgroups_1d(0, 64), 0);
    }

    #[test]
    fn workgroups_2d_single() {
        assert_eq!(workgroups_2d(1, 1, 8, 8), (1, 1));
        assert_eq!(workgroups_2d(0, 0, 8, 8), (0, 0));
    }

    #[test]
    fn validate_dispatch_within_limits() {
        let limits = wgpu::Limits {
            max_compute_workgroups_per_dimension: 65535,
            ..Default::default()
        };
        assert!(validate_dispatch(&limits, 100, 100, 1).is_ok());
        assert!(validate_dispatch(&limits, 65535, 65535, 65535).is_ok());
    }

    #[test]
    fn validate_dispatch_exceeds_limits() {
        let limits = wgpu::Limits {
            max_compute_workgroups_per_dimension: 65535,
            ..Default::default()
        };
        assert!(validate_dispatch(&limits, 65536, 1, 1).is_err());
        assert!(validate_dispatch(&limits, 1, 65536, 1).is_err());
        assert!(validate_dispatch(&limits, 1, 1, 65536).is_err());
    }

    #[test]
    fn validate_dispatch_error_contains_axis() {
        let limits = wgpu::Limits {
            max_compute_workgroups_per_dimension: 100,
            ..Default::default()
        };
        let err = validate_dispatch(&limits, 200, 1, 1).unwrap_err();
        assert!(err.to_string().contains("x"));
        let err = validate_dispatch(&limits, 1, 200, 1).unwrap_err();
        assert!(err.to_string().contains("y"));
    }

    #[test]
    fn workgroups_1d_large() {
        assert_eq!(workgroups_1d(1_000_000, 256), 3907);
        assert_eq!(workgroups_1d(u32::MAX, 256), 16_777_216);
    }

    #[test]
    fn ping_pong_swap() {
        // Verify swap logic without GPU
        let mut current = 0usize;
        assert_eq!(current, 0);
        assert_eq!(1 - current, 1);
        current = 1 - current;
        assert_eq!(current, 1);
        assert_eq!(1 - current, 0);
        current = 1 - current;
        assert_eq!(current, 0);
    }

    #[test]
    fn ping_pong_types() {
        let _size = std::mem::size_of::<PingPongBuffer>();
    }
}
