//! Surface lifecycle management.
//!
//! [`SurfaceState`] manages wgpu surface configuration, resize, and frame
//! acquisition. It does NOT own the `wgpu::Surface` — consumers create the
//! surface from their windowing system and pass it to each method.

use crate::error::{GpuError, Result};

/// Present mode preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PresentModePreference {
    /// VSync enabled (lowest latency with no tearing).
    Vsync,
    /// VSync disabled (may tear, lowest latency).
    NoVsync,
    /// Immediate presentation (tearing allowed).
    Immediate,
    /// Mailbox (low latency, no tearing, if supported).
    Mailbox,
}

impl PresentModePreference {
    /// Convert to `wgpu::PresentMode`.
    #[must_use]
    fn to_wgpu(self) -> wgpu::PresentMode {
        match self {
            Self::Vsync => wgpu::PresentMode::AutoVsync,
            Self::NoVsync => wgpu::PresentMode::AutoNoVsync,
            Self::Immediate => wgpu::PresentMode::Immediate,
            Self::Mailbox => wgpu::PresentMode::Mailbox,
        }
    }
}

/// A configured wgpu surface, ready for frame acquisition.
///
/// Does NOT own the `wgpu::Surface` — the consumer creates the surface
/// from their windowing system and passes it in. This struct manages
/// configuration, resize, and frame acquisition.
pub struct SurfaceState {
    config: wgpu::SurfaceConfiguration,
    format: wgpu::TextureFormat,
}

impl SurfaceState {
    /// Configure a surface for rendering.
    ///
    /// Selects the best available format (preferring sRGB) and alpha mode.
    pub fn configure(
        surface: &wgpu::Surface<'_>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        present_mode: PresentModePreference,
    ) -> Result<Self> {
        let caps = surface.get_capabilities(adapter);

        if caps.formats.is_empty() {
            return Err(GpuError::SurfaceConfig(
                "no supported surface formats".into(),
            ));
        }

        // Prefer sRGB format, fall back to first available
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: present_mode.to_wgpu(),
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };

        surface.configure(device, &config);

        tracing::debug!(?format, width, height, ?present_mode, "surface configured");

        Ok(Self { config, format })
    }

    /// Reconfigure the surface after a resize.
    ///
    /// Silently skips zero-size dimensions (common during window minimize).
    pub fn resize(
        &mut self,
        surface: &wgpu::Surface<'_>,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) {
        if width == 0 || height == 0 {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        surface.configure(device, &self.config);

        tracing::debug!(width, height, "surface resized");
    }

    /// Acquire the next frame texture.
    pub fn acquire(&self, surface: &wgpu::Surface<'_>) -> Result<wgpu::SurfaceTexture> {
        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => Ok(texture),
            wgpu::CurrentSurfaceTexture::Timeout => {
                Err(GpuError::SurfaceTexture("surface timeout".into()))
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                Err(GpuError::SurfaceTexture("surface outdated".into()))
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                Err(GpuError::SurfaceTexture("surface lost".into()))
            }
            _ => Err(GpuError::SurfaceTexture("unknown surface error".into())),
        }
    }

    /// The surface texture format.
    #[must_use]
    #[inline]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Current surface dimensions.
    #[must_use]
    #[inline]
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_mode_variants() {
        let modes = [
            PresentModePreference::Vsync,
            PresentModePreference::NoVsync,
            PresentModePreference::Immediate,
            PresentModePreference::Mailbox,
        ];
        assert_eq!(modes.len(), 4);
    }

    #[test]
    fn present_mode_to_wgpu() {
        assert_eq!(
            PresentModePreference::Vsync.to_wgpu(),
            wgpu::PresentMode::AutoVsync
        );
        assert_eq!(
            PresentModePreference::NoVsync.to_wgpu(),
            wgpu::PresentMode::AutoNoVsync
        );
        assert_eq!(
            PresentModePreference::Immediate.to_wgpu(),
            wgpu::PresentMode::Immediate
        );
        assert_eq!(
            PresentModePreference::Mailbox.to_wgpu(),
            wgpu::PresentMode::Mailbox
        );
    }

    #[test]
    fn present_mode_equality() {
        assert_eq!(PresentModePreference::Vsync, PresentModePreference::Vsync);
        assert_ne!(PresentModePreference::Vsync, PresentModePreference::NoVsync);
    }

    #[test]
    fn surface_state_types() {
        let _size = std::mem::size_of::<SurfaceState>();
    }
}
