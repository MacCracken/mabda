//! Texture loading and management.
//!
//! [`Texture`] wraps a `wgpu::Texture` with its view and sampler, ready for
//! binding in shaders. [`TextureCache`] provides keyed caching for loaded textures.

use crate::error::{GpuError, Result};
use std::collections::HashMap;

/// A shared sampler that can be reused across textures.
/// Create once and pass to [`Texture::from_rgba_with_sampler`] to avoid
/// per-texture sampler allocation.
#[must_use]
pub fn create_default_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("shared_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

/// A GPU texture with view and sampler, ready for binding.
///
/// # Examples
///
/// ```ignore
/// use mabda::texture::Texture;
///
/// // Load from image bytes (requires `image` feature):
/// let tex = Texture::from_bytes(&device, &queue, include_bytes!("icon.png"), "icon")?;
///
/// // 1x1 fallback textures:
/// let white = Texture::white_pixel(&device, &queue)?;
/// let normal = Texture::flat_normal(&device, &queue)?;
/// ```
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    /// Load a texture from PNG/JPEG bytes.
    ///
    /// Requires the `image` feature (enabled by default when the `image`
    /// dependency is present).
    #[cfg(feature = "image")]
    pub fn from_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        label: &str,
    ) -> Result<Self> {
        tracing::debug!(label, "loading texture from bytes");
        let img = image::load_from_memory(bytes)
            .map_err(GpuError::ImageDecode)?
            .to_rgba8();

        let (width, height) = img.dimensions();
        Self::from_rgba(device, queue, &img, width, height, label)
    }

    /// Create a 1x1 solid color texture.
    pub fn from_color(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: crate::color::Color,
    ) -> Result<Self> {
        let rgba = [
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            (color.a * 255.0) as u8,
        ];
        Self::from_rgba(device, queue, &rgba, 1, 1, "solid_color")
    }

    /// Create a 1x1 white pixel texture (default for untextured surfaces).
    pub fn white_pixel(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self> {
        Self::from_color(device, queue, crate::color::Color::WHITE)
    }

    /// Create a 1x1 black pixel texture.
    pub fn black_pixel(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self> {
        Self::from_color(device, queue, crate::color::Color::BLACK)
    }

    /// Create a 1x1 transparent pixel texture.
    pub fn transparent_pixel(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self> {
        Self::from_color(device, queue, crate::color::Color::TRANSPARENT)
    }

    /// Create a 1x1 flat normal map texture (pointing straight up: RGB = 128, 128, 255).
    pub fn flat_normal(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self> {
        Self::from_rgba(device, queue, &[128, 128, 255, 255], 1, 1, "flat_normal")
    }

    /// Create a texture from raw RGBA8 pixel data.
    pub fn from_rgba(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        label: &str,
    ) -> Result<Self> {
        let sampler = create_default_sampler(device);
        Self::from_rgba_with_sampler(device, queue, rgba, width, height, label, sampler)
    }

    /// Create a texture from raw RGBA8 pixel data with an externally-provided sampler.
    pub fn from_rgba_with_sampler(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        label: &str,
        sampler: wgpu::Sampler,
    ) -> Result<Self> {
        Self::from_raw(
            device,
            queue,
            rgba,
            width,
            height,
            4,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            label,
            sampler,
        )
    }

    /// Create a texture from raw pixel data with any format.
    ///
    /// This is the most flexible texture constructor. All other `from_*`
    /// methods delegate to this.
    ///
    /// - `bytes_per_pixel`: Number of bytes per texel (e.g., 4 for RGBA8, 8 for RGBA16Float).
    /// - `format`: The `wgpu::TextureFormat` matching the pixel data layout.
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[u8],
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
        format: wgpu::TextureFormat,
        label: &str,
        sampler: wgpu::Sampler,
    ) -> Result<Self> {
        tracing::debug!(width, height, ?format, label, "creating texture");
        if width == 0 || height == 0 {
            tracing::warn!(width, height, label, "rejected zero-size texture");
            return Err(GpuError::Texture("zero-size texture".into()));
        }

        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(bytes_per_pixel as usize))
            .ok_or_else(|| {
                tracing::error!(width, height, label, "texture dimensions overflow");
                GpuError::Texture("texture dimensions overflow".into())
            })?;
        if data.len() != expected {
            tracing::warn!(
                width,
                height,
                bytes_per_pixel,
                expected,
                actual = data.len(),
                label,
                "pixel buffer size mismatch"
            );
            return Err(GpuError::Texture(format!(
                "pixel buffer size mismatch: expected {width}x{height}x{bytes_per_pixel}={expected}, got {}",
                data.len()
            )));
        }

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_pixel * width),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Ok(Self {
            texture,
            view,
            sampler,
        })
    }

    /// Get the texture format.
    #[must_use]
    #[inline]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.texture.format()
    }

    /// Create a wgpu bind group for this texture (texture view + sampler at bindings 0, 1).
    #[must_use]
    pub fn bind_group(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Texture dimensions as (width, height).
    #[must_use]
    #[inline]
    pub fn size(&self) -> (u32, u32) {
        let s = self.texture.size();
        (s.width, s.height)
    }
}

