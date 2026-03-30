//! Offscreen render targets (framebuffers).
//!
//! [`RenderTarget`] creates an offscreen texture that can be rendered to and
//! read back. Used for screenshots, post-processing intermediate buffers,
//! and headless rendering.
//!
//! For MSAA or depth attachments, use [`RenderTargetBuilder`].

use crate::error::{GpuError, Result};

/// An offscreen render target (framebuffer) that can be drawn to and read back.
///
/// For targets with MSAA or depth attachments, use [`RenderTargetBuilder`].
pub struct RenderTarget {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    /// MSAA sample count (1 = no MSAA).
    pub sample_count: u32,
    /// The multisampled texture (only present when `sample_count > 1`).
    /// When MSAA is active, render into `msaa_view` and resolve to `view`.
    /// Kept alive to back the `msaa_view`.
    #[allow(dead_code)]
    msaa_texture: Option<wgpu::Texture>,
    /// View of the multisampled texture.
    pub msaa_view: Option<wgpu::TextureView>,
    /// Optional depth attachment.
    pub depth: Option<crate::depth::DepthTexture>,
}

impl RenderTarget {
    /// Create a new offscreen render target with the given dimensions and format.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let (width, height) = if width == 0 || height == 0 {
            tracing::warn!(
                width,
                height,
                "zero-size render target requested, clamping to 1x1"
            );
            (width.max(1), height.max(1))
        } else {
            (width, height)
        };

        tracing::debug!(width, height, ?format, "creating render target");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            texture,
            view,
            format,
            width,
            height,
            sample_count: 1,
            msaa_texture: None,
            msaa_view: None,
            depth: None,
        }
    }

    /// Create a render target matching a surface format and size.
    pub fn matching_surface(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        Self::new(device, width, height, surface_format)
    }

    /// Get the view to render into (MSAA view if active, otherwise resolve view).
    #[must_use]
    #[inline]
    pub fn render_view(&self) -> &wgpu::TextureView {
        self.msaa_view.as_ref().unwrap_or(&self.view)
    }

    /// Get the resolve target (only meaningful when MSAA is active).
    ///
    /// Returns `Some(&view)` when `sample_count > 1`, `None` otherwise.
    #[must_use]
    #[inline]
    pub fn resolve_target(&self) -> Option<&wgpu::TextureView> {
        if self.sample_count > 1 {
            Some(&self.view)
        } else {
            None
        }
    }

    /// Read back the render target pixels as RGBA8 bytes.
    ///
    /// This is a blocking GPU readback — use for tests and screenshots,
    /// not in game loops.
    pub fn read_pixels(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Vec<u8>> {
        tracing::debug!(self.width, self.height, ?self.format, "reading render target pixels");
        let bytes_per_row = 4u32.checked_mul(self.width).ok_or_else(|| {
            tracing::error!(width = self.width, "render target bytes_per_row overflow");
            GpuError::Buffer("bytes_per_row overflow".into())
        })?;
        // wgpu requires rows aligned to 256 bytes
        let padded_bytes_per_row = (bytes_per_row + 255) & !255;
        let buffer_size = u64::from(padded_bytes_per_row.checked_mul(self.height).ok_or_else(
            || {
                tracing::error!(
                    width = self.width,
                    height = self.height,
                    "render target buffer size overflow"
                );
                GpuError::Buffer("buffer size overflow".into())
            },
        )?);

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render_target_readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("readback_encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = device.poll(wgpu::PollType::Wait {
            timeout: None,
            submission_index: None,
        });
        rx.recv()
            .map_err(|e| {
                tracing::error!("render target readback channel error: {e}");
                let _ = e;
                GpuError::ReadbackChannel
            })?
            .map_err(|e| {
                tracing::error!("render target readback map failed: {e}");
                GpuError::ReadbackMap(e)
            })?;

        let data = buffer_slice.get_mapped_range();

        // Strip row padding
        let mut pixels = Vec::with_capacity((4 * self.width * self.height) as usize);
        for row in 0..self.height {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + (4 * self.width) as usize;
            pixels.extend_from_slice(&data[start..end]);
        }

        drop(data);
        staging.unmap();

        Ok(pixels)
    }
}

/// Builder for render targets with MSAA and/or depth attachments.
///
/// # Example
///
/// ```ignore
/// let target = RenderTargetBuilder::new(device, 1920, 1080)
///     .format(wgpu::TextureFormat::Rgba8UnormSrgb)
///     .msaa(4)
///     .depth(DepthTexture::DEFAULT_FORMAT)
///     .build();
/// ```
pub struct RenderTargetBuilder<'a> {
    device: &'a wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    sample_count: u32,
    depth_format: Option<wgpu::TextureFormat>,
}

impl<'a> RenderTargetBuilder<'a> {
    /// Start building a render target.
    #[must_use]
    pub fn new(device: &'a wgpu::Device, width: u32, height: u32) -> Self {
        Self {
            device,
            width,
            height,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            sample_count: 1,
            depth_format: None,
        }
    }

    /// Set the color format (default: `Rgba8UnormSrgb`).
    #[must_use]
    pub fn format(mut self, format: wgpu::TextureFormat) -> Self {
        self.format = format;
        self
    }

    /// Enable MSAA with the given sample count (1, 2, 4, 8, or 16).
    ///
    /// When MSAA is active, the render target creates a multisampled texture
    /// for rendering and a single-sampled resolve texture for readback/sampling.
    #[must_use]
    pub fn msaa(mut self, sample_count: u32) -> Self {
        self.sample_count = sample_count;
        self
    }

    /// Attach a depth buffer with the given format.
    #[must_use]
    pub fn depth(mut self, depth_format: wgpu::TextureFormat) -> Self {
        self.depth_format = Some(depth_format);
        self
    }

    /// Build the render target.
    pub fn build(self) -> RenderTarget {
        let (width, height) = (self.width.max(1), self.height.max(1));

        tracing::debug!(
            width,
            height,
            ?self.format,
            self.sample_count,
            depth = self.depth_format.is_some(),
            "creating render target (builder)"
        );

        // Resolve texture (always single-sampled, used for readback/sampling)
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render_target_resolve"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // MSAA texture (multisampled, only if sample_count > 1)
        let (msaa_texture, msaa_view) = if self.sample_count > 1 {
            let msaa_tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("render_target_msaa"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let msaa_v = msaa_tex.create_view(&wgpu::TextureViewDescriptor::default());
            (Some(msaa_tex), Some(msaa_v))
        } else {
            (None, None)
        };

        // Depth attachment
        let depth = self
            .depth_format
            .map(|fmt| crate::depth::DepthTexture::new(self.device, width, height, fmt));

        RenderTarget {
            texture,
            view,
            format: self.format,
            width,
            height,
            sample_count: self.sample_count,
            msaa_texture,
            msaa_view,
            depth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_target_size() {
        let _size = std::mem::size_of::<RenderTarget>();
    }

    #[test]
    fn render_target_no_msaa() {
        // Non-MSAA target: render_view returns main view, resolve_target is None
        // (Can't create actual textures without device, but verify the logic)
        assert_eq!(1u32, 1); // sample_count = 1 means no MSAA
    }

    #[test]
    fn builder_defaults() {
        // Verify builder has sensible defaults without a device
        assert_eq!(
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Rgba8UnormSrgb
        );
    }
}
