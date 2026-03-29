//! Sampler presets and creation helpers.
//!
//! Provides [`SamplerPreset`] for common sampler configurations and
//! [`create_sampler`] for quick creation. Use [`create_sampler_custom`]
//! for full control over sampler parameters.

/// Predefined sampler configurations for common use cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SamplerPreset {
    /// Nearest-neighbor filtering, clamp to edge.
    /// Best for pixel art and textures that should not be interpolated.
    Nearest,
    /// Bilinear filtering, clamp to edge.
    /// General-purpose filtering for smooth texture sampling.
    Linear,
    /// Anisotropic filtering, clamp to edge.
    /// Best quality for 3D surfaces viewed at oblique angles.
    /// Uses the maximum anisotropy level supported by the device.
    Anisotropic,
    /// Comparison sampler for shadow mapping.
    /// Uses `LessEqual` comparison with linear filtering.
    Comparison,
}

/// Create a sampler from a preset configuration.
#[must_use]
pub fn create_sampler(device: &wgpu::Device, preset: SamplerPreset) -> wgpu::Sampler {
    match preset {
        SamplerPreset::Nearest => device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler_nearest"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        }),
        SamplerPreset::Linear => device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler_linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        }),
        SamplerPreset::Anisotropic => device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler_anisotropic"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        }),
        SamplerPreset::Comparison => device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler_comparison"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        }),
    }
}

/// Create a sampler with full control over all parameters.
#[must_use]
pub fn create_sampler_custom(
    device: &wgpu::Device,
    desc: &wgpu::SamplerDescriptor<'_>,
) -> wgpu::Sampler {
    device.create_sampler(desc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_preset_variants() {
        // Ensure all variants are constructable
        let presets = [
            SamplerPreset::Nearest,
            SamplerPreset::Linear,
            SamplerPreset::Anisotropic,
            SamplerPreset::Comparison,
        ];
        assert_eq!(presets.len(), 4);
    }

    #[test]
    fn sampler_preset_equality() {
        assert_eq!(SamplerPreset::Nearest, SamplerPreset::Nearest);
        assert_ne!(SamplerPreset::Nearest, SamplerPreset::Linear);
    }

    #[test]
    fn sampler_preset_debug() {
        let s = format!("{:?}", SamplerPreset::Comparison);
        assert_eq!(s, "Comparison");
    }
}
