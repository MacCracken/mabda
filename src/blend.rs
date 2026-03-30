//! Blend state presets for common rendering modes.
//!
//! Use [`BlendPreset`] with [`blend_state`] to get a `wgpu::BlendState`
//! for render pipeline color targets.

/// Common blend mode presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BlendPreset {
    /// No blending — source overwrites destination.
    Opaque,
    /// Standard alpha blending: `src * src_alpha + dst * (1 - src_alpha)`.
    AlphaBlend,
    /// Premultiplied alpha: `src + dst * (1 - src_alpha)`.
    /// Use when source colors are pre-multiplied by alpha.
    PremultipliedAlpha,
    /// Additive blending: `src + dst`. No alpha modulation.
    /// Good for glow, particles, light accumulation.
    Additive,
    /// Multiply blending: `src * dst`.
    /// Darkens the destination by the source color.
    Multiply,
}

/// Convert a blend preset to a `wgpu::BlendState`.
#[must_use]
pub fn blend_state(preset: BlendPreset) -> wgpu::BlendState {
    match preset {
        BlendPreset::Opaque => wgpu::BlendState::REPLACE,
        BlendPreset::AlphaBlend => wgpu::BlendState::ALPHA_BLENDING,
        BlendPreset::PremultipliedAlpha => wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
        BlendPreset::Additive => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        },
        BlendPreset::Multiply => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::DstAlpha,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_preset_variants() {
        let presets = [
            BlendPreset::Opaque,
            BlendPreset::AlphaBlend,
            BlendPreset::PremultipliedAlpha,
            BlendPreset::Additive,
            BlendPreset::Multiply,
        ];
        assert_eq!(presets.len(), 5);
    }

    #[test]
    fn blend_state_opaque() {
        let state = blend_state(BlendPreset::Opaque);
        assert_eq!(state, wgpu::BlendState::REPLACE);
    }

    #[test]
    fn blend_state_alpha() {
        let state = blend_state(BlendPreset::AlphaBlend);
        assert_eq!(state, wgpu::BlendState::ALPHA_BLENDING);
    }

    #[test]
    fn blend_state_premultiplied() {
        let state = blend_state(BlendPreset::PremultipliedAlpha);
        assert_eq!(state, wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
    }

    #[test]
    fn blend_state_additive_symmetry() {
        let state = blend_state(BlendPreset::Additive);
        assert_eq!(state.color.src_factor, wgpu::BlendFactor::One);
        assert_eq!(state.color.dst_factor, wgpu::BlendFactor::One);
    }

    #[test]
    fn blend_preset_equality() {
        assert_eq!(BlendPreset::Opaque, BlendPreset::Opaque);
        assert_ne!(BlendPreset::Opaque, BlendPreset::Additive);
    }

    #[test]
    fn blend_preset_debug() {
        assert_eq!(format!("{:?}", BlendPreset::Multiply), "Multiply");
    }
}
