//! GPU resource lifecycle helpers.
//!
//! [`FrameResources`] tracks transient GPU resources that should be
//! reclaimed at the end of each frame (temporary buffers, staging uploads).

/// Tracks transient GPU resources for automatic end-of-frame cleanup.
///
/// Resources added during a frame are dropped when [`clear`](Self::clear)
/// is called (typically at frame start or end). This prevents accumulation
/// of temporary buffers and textures.
///
/// # Example
///
/// ```ignore
/// let mut frame_res = FrameResources::new();
///
/// // During frame: register temporary resources
/// let staging = create_staging_buffer(device, size, "temp");
/// frame_res.track_buffer(staging);
///
/// // End of frame: all tracked resources dropped
/// frame_res.clear();
/// ```
pub struct FrameResources {
    buffers: Vec<wgpu::Buffer>,
    textures: Vec<wgpu::Texture>,
}

impl FrameResources {
    /// Create an empty resource tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            textures: Vec::new(),
        }
    }

    /// Track a buffer for end-of-frame cleanup.
    pub fn track_buffer(&mut self, buffer: wgpu::Buffer) {
        self.buffers.push(buffer);
    }

    /// Track a texture for end-of-frame cleanup.
    pub fn track_texture(&mut self, texture: wgpu::Texture) {
        self.textures.push(texture);
    }

    /// Drop all tracked resources.
    ///
    /// Call at frame start or end to reclaim GPU memory from temporary
    /// resources created during the previous frame.
    pub fn clear(&mut self) {
        if !self.buffers.is_empty() || !self.textures.is_empty() {
            tracing::debug!(
                buffers = self.buffers.len(),
                textures = self.textures.len(),
                "clearing frame resources"
            );
        }
        self.buffers.clear();
        self.textures.clear();
    }

    /// Number of tracked buffers.
    #[must_use]
    #[inline]
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Number of tracked textures.
    #[must_use]
    #[inline]
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Total number of tracked resources.
    #[must_use]
    #[inline]
    pub fn total_count(&self) -> usize {
        self.buffers.len() + self.textures.len()
    }

    /// Whether any resources are being tracked.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty() && self.textures.is_empty()
    }
}

impl Default for FrameResources {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_resources_empty() {
        let res = FrameResources::new();
        assert!(res.is_empty());
        assert_eq!(res.total_count(), 0);
        assert_eq!(res.buffer_count(), 0);
        assert_eq!(res.texture_count(), 0);
    }

    #[test]
    fn frame_resources_default() {
        let res = FrameResources::default();
        assert!(res.is_empty());
    }

    #[test]
    fn frame_resources_types() {
        let _size = std::mem::size_of::<FrameResources>();
    }

    fn try_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let ctx = pollster::block_on(crate::context::GpuContext::new()).ok()?;
        Some((ctx.device, ctx.queue))
    }

    #[test]
    fn gpu_track_buffer_and_clear() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let mut res = FrameResources::new();
        let buf = crate::buffer::create_storage_buffer(&device, &[0u8; 64], "track_buf", true);
        res.track_buffer(buf);
        assert_eq!(res.buffer_count(), 1);
        assert_eq!(res.total_count(), 1);
        res.clear();
        assert!(res.is_empty());
    }

    #[test]
    fn gpu_track_texture_and_clear() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let mut res = FrameResources::new();
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("track_tex"),
            size: wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        res.track_texture(tex);
        assert_eq!(res.texture_count(), 1);
        res.clear();
        assert!(res.is_empty());
    }

    #[test]
    fn gpu_track_mixed() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let mut res = FrameResources::new();
        let buf1 = crate::buffer::create_storage_buffer(&device, &[0u8; 32], "mixed_buf1", true);
        let buf2 = crate::buffer::create_storage_buffer(&device, &[0u8; 32], "mixed_buf2", true);
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mixed_tex"),
            size: wgpu::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        res.track_buffer(buf1);
        res.track_buffer(buf2);
        res.track_texture(tex);
        assert_eq!(res.total_count(), 3);
        res.clear();
        assert!(res.is_empty());
    }
}
