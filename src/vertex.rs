//! Vertex types and buffer layout descriptors.
//!
//! Provides canonical vertex types for the AGNOS GPU ecosystem with
//! pre-computed [`wgpu::VertexBufferLayout`] descriptors. All types
//! implement [`VertexLayout`] for generic pipeline construction.

use serde::{Deserialize, Serialize};

/// Trait for types that describe their GPU vertex buffer layout.
///
/// Implement this for custom vertex types to enable generic pipeline
/// construction. All built-in vertex types ([`Vertex2D`], [`Vertex3D`],
/// [`SkinnedVertex3D`]) implement this trait.
///
/// # Examples
///
/// ```ignore
/// use mabda::vertex::VertexLayout;
///
/// // Use the trait to get a layout for any vertex type generically:
/// fn create_pipeline<V: VertexLayout>(device: &wgpu::Device) {
///     let layout = V::layout();
///     // ... use layout in pipeline construction
/// }
/// ```
pub trait VertexLayout {
    /// Returns the wgpu vertex buffer layout descriptor for this type.
    fn layout() -> wgpu::VertexBufferLayout<'static>;
}

/// A 2D vertex with position, texture coordinates, and color.
///
/// # Examples
///
/// ```
/// use mabda::vertex::Vertex2D;
///
/// let v = Vertex2D {
///     position: [0.0, 0.0],
///     tex_coords: [0.0, 0.0],
///     color: [1.0, 1.0, 1.0, 1.0],
/// };
/// assert_eq!(std::mem::size_of::<Vertex2D>(), 32);
/// ```
#[repr(C)]
#[derive(
    Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bytemuck::Pod, bytemuck::Zeroable,
)]
pub struct Vertex2D {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
    pub color: [f32; 4],
}

impl Vertex2D {
    /// wgpu vertex buffer layout for Vertex2D.
    #[must_use]
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // tex_coords
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color (after position [f32; 2] + tex_coords [f32; 2])
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 2]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

impl VertexLayout for Vertex2D {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        Self::layout()
    }
}

/// A 3D vertex with position, normal, texture coordinates, and color.
///
/// # Examples
///
/// ```
/// use mabda::vertex::Vertex3D;
///
/// let v = Vertex3D {
///     position: [0.0, 1.0, 0.0],
///     normal: [0.0, 1.0, 0.0],
///     tex_coords: [0.5, 0.5],
///     color: [1.0, 1.0, 1.0, 1.0],
/// };
/// assert_eq!(std::mem::size_of::<Vertex3D>(), 48);
/// ```
#[repr(C)]
#[derive(
    Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bytemuck::Pod, bytemuck::Zeroable,
)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tex_coords: [f32; 2],
    pub color: [f32; 4],
}

impl Vertex3D {
    /// wgpu vertex buffer layout for Vertex3D.
    #[must_use]
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // normal
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // tex_coords
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 3]>() * 2 + std::mem::size_of::<[f32; 2]>())
                        as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

impl VertexLayout for Vertex3D {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        Self::layout()
    }
}

/// A skinned 3D vertex with tangent, joint indices, and joint weights.
/// Used for skeletal animation and normal-mapped meshes.
///
/// # Examples
///
/// ```
/// use mabda::vertex::SkinnedVertex3D;
///
/// assert_eq!(std::mem::size_of::<SkinnedVertex3D>(), 96);
/// ```
#[repr(C)]
#[derive(
    Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bytemuck::Pod, bytemuck::Zeroable,
)]
pub struct SkinnedVertex3D {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tex_coords: [f32; 2],
    pub color: [f32; 4],
    /// Tangent vector for normal mapping (xyz = tangent, w = handedness ±1).
    pub tangent: [f32; 4],
    /// Joint indices (up to 4 joints per vertex).
    pub joints: [u32; 4],
    /// Joint weights (sum to 1.0).
    pub weights: [f32; 4],
}

impl SkinnedVertex3D {
    /// wgpu vertex buffer layout for SkinnedVertex3D.
    #[must_use]
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        use std::mem::size_of;
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // normal
                wgpu::VertexAttribute {
                    offset: size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // tex_coords
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 3]>() * 2 + size_of::<[f32; 2]>())
                        as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // tangent
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 3]>() * 2
                        + size_of::<[f32; 2]>()
                        + size_of::<[f32; 4]>()) as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // joints
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 3]>() * 2
                        + size_of::<[f32; 2]>()
                        + size_of::<[f32; 4]>() * 2)
                        as wgpu::BufferAddress,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Uint32x4,
                },
                // weights
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 3]>() * 2
                        + size_of::<[f32; 2]>()
                        + size_of::<[f32; 4]>() * 2
                        + size_of::<[u32; 4]>()) as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

impl VertexLayout for SkinnedVertex3D {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        Self::layout()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    #[test]
    fn vertex2d_size() {
        assert_eq!(std::mem::size_of::<Vertex2D>(), 32);
    }

    #[test]
    fn vertex3d_size() {
        assert_eq!(std::mem::size_of::<Vertex3D>(), 48);
    }

    #[test]
    fn skinned_vertex3d_size() {
        assert_eq!(std::mem::size_of::<SkinnedVertex3D>(), 96);
    }

    #[test]
    fn vertex2d_layout_stride() {
        let layout = Vertex2D::layout();
        assert_eq!(layout.array_stride, 32);
        assert_eq!(layout.attributes.len(), 3);
    }

