//! Render pipeline abstraction.
//!
//! [`RenderPipeline`] wraps `wgpu::RenderPipeline` with bind group layout
//! management. Use [`RenderPipelineBuilder`] to construct pipelines with
//! sensible defaults.

use crate::error::Result;

/// A render pipeline wrapping `wgpu::RenderPipeline` with bind group layout(s).
///
/// Analogous to [`ComputePipeline`](crate::compute::ComputePipeline) but for
/// vertex/fragment shader stages. Supports multiple bind group layouts for
/// complex rendering setups (uniforms, textures, shadow maps, etc.).
pub struct RenderPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
}

impl RenderPipeline {
    /// Get the underlying wgpu render pipeline.
    #[must_use]
    #[inline]
    pub fn raw(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    /// Get bind group layout by index.
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

    /// One-shot draw: creates encoder, begins render pass, draws, submits.
    ///
    /// For batched draws (multiple passes per submission), use
    /// [`encode_draw`](Self::encode_draw) instead.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_view: &wgpu::TextureView,
        bind_groups: &[&wgpu::BindGroup],
        vertex_buffers: &[&wgpu::Buffer],
        index_buffer: Option<(&wgpu::Buffer, wgpu::IndexFormat)>,
        draw_command: DrawCommand,
        clear_color: Option<crate::color::Color>,
    ) {
        tracing::debug!("render pipeline draw");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render_encoder"),
        });

        self.encode_draw(
            &mut encoder,
            color_view,
            None,
            bind_groups,
            vertex_buffers,
            index_buffer,
            draw_command,
            clear_color,
        );

        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Encode a draw into an existing command encoder.
    ///
    /// Use this to batch multiple draws into a single submission,
    /// or to combine render and compute operations.
    #[allow(clippy::too_many_arguments)]
    pub fn encode_draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: Option<&wgpu::TextureView>,
        bind_groups: &[&wgpu::BindGroup],
        vertex_buffers: &[&wgpu::Buffer],
        index_buffer: Option<(&wgpu::Buffer, wgpu::IndexFormat)>,
        draw_command: DrawCommand,
        clear_color: Option<crate::color::Color>,
    ) {
        let color_load = match clear_color {
            Some(c) => wgpu::LoadOp::Clear(c.to_wgpu()),
            None => wgpu::LoadOp::Load,
        };

        let depth_stencil_attachment =
            depth_view.map(|view| wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);

        for (i, bg) in bind_groups.iter().enumerate() {
            pass.set_bind_group(i as u32, *bg, &[]);
        }

        for (i, buf) in vertex_buffers.iter().enumerate() {
            pass.set_vertex_buffer(i as u32, buf.slice(..));
        }

        if let Some((buf, format)) = index_buffer {
            pass.set_index_buffer(buf.slice(..), format);
        }

        match draw_command {
            DrawCommand::Draw {
                vertex_count,
                instance_count,
            } => {
                pass.draw(0..vertex_count, 0..instance_count);
            }
            DrawCommand::DrawIndexed {
                index_count,
                instance_count,
                first_index,
                base_vertex,
                first_instance,
            } => {
                pass.draw_indexed(
                    first_index..first_index + index_count,
                    base_vertex,
                    first_instance..first_instance + instance_count,
                );
            }
        }
    }
}

/// What to draw.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum DrawCommand {
    /// Non-indexed draw.
    Draw {
        vertex_count: u32,
        instance_count: u32,
    },
    /// Indexed draw.
    DrawIndexed {
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    },
}

/// Builder for constructing a [`RenderPipeline`].
///
/// Provides sensible defaults: `TriangleList` topology, `Ccw` front face,
/// no cull mode, no depth/stencil, single-sample. Override only what you need.
pub struct RenderPipelineBuilder<'a> {
    device: &'a wgpu::Device,
    label: Option<&'a str>,
    wgsl_source: &'a str,
    vertex_entry: &'a str,
    fragment_entry: &'a str,
    vertex_layouts: Vec<wgpu::VertexBufferLayout<'a>>,
    bind_group_layout_entries: Vec<Vec<wgpu::BindGroupLayoutEntry>>,
    color_targets: Vec<Option<wgpu::ColorTargetState>>,
    depth_stencil: Option<wgpu::DepthStencilState>,
    primitive: wgpu::PrimitiveState,
    multisample: wgpu::MultisampleState,
}

impl<'a> RenderPipelineBuilder<'a> {
    /// Start building a render pipeline from WGSL source.
    pub fn new(
        device: &'a wgpu::Device,
        wgsl_source: &'a str,
        vertex_entry: &'a str,
        fragment_entry: &'a str,
    ) -> Self {
        Self {
            device,
            label: None,
            wgsl_source,
            vertex_entry,
            fragment_entry,
            vertex_layouts: Vec::new(),
            bind_group_layout_entries: Vec::new(),
            color_targets: Vec::new(),
            depth_stencil: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            multisample: wgpu::MultisampleState::default(),
        }
    }

