//! Compute shader pipeline — general-purpose GPU compute.
//!
//! Wraps `wgpu::ComputePipeline` with bind group layout management and
//! dispatch helpers. Replaces the ad-hoc compute setup in rasa-gpu and
//! soorat's compute module.

/// A compute pipeline wrapping `wgpu::ComputePipeline` with buffer management.
///
/// The default layout creates storage buffer bindings: buffer 0 is read-write
/// (output), buffers 1+ are read-only (inputs). Use [`ComputePipeline::with_layout`]
/// for custom layouts with uniform buffers or mixed access patterns.
pub struct ComputePipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl ComputePipeline {
    /// Create a compute pipeline from WGSL source code.
    ///
    /// `entry_point`: the compute shader entry function name.
    /// `buffer_count`: number of storage buffers in the bind group (bindings 0..n).
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compute_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

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

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compute_layout"),
            entries: &entries,
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("compute_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
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
            bind_group_layout,
        }
    }

    /// Create a compute pipeline with a custom bind group layout.
    ///
    /// Use this when you need uniform buffers, mixed read-write patterns,
    /// or texture bindings alongside storage buffers.
    pub fn with_layout(
        device: &wgpu::Device,
        wgsl_source: &str,
        entry_point: &str,
        entries: &[wgpu::BindGroupLayoutEntry],
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compute_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compute_layout_custom"),
            entries,
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("compute_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
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
            bind_group_layout,
        }
    }

    /// Get the bind group layout for creating bind groups.
    #[must_use]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Get the underlying wgpu compute pipeline.
    #[must_use]
    pub fn raw(&self) -> &wgpu::ComputePipeline {
        &self.pipeline
    }

    /// Dispatch the compute shader with explicit workgroup counts.
    ///
    /// Creates a command encoder, runs one compute pass, and submits.
    /// For batched dispatches (multiple passes per submission), use
    /// [`encode_dispatch`](Self::encode_dispatch) instead.
    pub fn dispatch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group: &wgpu::BindGroup,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("compute_encoder"),
        });

        self.encode_dispatch(
            &mut encoder,
            bind_group,
            workgroups_x,
            workgroups_y,
            workgroups_z,
        );

        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Encode a compute dispatch into an existing command encoder.
    ///
    /// Use this to batch multiple dispatches into a single submission,
    /// or to combine compute and copy operations.
    pub fn encode_dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("compute_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(workgroups_x, workgroups_y, workgroups_z);
    }
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
}
