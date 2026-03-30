//! Color types and utilities.
//!
//! [`Color`] is the shared RGBA color type used across the AGNOS GPU ecosystem.
//! It provides conversions to/from common formats (u8, hex, array, wgpu) and
//! basic color math (lerp, luminance).

use serde::{Deserialize, Serialize};

/// RGBA color with f32 components (0.0–1.0).
///
/// # Examples
///
/// ```
/// use mabda::color::Color;
///
/// let c = Color::new(1.0, 0.5, 0.0, 1.0);
/// assert_eq!(c.r, 1.0);
///
/// let red = Color::from_hex(0xFF0000FF);
/// let blended = Color::WHITE.lerp(Color::BLACK, 0.5);
/// assert!((blended.r - 0.5).abs() < f32::EPSILON);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const RED: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const GREEN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const BLUE: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const CORNFLOWER_BLUE: Self = Self {
        r: 0.392,
        g: 0.584,
        b: 0.929,
        a: 1.0,
    };

    /// Create a color from RGBA components (0.0–1.0).
    #[must_use]
    #[inline]
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        debug_assert!(
            r.is_finite() && g.is_finite() && b.is_finite() && a.is_finite(),
            "Color components must be finite"
        );
        Self { r, g, b, a }
    }

    /// Create a color from RGB components with full opacity.
    #[must_use]
    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Create from 8-bit RGBA (0–255).
    #[must_use]
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Create from hex color (e.g., `0xFF0000FF` for opaque red).
    #[must_use]
    pub fn from_hex(hex: u32) -> Self {
        Self::from_rgba8(
            ((hex >> 24) & 0xFF) as u8,
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
        )
    }

    /// Convert to `[f32; 4]` array.
    #[must_use]
    #[inline]
    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Convert to `wgpu::Color`.
    #[must_use]
    pub fn to_wgpu(self) -> wgpu::Color {
        wgpu::Color {
            r: self.r as f64,
            g: self.g as f64,
            b: self.b as f64,
            a: self.a as f64,
        }
    }

    /// Luminance (perceived brightness, Rec. 709).
    #[must_use]
    #[inline]
    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// Linear interpolation between two colors.
    #[must_use]
    #[inline]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

impl From<[f32; 4]> for Color {
    fn from(arr: [f32; 4]) -> Self {
        Self::new(arr[0], arr[1], arr[2], arr[3])
    }
}

impl From<[f32; 3]> for Color {
    fn from(arr: [f32; 3]) -> Self {
        Self::rgb(arr[0], arr[1], arr[2])
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        c.to_array()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_constants() {
        assert_eq!(Color::WHITE.to_array(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(Color::BLACK.to_array(), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(Color::TRANSPARENT.a, 0.0);
    }

    #[test]
    fn color_from_rgba8() {
        let c = Color::from_rgba8(255, 128, 0, 255);
        assert!((c.r - 1.0).abs() < f32::EPSILON);
        assert!((c.g - 128.0 / 255.0).abs() < 0.001);
        assert_eq!(c.b, 0.0);
    }

    #[test]
    fn color_from_hex() {
        let red = Color::from_hex(0xFF0000FF);
        assert_eq!(red.r, 1.0);
        assert_eq!(red.g, 0.0);
        assert_eq!(red.b, 0.0);
        assert_eq!(red.a, 1.0);
    }

    #[test]
    fn color_lerp() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        let mid = a.lerp(b, 0.5);
        assert!((mid.r - 0.5).abs() < f32::EPSILON);
        assert!((mid.g - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn color_lerp_clamp() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        let over = a.lerp(b, 2.0);
        assert_eq!(over.r, 1.0);
    }

    #[test]
    fn color_from_array() {
        let c: Color = [0.1, 0.2, 0.3, 0.4].into();
        assert_eq!(c.r, 0.1);
        assert_eq!(c.a, 0.4);

        let c3: Color = [0.5, 0.6, 0.7].into();
        assert_eq!(c3.a, 1.0);
    }

    #[test]
    fn color_to_array() {
        let arr: [f32; 4] = Color::RED.into();
        assert_eq!(arr, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn color_serde_roundtrip() {
        let c = Color::new(0.1, 0.2, 0.3, 0.9);
        let json = serde_json::to_string(&c).unwrap();
        let decoded: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(c, decoded);
    }

    #[test]
    fn color_to_wgpu() {
        let c = Color::RED;
        let w = c.to_wgpu();
        assert_eq!(w.r, 1.0);
        assert_eq!(w.g, 0.0);
    }

    #[test]
    fn color_default() {
        assert_eq!(Color::default(), Color::WHITE);
    }

    #[test]
    fn color_luminance() {
        assert!((Color::WHITE.luminance() - 1.0).abs() < 0.01);
        assert_eq!(Color::BLACK.luminance(), 0.0);
        assert!(Color::RED.luminance() < Color::GREEN.luminance());
    }

    #[test]
    fn color_rgb_constructor() {
        let c = Color::rgb(0.1, 0.2, 0.3);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn color_lerp_boundaries() {
        let a = Color::RED;
        let b = Color::BLUE;
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, -1.0), a);
    }

    #[test]
    fn color_from_hex_zero() {
        assert_eq!(Color::from_hex(0x00000000), Color::TRANSPARENT);
    }

    #[test]
    fn color_from_rgba8_boundary() {
        assert_eq!(Color::from_rgba8(0, 0, 0, 0), Color::TRANSPARENT);
        assert_eq!(Color::from_rgba8(255, 255, 255, 255), Color::WHITE);
    }

    #[test]
    fn color_lerp_midpoints() {
        let a = Color::new(0.0, 0.0, 0.0, 0.0);
        let b = Color::new(1.0, 1.0, 1.0, 1.0);
        let q = a.lerp(b, 0.25);
        assert!((q.r - 0.25).abs() < f32::EPSILON);
        assert!((q.a - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn color_from_hex_components() {
        let c = Color::from_hex(0xAABBCCDD);
        assert!((c.r - 0xAA as f32 / 255.0).abs() < 0.001);
        assert!((c.g - 0xBB as f32 / 255.0).abs() < 0.001);
        assert!((c.b - 0xCC as f32 / 255.0).abs() < 0.001);
        assert!((c.a - 0xDD as f32 / 255.0).abs() < 0.001);
    }

    #[test]
    fn color_rgba8_roundtrip() {
        for val in [0u8, 1, 127, 128, 254, 255] {
            let c = Color::from_rgba8(val, val, val, val);
            let r_back = (c.r * 255.0).round() as u8;
            assert_eq!(r_back, val, "roundtrip failed for {val}");
        }
    }
}