    #[test]
    fn vertex3d_layout_stride() {
        let layout = Vertex3D::layout();
        assert_eq!(layout.array_stride, 48);
        assert_eq!(layout.attributes.len(), 4);
    }

    #[test]
    fn skinned_vertex3d_layout_stride() {
        let layout = SkinnedVertex3D::layout();
        assert_eq!(layout.array_stride, 96);
        assert_eq!(layout.attributes.len(), 7);
    }

    #[test]
    fn vertex2d_layout_offsets() {
        let layout = Vertex2D::layout();
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[1].offset, 8);
        assert_eq!(layout.attributes[2].offset, 16);
    }

    #[test]
    fn vertex3d_layout_offsets() {
        let layout = Vertex3D::layout();
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[1].offset, 12);
        assert_eq!(layout.attributes[2].offset, 24);
        assert_eq!(layout.attributes[3].offset, 32);
    }

    #[test]
    fn skinned_vertex3d_layout_offsets() {
        let layout = SkinnedVertex3D::layout();
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[1].offset, 12);
        assert_eq!(layout.attributes[2].offset, 24);
        assert_eq!(layout.attributes[3].offset, 32);
        assert_eq!(layout.attributes[4].offset, 48);
        assert_eq!(layout.attributes[5].offset, 64);
        assert_eq!(layout.attributes[6].offset, 80);
    }

    #[test]
    fn vertex2d_bytemuck_cast() {
        let v = Vertex2D {
            position: [1.0, 2.0],
            tex_coords: [0.5, 0.5],
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let bytes: &[u8] = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), 32);
        let back: &Vertex2D = bytemuck::from_bytes(bytes);
        assert_eq!(*back, v);
    }

    #[test]
    fn vertex3d_bytemuck_cast() {
        let v = Vertex3D {
            position: [1.0, 2.0, 3.0],
            normal: [0.0, 1.0, 0.0],
            tex_coords: [0.5, 0.5],
            color: [1.0, 0.0, 0.0, 1.0],
        };
        let bytes: &[u8] = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), 48);
        let back: &Vertex3D = bytemuck::from_bytes(bytes);
        assert_eq!(*back, v);
    }

    #[test]
    fn skinned_vertex3d_bytemuck_cast() {
        let v = SkinnedVertex3D {
            position: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            tex_coords: [0.0; 2],
            color: [1.0; 4],
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 1, 0, 0],
            weights: [0.5, 0.5, 0.0, 0.0],
        };
        let bytes = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), 96);
    }

    #[test]
    fn vertex2d_zeroed() {
        let v = Vertex2D::zeroed();
        assert_eq!(v.position, [0.0, 0.0]);
        assert_eq!(v.tex_coords, [0.0, 0.0]);
        assert_eq!(v.color, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn vertex3d_zeroed() {
        let v = Vertex3D::zeroed();
        assert_eq!(v.position, [0.0, 0.0, 0.0]);
        assert_eq!(v.normal, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn skinned_vertex3d_zeroed() {
        let v = SkinnedVertex3D::zeroed();
        assert_eq!(v.position, [0.0; 3]);
        assert_eq!(v.joints, [0; 4]);
        assert_eq!(v.weights, [0.0; 4]);
    }

    #[test]
    fn vertex2d_serde() {
        let v = Vertex2D {
            position: [1.0, 2.0],
            tex_coords: [0.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let json = serde_json::to_string(&v).unwrap();
        let decoded: Vertex2D = serde_json::from_str(&json).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn vertex3d_serde() {
        let v = Vertex3D {
            position: [1.0, 2.0, 3.0],
            normal: [0.0, 1.0, 0.0],
            tex_coords: [0.5, 0.5],
            color: [1.0, 0.0, 0.0, 1.0],
        };
        let json = serde_json::to_string(&v).unwrap();
        let decoded: Vertex3D = serde_json::from_str(&json).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn skinned_vertex3d_serde() {
        let v = SkinnedVertex3D {
            position: [1.0, 2.0, 3.0],
            normal: [0.0, 1.0, 0.0],
            tex_coords: [0.5, 0.5],
            color: [1.0, 0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 1, 2, 3],
            weights: [0.5, 0.3, 0.1, 0.1],
        };
        let json = serde_json::to_string(&v).unwrap();
        let decoded: SkinnedVertex3D = serde_json::from_str(&json).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn vertex2d_batch_bytemuck() {
        let verts = vec![
            Vertex2D {
                position: [0.0, 0.0],
                tex_coords: [0.0, 0.0],
                color: [1.0; 4],
            },
            Vertex2D {
                position: [1.0, 0.0],
                tex_coords: [1.0, 0.0],
                color: [1.0; 4],
            },
            Vertex2D {
                position: [0.0, 1.0],
                tex_coords: [0.0, 1.0],
                color: [1.0; 4],
            },
        ];
        let bytes: &[u8] = bytemuck::cast_slice(&verts);
        assert_eq!(bytes.len(), 32 * 3);
    }

    #[test]
    fn vertex_layout_trait() {
        fn check_layout<T: VertexLayout>() {
            let layout = T::layout();
            assert!(layout.array_stride > 0);
        }
        check_layout::<Vertex2D>();
        check_layout::<Vertex3D>();
        check_layout::<SkinnedVertex3D>();
    }
}