/// A cached texture entry: texture + its bind group.
struct CachedTexture {
    texture: Texture,
    bind_group: wgpu::BindGroup,
}

/// Cache of loaded textures, keyed by u64 texture ID.
///
/// # Examples
///
/// ```ignore
/// use mabda::texture::TextureCache;
///
/// let mut cache = TextureCache::new();
/// cache.insert(0, texture, 1920 * 1080 * 4);
/// let tex = cache.get(0);
/// ```
pub struct TextureCache {
    entries: HashMap<u64, CachedTexture>,
}

impl TextureCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert a texture, creating its bind group.
    pub fn insert(
        &mut self,
        id: u64,
        texture: Texture,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) {
        tracing::debug!(id, "texture cache insert");
        let bind_group = texture.bind_group(device, layout);
        self.entries.insert(
            id,
            CachedTexture {
                texture,
                bind_group,
            },
        );
    }

    /// Get the bind group for a texture ID, or load from bytes if not cached.
    #[cfg(feature = "image")]
    pub fn get_or_load(
        &mut self,
        id: u64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        bytes: &[u8],
        label: &str,
    ) -> Result<&wgpu::BindGroup> {
        if !self.entries.contains_key(&id) {
            let texture = Texture::from_bytes(device, queue, bytes, label)?;
            self.insert(id, texture, device, layout);
        }
        self.entries
            .get(&id)
            .map(|e| &e.bind_group)
            .ok_or_else(|| GpuError::Texture(format!("texture {id} missing after insert")))
    }

    /// Get the bind group for a texture ID.
    #[must_use]
    pub fn get_bind_group(&self, id: u64) -> Option<&wgpu::BindGroup> {
        self.entries.get(&id).map(|e| &e.bind_group)
    }

    /// Check if a texture ID exists.
    #[must_use]
    #[inline]
    pub fn contains(&self, id: u64) -> bool {
        self.entries.contains_key(&id)
    }

    /// Get a texture reference by ID.
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Texture> {
        self.entries.get(&id).map(|e| &e.texture)
    }

    /// Number of cached textures.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A GPU cubemap texture (6 faces) with view and sampler.
///
/// # Examples
///
/// ```ignore
/// use mabda::texture::CubemapTexture;
///
/// let cubemap = CubemapTexture::from_faces(
///     &device, &queue, &faces, 512, "skybox",
/// )?;
/// ```
pub struct CubemapTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub size: u32,
}

impl CubemapTexture {
    /// Create a cubemap from 6 face images (raw pixel data, same size).
    ///
    /// Faces should be provided in order: +X, -X, +Y, -Y, +Z, -Z.
    /// Each face must be `size x size` pixels in the given format.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        faces: [&[u8]; 6],
        size: u32,
        bytes_per_pixel: u32,
        format: wgpu::TextureFormat,
        label: &str,
        sampler: wgpu::Sampler,
    ) -> Result<Self> {
        tracing::debug!(size, ?format, label, "creating cubemap texture");
        if size == 0 {
            return Err(GpuError::Texture("zero-size cubemap".into()));
        }

        let expected_face_size = (size as usize)
            .checked_mul(size as usize)
            .and_then(|v| v.checked_mul(bytes_per_pixel as usize))
            .ok_or_else(|| {
                tracing::error!(size, bytes_per_pixel, "cubemap face size overflow");
                GpuError::Texture("cubemap face size overflow".into())
            })?;
        for (i, face) in faces.iter().enumerate() {
            if face.len() != expected_face_size {
                return Err(GpuError::Texture(format!(
                    "cubemap face {i} size mismatch: expected {expected_face_size}, got {}",
                    face.len()
                )));
            }
        }

        let extent = wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        for (i, face) in faces.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                face,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_pixel * size),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(label),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        Ok(Self {
            texture,
            view,
            sampler,
            size,
        })
    }

    /// Create a bind group for this cubemap (view at binding 0, sampler at binding 1).
    #[must_use]
    pub fn bind_group(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cubemap_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}

