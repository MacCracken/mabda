//! Render pass builder for fluent attachment configuration.
//!
//! [`RenderPassBuilder`] provides a convenient API for constructing
//! `wgpu::RenderPassDescriptor` with color attachments (including MRT),
//! depth/stencil, and clear values.

/// Builder for configuring and beginning a render pass on a command encoder.
///
/// Supports multiple color attachments (MRT), optional depth/stencil,
/// and per-attachment load/store operations.
///
/// # Example
///
/// ```ignore
/// let mut pass = RenderPassBuilder::new()
///     .color_attachment(&color_view, Some(Color::BLACK))
///     .depth_attachment(&depth_view)
///     .begin(&mut encoder);
/// ```
pub struct RenderPassBuilder<'a> {
    label: Option<&'a str>,
    color_attachments: Vec<ColorAttachment<'a>>,
    depth_attachment: Option<DepthAttachment<'a>>,
}

struct ColorAttachment<'a> {
    view: &'a wgpu::TextureView,
    resolve_target: Option<&'a wgpu::TextureView>,
    load: wgpu::LoadOp<wgpu::Color>,
    store: wgpu::StoreOp,
}

struct DepthAttachment<'a> {
    view: &'a wgpu::TextureView,
    depth_load: wgpu::LoadOp<f32>,
    depth_store: wgpu::StoreOp,
    stencil_ops: Option<wgpu::Operations<u32>>,
}

impl<'a> RenderPassBuilder<'a> {
    /// Create a new render pass builder with no attachments.
    #[must_use]
    pub fn new() -> Self {
        Self {
            label: None,
            color_attachments: Vec::new(),
            depth_attachment: None,
        }
    }

    /// Set a debug label for the render pass.
    #[must_use]
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Add a color attachment that clears to the given color.
    ///
    /// Pass `None` for `clear_color` to load the existing contents.
    #[must_use]
    pub fn color_attachment(
        mut self,
        view: &'a wgpu::TextureView,
        clear_color: Option<crate::color::Color>,
    ) -> Self {
        let load = match clear_color {
            Some(c) => wgpu::LoadOp::Clear(c.to_wgpu()),
            None => wgpu::LoadOp::Load,
        };
        self.color_attachments.push(ColorAttachment {
            view,
            resolve_target: None,
            load,
            store: wgpu::StoreOp::Store,
        });
        self
    }

    /// Add a color attachment with an MSAA resolve target.
    ///
    /// `render_view` is the multisampled texture to render into.
    /// `resolve_view` is the single-sampled texture to resolve to.
    #[must_use]
    pub fn color_attachment_msaa(
        mut self,
        render_view: &'a wgpu::TextureView,
        resolve_view: &'a wgpu::TextureView,
        clear_color: Option<crate::color::Color>,
    ) -> Self {
        let load = match clear_color {
            Some(c) => wgpu::LoadOp::Clear(c.to_wgpu()),
            None => wgpu::LoadOp::Load,
        };
        self.color_attachments.push(ColorAttachment {
            view: render_view,
            resolve_target: Some(resolve_view),
            load,
            store: wgpu::StoreOp::Store,
        });
        self
    }

    /// Add a depth attachment that clears to 1.0 (far plane).
    #[must_use]
    pub fn depth_attachment(mut self, view: &'a wgpu::TextureView) -> Self {
        self.depth_attachment = Some(DepthAttachment {
            view,
            depth_load: wgpu::LoadOp::Clear(1.0),
            depth_store: wgpu::StoreOp::Store,
            stencil_ops: None,
        });
        self
    }

    /// Add a depth attachment that loads existing depth values.
    #[must_use]
    pub fn depth_attachment_load(mut self, view: &'a wgpu::TextureView) -> Self {
        self.depth_attachment = Some(DepthAttachment {
            view,
            depth_load: wgpu::LoadOp::Load,
            depth_store: wgpu::StoreOp::Store,
            stencil_ops: None,
        });
        self
    }

    /// Add a depth+stencil attachment that clears both.
    #[must_use]
    pub fn depth_stencil_attachment(mut self, view: &'a wgpu::TextureView) -> Self {
        self.depth_attachment = Some(DepthAttachment {
            view,
            depth_load: wgpu::LoadOp::Clear(1.0),
            depth_store: wgpu::StoreOp::Store,
            stencil_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(0),
                store: wgpu::StoreOp::Store,
            }),
        });
        self
    }

    /// Begin the render pass on the given command encoder.
    ///
    /// Returns the `wgpu::RenderPass` ready for pipeline/bindgroup/draw calls.
    pub fn begin(self, encoder: &'a mut wgpu::CommandEncoder) -> wgpu::RenderPass<'a> {
        let color_attachments: Vec<Option<wgpu::RenderPassColorAttachment<'_>>> = self
            .color_attachments
            .iter()
            .map(|ca| {
                Some(wgpu::RenderPassColorAttachment {
                    view: ca.view,
                    resolve_target: ca.resolve_target,
                    ops: wgpu::Operations {
                        load: ca.load,
                        store: ca.store,
                    },
                    depth_slice: None,
                })
            })
            .collect();

        let depth_stencil_attachment =
            self.depth_attachment
                .as_ref()
                .map(|da| wgpu::RenderPassDepthStencilAttachment {
                    view: da.view,
                    depth_ops: Some(wgpu::Operations {
                        load: da.depth_load,
                        store: da.depth_store,
                    }),
                    stencil_ops: da.stencil_ops,
                });

        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: self.label,
            color_attachments: &color_attachments,
            depth_stencil_attachment,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }
}

impl Default for RenderPassBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_default() {
        let builder = RenderPassBuilder::new();
        assert!(builder.color_attachments.is_empty());
        assert!(builder.depth_attachment.is_none());
        assert!(builder.label.is_none());
    }

    #[test]
    fn builder_default_trait() {
        let builder = RenderPassBuilder::default();
        assert!(builder.color_attachments.is_empty());
    }

    #[test]
    fn builder_types() {
        let _size = std::mem::size_of::<RenderPassBuilder<'_>>();
    }
}
