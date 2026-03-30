//! Depth and stencil texture management.
//!
//! [`DepthTexture`] wraps a wgpu depth texture with its view, providing
//! creation and resize helpers for shadow maps, 3D rendering, and
//! depth-based effects.

/// A GPU depth texture with view, ready for use as a depth attachment.
///
/// # Examples
///
/// ```ignore
/// use mabda::depth::DepthTexture;
///
/// let depth = DepthTexture::new_default(&device, 1920, 1080);
/// // Use depth.view as a depth attachment in a render pass
/// ```
pub struct DepthTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
}

impl DepthTexture {
    /// Default depth format (32-bit float, no stencil).
    pub const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// Depth format with 8-bit stencil.
    pub const DEPTH_STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

    /// Create a depth texture with the given dimensions and format.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        tracing::debug!(width, height, ?format, "creating depth texture");

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            texture,
            view,
            format,
            width,
            height,
        }
    }

    /// Create a depth texture with the default format (`Depth32Float`).
    pub fn new_default(device: &wgpu::Device, width: u32, height: u32) -> Self {
        Self::new(device, width, height, Self::DEFAULT_FORMAT)
    }

    /// Recreate the depth texture with new dimensions.
    ///
    /// Silently skips zero-size dimensions.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        tracing::debug!(width, height, ?self.format, "resizing depth texture");
        *self = Self::new(device, width, height, self.format);
    }

    /// Create a depth/stencil state descriptor for render pipeline integration.
    ///
    /// Uses `Less` comparison (standard depth test) with depth writes enabled.
    #[must_use]
    #[inline]
    pub fn depth_stencil_state(&self) -> wgpu::DepthStencilState {
        wgpu::DepthStencilState {
            format: self.format,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_texture_types() {
        let _size = std::mem::size_of::<DepthTexture>();
    }

    #[test]
    fn default_format() {
        assert_eq!(
            DepthTexture::DEFAULT_FORMAT,
            wgpu::TextureFormat::Depth32Float
        );
    }

    #[test]
    fn depth_stencil_format() {
        assert_eq!(
            DepthTexture::DEPTH_STENCIL_FORMAT,
            wgpu::TextureFormat::Depth24PlusStencil8
        );
    }

    fn try_gpu() -> Option<wgpu::Device> {
        let ctx = pollster::block_on(crate::context::GpuContext::new()).ok()?;
        Some(ctx.device)
    }

    #[test]
    fn gpu_create_default_depth() {
        let Some(device) = try_gpu() else { return };
        let depth = DepthTexture::new_default(&device, 1920, 1080);
        assert_eq!(depth.width, 1920);
        assert_eq!(depth.height, 1080);
        assert_eq!(depth.format, DepthTexture::DEFAULT_FORMAT);
    }

    #[test]
    fn gpu_create_depth_stencil() {
        let Some(device) = try_gpu() else { return };
        let depth = DepthTexture::new(&device, 800, 600, DepthTexture::DEPTH_STENCIL_FORMAT);
        assert_eq!(depth.format, wgpu::TextureFormat::Depth24PlusStencil8);
    }

    #[test]
    fn gpu_resize_depth() {
        let Some(device) = try_gpu() else { return };
        let mut depth = DepthTexture::new_default(&device, 100, 100);
        depth.resize(&device, 200, 200);
        assert_eq!(depth.width, 200);
        assert_eq!(depth.height, 200);
    }

    #[test]
    fn gpu_resize_zero_is_noop() {
        let Some(device) = try_gpu() else { return };
        let mut depth = DepthTexture::new_default(&device, 100, 100);
        depth.resize(&device, 0, 200);
        assert_eq!(depth.width, 100); // unchanged
    }

    #[test]
    fn gpu_depth_stencil_state() {
        let Some(device) = try_gpu() else { return };
        let depth = DepthTexture::new_default(&device, 100, 100);
        let state = depth.depth_stencil_state();
        assert_eq!(state.format, DepthTexture::DEFAULT_FORMAT);
        assert_eq!(state.depth_write_enabled, Some(true));
        assert_eq!(state.depth_compare, Some(wgpu::CompareFunction::Less));
    }

    #[test]
    fn gpu_min_size_clamp() {
        let Some(device) = try_gpu() else { return };
        let depth = DepthTexture::new_default(&device, 0, 0);
        assert_eq!(depth.width, 1);
        assert_eq!(depth.height, 1);
    }
}