/// Copy a region from one texture to another.
///
/// Both textures must have compatible formats and the region must fit
/// within both source and destination dimensions.
pub fn copy_texture_to_texture(
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::Texture,
    dest: &wgpu::Texture,
    width: u32,
    height: u32,
) {
    tracing::debug!(width, height, "texture-to-texture copy");
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: source,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: dest,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

/// Calculate the number of mip levels for a texture of the given dimensions.
///
/// Returns `floor(log2(max(width, height))) + 1`, which is the standard
/// mip chain length down to 1x1.
#[must_use]
#[inline]
pub fn mip_level_count(width: u32, height: u32) -> u32 {
    if width == 0 || height == 0 {
        return 1;
    }
    (width.max(height)).ilog2() + 1
}

/// Validate texture dimensions against device limits.
///
/// Returns `Err(GpuError::TextureDimensionExceeded)` if either dimension
/// exceeds `max_texture_dimension_2d`.
pub fn validate_dimensions(
    width: u32,
    height: u32,
    limits: &wgpu::Limits,
) -> crate::error::Result<()> {
    let max = limits.max_texture_dimension_2d;
    if width > max {
        return Err(crate::error::GpuError::TextureDimensionExceeded {
            actual: width,
            limit: max,
        });
    }
    if height > max {
        return Err(crate::error::GpuError::TextureDimensionExceeded {
            actual: height,
            limit: max,
        });
    }
    Ok(())
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_cache_empty() {
        let cache = TextureCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(!cache.contains(0));
        assert!(cache.get(0).is_none());
        assert!(cache.get_bind_group(0).is_none());
    }

    #[test]
    fn texture_cache_default() {
        let cache = TextureCache::default();
        assert!(cache.is_empty());
    }

    #[test]
    fn validate_dimensions_within_limits() {
        let limits = wgpu::Limits {
            max_texture_dimension_2d: 8192,
            ..Default::default()
        };
        assert!(validate_dimensions(1024, 1024, &limits).is_ok());
        assert!(validate_dimensions(8192, 8192, &limits).is_ok());
    }

    #[test]
    fn mip_level_count_values() {
        assert_eq!(mip_level_count(1, 1), 1);
        assert_eq!(mip_level_count(2, 2), 2);
        assert_eq!(mip_level_count(4, 4), 3);
        assert_eq!(mip_level_count(256, 256), 9);
        assert_eq!(mip_level_count(1024, 512), 11);
        assert_eq!(mip_level_count(1, 512), 10);
        assert_eq!(mip_level_count(0, 0), 1);
    }

    #[test]
    fn validate_dimensions_exceeds_limits() {
        let limits = wgpu::Limits {
            max_texture_dimension_2d: 8192,
            ..Default::default()
        };
        assert!(validate_dimensions(8193, 1024, &limits).is_err());
        assert!(validate_dimensions(1024, 8193, &limits).is_err());
    }

    fn try_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let ctx = pollster::block_on(crate::context::GpuContext::new()).ok()?;
        Some((ctx.device, ctx.queue))
    }

    #[test]
    fn gpu_create_default_sampler() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let _sampler = create_default_sampler(&device);
    }

    #[test]
    fn gpu_texture_from_rgba() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let rgba = [255u8, 0, 0, 255]; // 1x1 red pixel
        let tex = Texture::from_rgba(&device, &queue, &rgba, 1, 1, "red_pixel").unwrap();
        assert_eq!(tex.texture.width(), 1);
        assert_eq!(tex.texture.height(), 1);
    }

    #[test]
    fn gpu_texture_from_color() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let tex = Texture::from_color(&device, &queue, crate::color::Color::RED).unwrap();
        assert_eq!(tex.texture.width(), 1);
    }

    #[test]
    fn gpu_texture_white_pixel() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let _tex = Texture::white_pixel(&device, &queue).unwrap();
    }

    #[test]
    fn gpu_texture_black_pixel() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let _tex = Texture::black_pixel(&device, &queue).unwrap();
    }

    #[test]
    fn gpu_texture_transparent_pixel() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let _tex = Texture::transparent_pixel(&device, &queue).unwrap();
    }

    #[test]
    fn gpu_texture_flat_normal() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let _tex = Texture::flat_normal(&device, &queue).unwrap();
    }

    #[test]
    fn gpu_texture_from_raw() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let data = vec![128u8; 4 * 4 * 4]; // 4x4 RGBA
        let sampler = create_default_sampler(&device);
        let tex = Texture::from_raw(
            &device,
            &queue,
            &data,
            4,
            4,
            4,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "raw_test",
            sampler,
        )
        .unwrap();
        assert_eq!(tex.texture.width(), 4);
        assert_eq!(tex.texture.height(), 4);
    }

    #[test]
    fn gpu_copy_texture_to_texture() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        // Create textures with COPY_SRC/COPY_DST usage for copy test
        let src_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("copy_src"),
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dst_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("copy_dst"),
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("copy_test"),
        });
        copy_texture_to_texture(&mut encoder, &src_tex, &dst_tex, 2, 2);
        queue.submit(std::iter::once(encoder.finish()));
    }

    #[test]
    fn gpu_texture_bind_group() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let tex = Texture::white_pixel(&device, &queue).unwrap();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tex_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let _bg = tex.bind_group(&device, &layout);
    }

    #[test]
    fn gpu_texture_size() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let data = vec![0u8; 8 * 4 * 4]; // 8x4 RGBA
        let tex = Texture::from_rgba(&device, &queue, &data, 8, 4, "size_test").unwrap();
        assert_eq!(tex.size(), (8, 4));
    }

    #[test]
    fn gpu_texture_format() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let tex = Texture::white_pixel(&device, &queue).unwrap();
        assert_eq!(tex.format(), wgpu::TextureFormat::Rgba8UnormSrgb);
    }

    #[test]
    fn gpu_cubemap_texture() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let face_data = vec![128u8; 2 * 2 * 4]; // 2x2 RGBA per face
        let faces: [&[u8]; 6] = [
            &face_data, &face_data, &face_data, &face_data, &face_data, &face_data,
        ];
        let sampler = create_default_sampler(&device);
        let cubemap = CubemapTexture::new(
            &device,
            &queue,
            faces,
            2,
            4,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "test_cubemap",
            sampler,
        )
        .unwrap();
        assert_eq!(cubemap.size, 2);
    }

    #[test]
    fn gpu_cubemap_zero_size_error() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let face = vec![0u8; 0];
        let faces: [&[u8]; 6] = [&face, &face, &face, &face, &face, &face];
        let sampler = create_default_sampler(&device);
        let result = CubemapTexture::new(
            &device,
            &queue,
            faces,
            0,
            4,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "bad",
            sampler,
        );
        assert!(result.is_err());
    }

    #[test]
    fn gpu_cubemap_face_size_mismatch() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let good_face = vec![0u8; 2 * 2 * 4];
        let bad_face = vec![0u8; 3 * 3 * 4]; // wrong size
        let faces: [&[u8]; 6] = [
            &good_face, &good_face, &good_face, &good_face, &good_face, &bad_face,
        ];
        let sampler = create_default_sampler(&device);
        let result = CubemapTexture::new(
            &device,
            &queue,
            faces,
            2,
            4,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "mismatch",
            sampler,
        );
        assert!(result.is_err());
    }

    #[test]
    fn gpu_texture_cache_with_bind_group() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let tex = Texture::white_pixel(&device, &queue).unwrap();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cache_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let mut cache = TextureCache::new();
        cache.insert(42, tex, &device, &layout);
        assert!(cache.contains(42));
        assert!(cache.get(42).is_some());
        assert!(cache.get_bind_group(42).is_some());
    }
}
