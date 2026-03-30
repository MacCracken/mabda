//! Debug labels and marker groups for GPU command encoders.
//!
//! These helpers wrap wgpu's debug group and marker APIs for structured
//! GPU debugging in tools like RenderDoc, Xcode GPU Debugger, and PIX.

/// Push a named debug group onto a command encoder.
///
/// Debug groups appear as collapsible sections in GPU profiling tools.
/// Always pair with [`pop_debug_group`].
#[inline]
pub fn push_debug_group(encoder: &mut wgpu::CommandEncoder, label: &str) {
    encoder.push_debug_group(label);
}

/// Pop the current debug group from a command encoder.
#[inline]
pub fn pop_debug_group(encoder: &mut wgpu::CommandEncoder) {
    encoder.pop_debug_group();
}

/// Insert a debug marker at the current point in the command stream.
///
/// Markers appear as single points in GPU profiling tools (not collapsible).
#[inline]
pub fn insert_debug_marker(encoder: &mut wgpu::CommandEncoder, label: &str) {
    encoder.insert_debug_marker(label);
}

/// RAII debug group guard — pushes on creation, pops on drop.
///
/// # Example
///
/// ```ignore
/// {
///     let _guard = DebugScope::new(&mut encoder, "shadow_pass");
///     // ... render commands ...
/// } // debug group automatically popped here
/// ```
pub struct DebugScope<'a> {
    encoder: &'a mut wgpu::CommandEncoder,
}

impl<'a> DebugScope<'a> {
    /// Push a debug group and return a guard that pops it on drop.
    #[must_use = "debug scope pops immediately if not bound to a variable"]
    pub fn new(encoder: &'a mut wgpu::CommandEncoder, label: &str) -> Self {
        encoder.push_debug_group(label);
        Self { encoder }
    }
}

impl Drop for DebugScope<'_> {
    fn drop(&mut self) {
        self.encoder.pop_debug_group();
    }
}

/// Push a named debug group onto a render pass.
#[inline]
pub fn push_render_debug_group<'a>(pass: &mut wgpu::RenderPass<'a>, label: &str) {
    pass.push_debug_group(label);
}

/// Pop the current debug group from a render pass.
#[inline]
pub fn pop_render_debug_group<'a>(pass: &mut wgpu::RenderPass<'a>) {
    pass.pop_debug_group();
}

/// Insert a debug marker into a render pass.
#[inline]
pub fn insert_render_debug_marker<'a>(pass: &mut wgpu::RenderPass<'a>, label: &str) {
    pass.insert_debug_marker(label);
}

/// Push a named debug group onto a compute pass.
#[inline]
pub fn push_compute_debug_group<'a>(pass: &mut wgpu::ComputePass<'a>, label: &str) {
    pass.push_debug_group(label);
}

/// Pop the current debug group from a compute pass.
#[inline]
pub fn pop_compute_debug_group<'a>(pass: &mut wgpu::ComputePass<'a>) {
    pass.pop_debug_group();
}

/// Insert a debug marker into a compute pass.
#[inline]
pub fn insert_compute_debug_marker<'a>(pass: &mut wgpu::ComputePass<'a>, label: &str) {
    pass.insert_debug_marker(label);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_scope_types() {
        let _size = std::mem::size_of::<DebugScope<'_>>();
    }

    fn try_gpu() -> Option<crate::context::GpuContext> {
        pollster::block_on(crate::context::GpuContext::new()).ok()
    }

    #[test]
    fn gpu_push_pop_debug_group() {
        let Some(ctx) = try_gpu() else { return };
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("test"),
            });
        push_debug_group(&mut encoder, "test_group");
        insert_debug_marker(&mut encoder, "marker");
        pop_debug_group(&mut encoder);
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }

    #[test]
    fn gpu_debug_scope_raii() {
        let Some(ctx) = try_gpu() else { return };
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scope_test"),
            });
        {
            let _scope = DebugScope::new(&mut encoder, "scoped_group");
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }

    #[test]
    #[cfg(feature = "graphics")]
    fn gpu_render_pass_debug_groups() {
        let Some(ctx) = try_gpu() else { return };
        let target = crate::render_target::RenderTarget::new(
            &ctx.device,
            64,
            64,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_debug_test"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("debug_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            push_render_debug_group(&mut pass, "render_group");
            insert_render_debug_marker(&mut pass, "render_marker");
            pop_render_debug_group(&mut pass);
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }

    #[test]
    fn gpu_compute_pass_debug_groups() {
        let Some(ctx) = try_gpu() else { return };
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("compute_debug_test"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("debug_compute_pass"),
                timestamp_writes: None,
            });
            push_compute_debug_group(&mut pass, "compute_group");
            insert_compute_debug_marker(&mut pass, "compute_marker");
            pop_compute_debug_group(&mut pass);
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }
}