    /// Set a debug label.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Add a vertex buffer layout.
    pub fn vertex_layout(mut self, layout: wgpu::VertexBufferLayout<'a>) -> Self {
        self.vertex_layouts.push(layout);
        self
    }

    /// Add a bind group with the given layout entries.
    pub fn bind_group(mut self, entries: Vec<wgpu::BindGroupLayoutEntry>) -> Self {
        self.bind_group_layout_entries.push(entries);
        self
    }

    /// Add a color target with optional blending.
    pub fn color_target(
        mut self,
        format: wgpu::TextureFormat,
        blend: Option<wgpu::BlendState>,
    ) -> Self {
        self.color_targets.push(Some(wgpu::ColorTargetState {
            format,
            blend,
            write_mask: wgpu::ColorWrites::ALL,
        }));
        self
    }

    /// Set depth/stencil state.
    pub fn depth_stencil(mut self, state: wgpu::DepthStencilState) -> Self {
        self.depth_stencil = Some(state);
        self
    }

    /// Set primitive topology (default: `TriangleList`).
    pub fn topology(mut self, topology: wgpu::PrimitiveTopology) -> Self {
        self.primitive.topology = topology;
        self
    }

    /// Set cull mode (default: `None`).
    pub fn cull_mode(mut self, cull: Option<wgpu::Face>) -> Self {
        self.primitive.cull_mode = cull;
        self
    }

    /// Set front face winding (default: `Ccw`).
    pub fn front_face(mut self, front_face: wgpu::FrontFace) -> Self {
        self.primitive.front_face = front_face;
        self
    }

    /// Set multisample state.
    pub fn multisample(mut self, state: wgpu::MultisampleState) -> Self {
        self.multisample = state;
        self
    }

    /// Build the render pipeline. Returns error on shader compilation failure.
    pub fn build(self) -> Result<RenderPipeline> {
        tracing::debug!(
            label = self.label.unwrap_or("unnamed"),
            "building render pipeline"
        );

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: self.label,
                source: wgpu::ShaderSource::Wgsl(self.wgsl_source.into()),
            });

        let bind_group_layouts: Vec<wgpu::BindGroupLayout> = self
            .bind_group_layout_entries
            .iter()
            .enumerate()
            .map(|(i, entries)| {
                self.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some(&format!("bind_group_layout_{i}")),
                        entries,
                    })
            })
            .collect();

        let layout_refs: Vec<Option<&wgpu::BindGroupLayout>> =
            bind_group_layouts.iter().map(Some).collect();

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts: &layout_refs,
                immediate_size: 0,
            });

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: self.label,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(self.vertex_entry),
                    buffers: &self.vertex_layouts,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(self.fragment_entry),
                    targets: &self.color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: self.primitive,
                depth_stencil: self.depth_stencil,
                multisample: self.multisample,
                multiview_mask: None,
                cache: None,
            });

        Ok(RenderPipeline {
            pipeline,
            bind_group_layouts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_command_variants() {
        let d = DrawCommand::Draw {
            vertex_count: 3,
            instance_count: 1,
        };
        assert!(matches!(d, DrawCommand::Draw { .. }));

        let di = DrawCommand::DrawIndexed {
            index_count: 6,
            instance_count: 1,
            first_index: 0,
            base_vertex: 0,
            first_instance: 0,
        };
        assert!(matches!(di, DrawCommand::DrawIndexed { .. }));
    }

    #[test]
    fn render_pipeline_types() {
        let _size = std::mem::size_of::<RenderPipeline>();
    }

    #[test]
    fn draw_command_debug() {
        let d = DrawCommand::Draw {
            vertex_count: 100,
            instance_count: 10,
        };
        let s = format!("{d:?}");
        assert!(s.contains("100"));
        assert!(s.contains("10"));
    }

    #[test]
    fn draw_command_clone() {
        let d = DrawCommand::DrawIndexed {
            index_count: 36,
            instance_count: 1,
            first_index: 0,
            base_vertex: 0,
            first_instance: 0,
        };
        let d2 = d;
        assert!(matches!(
            d2,
            DrawCommand::DrawIndexed {
                index_count: 36,
                ..
            }
        ));
    }

    #[test]
    fn primitive_defaults() {
        let prim = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        };
        assert_eq!(prim.topology, wgpu::PrimitiveTopology::TriangleList);
        assert_eq!(prim.front_face, wgpu::FrontFace::Ccw);
        assert!(prim.cull_mode.is_none());
    }
}
