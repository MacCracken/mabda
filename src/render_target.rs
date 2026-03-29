//! Offscreen render targets (framebuffers).
//!
//! [`RenderTarget`] creates an offscreen texture that can be rendered to and
//! read back. Used for screenshots, post-processing intermediate buffers,
//! and headless rendering.

use crate::error::{GpuError, Result};

/// An offscreen render target (framebuffer) that can be drawn to and read back.
pub struct RenderTarget {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
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

    /// Read back the render target pixels as RGBA8 bytes.
    ///
    /// This is a blocking GPU readback — use for tests and screenshots,
    /// not in game loops.
    pub fn read_pixels(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Vec<u8>> {
        tracing::debug!(self.width, self.height, ?self.format, "reading render target pixels");
        let bytes_per_row = 4u32.checked_mul(self.width).ok_or_else(|| {
            tracing::error!(width = self.width, "render target bytes_per_row overflow");
            GpuError::Readback("bytes_per_row overflow".into())
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
                GpuError::Readback("buffer size overflow".into())
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
                GpuError::Readback(format!("channel: {e}"))
            })?
            .map_err(|e| {
                tracing::error!("render target readback map failed: {e}");
                GpuError::Readback(format!("map failed: {e}"))
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

#[cfg(test)]
mod tests {
    #[test]
    fn render_target_size() {
        let _size = std::mem::size_of::<super::RenderTarget>();
    }
}
