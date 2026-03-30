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
}
