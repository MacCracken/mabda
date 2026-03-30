//! Instanced rendering — per-instance transform and color data.
//!
//! [`InstanceData`] provides a standard per-instance layout (model matrix + color tint)
//! compatible with the vertex attribute locations used by AGNOS renderers.
//! [`InstanceBuffer`] wraps a GPU buffer with dynamic growth for frame-varying instance counts.

/// Per-instance data: model matrix (column-major 4x4) + color tint.
///
/// 80 bytes total. Vertex step mode is `Instance`, using shader locations 7–11.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    /// Model matrix (column-major 4x4).
    pub model: [f32; 16],
    /// Color tint (RGBA, multiplied with vertex/texture color).
    pub color: [f32; 4],
}

/// Column-major 4x4 identity matrix.
const IDENTITY_MAT4: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, // col 0
    0.0, 1.0, 0.0, 0.0, // col 1
    0.0, 0.0, 1.0, 0.0, // col 2
    0.0, 0.0, 0.0, 1.0, // col 3
];

impl Default for InstanceData {
    fn default() -> Self {
        Self {
            model: IDENTITY_MAT4,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

impl InstanceData {
    /// Create instance data from a translation.
    #[must_use]
    pub fn from_translation(x: f32, y: f32, z: f32) -> Self {
        let mut model = IDENTITY_MAT4;
        model[12] = x;
        model[13] = y;
        model[14] = z;
        Self {
            model,
            ..Default::default()
        }
    }

    /// wgpu vertex buffer layout for instance data (step mode = Instance).
    ///
    /// Shader locations 7–10: model matrix (4 × vec4 columns).
    /// Shader location 11: color tint (vec4).
    #[must_use]
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // model matrix columns at locations 7-10
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // color tint at location 11
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Instance buffer — holds per-instance data on the GPU with dynamic growth.
///
/// Uses the same exponential growth strategy as [`GrowableBuffer`](crate::buffer::GrowableBuffer).
pub struct InstanceBuffer {
    pub buffer: wgpu::Buffer,
    pub count: u32,
    capacity: usize,
}

impl InstanceBuffer {
    /// Create an instance buffer from a slice of instance data.
    #[must_use = "GPU buffer allocated but not used"]
    pub fn new(device: &wgpu::Device, instances: &[InstanceData]) -> Self {
        use wgpu::util::DeviceExt;
        tracing::debug!(count = instances.len(), "creating instance buffer");
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("instance_buffer"),
            contents: bytemuck::cast_slice(instances),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            buffer,
            count: instances.len() as u32,
            capacity: instances.len(),
        }
    }

    /// Create an empty instance buffer with pre-allocated capacity.
    #[must_use = "GPU buffer allocated but not used"]
    pub fn with_capacity(device: &wgpu::Device, capacity: usize) -> Self {
        let capacity = capacity.max(16);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_buffer"),
            size: (capacity * std::mem::size_of::<InstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            count: 0,
            capacity,
        }
    }

    /// Update instance data. Regrows buffer if needed (3/2 exponential growth).
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[InstanceData],
    ) {
        if instances.is_empty() {
            self.count = 0;
            return;
        }

        if instances.len() > self.capacity {
            self.capacity = (instances.len() * 3 / 2).max(16);
            tracing::debug!(
                old_count = self.count,
                new_capacity = self.capacity,
                "instance buffer regrow"
            );
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("instance_buffer"),
                size: (self.capacity * std::mem::size_of::<InstanceData>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(instances));
        self.count = instances.len() as u32;
    }

    /// Current instance count.
    #[must_use]
    #[inline]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Current capacity in instances.
    #[must_use]
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_data_size() {
        assert_eq!(std::mem::size_of::<InstanceData>(), 80);
    }

    #[test]
    fn instance_data_default() {
        let d = InstanceData::default();
        assert_eq!(d.model[0], 1.0);
        assert_eq!(d.model[5], 1.0);
        assert_eq!(d.model[10], 1.0);
        assert_eq!(d.model[15], 1.0);
        assert_eq!(d.color, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn instance_data_translation() {
        let d = InstanceData::from_translation(5.0, 3.0, 1.0);
        assert_eq!(d.model[12], 5.0);
        assert_eq!(d.model[13], 3.0);
        assert_eq!(d.model[14], 1.0);
    }

    #[test]
    fn instance_data_layout() {
        let layout = InstanceData::layout();
        assert_eq!(layout.array_stride, 80);
        assert_eq!(layout.attributes.len(), 5);
        assert_eq!(layout.step_mode, wgpu::VertexStepMode::Instance);
    }

    #[test]
    fn instance_data_layout_offsets() {
        let layout = InstanceData::layout();
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[1].offset, 16);
        assert_eq!(layout.attributes[2].offset, 32);
        assert_eq!(layout.attributes[3].offset, 48);
        assert_eq!(layout.attributes[4].offset, 64);
    }

    #[test]
    fn instance_data_layout_locations() {
        let layout = InstanceData::layout();
        assert_eq!(layout.attributes[0].shader_location, 7);
        assert_eq!(layout.attributes[4].shader_location, 11);
    }

    #[test]
    fn instance_data_bytemuck() {
        let d = InstanceData::default();
        let bytes = bytemuck::bytes_of(&d);
        assert_eq!(bytes.len(), 80);
    }

    #[test]
    fn instance_data_batch_cast() {
        let instances = vec![
            InstanceData::from_translation(0.0, 0.0, 0.0),
            InstanceData::from_translation(1.0, 0.0, 0.0),
            InstanceData::from_translation(2.0, 0.0, 0.0),
        ];
        let bytes: &[u8] = bytemuck::cast_slice(&instances);
        assert_eq!(bytes.len(), 80 * 3);
    }

    #[test]
    fn instance_buffer_types() {
        let _size = std::mem::size_of::<InstanceBuffer>();
    }

    #[test]
    fn identity_matrix_is_correct() {
        let m = IDENTITY_MAT4;
        for i in 0..4 {
            for j in 0..4 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert_eq!(m[j * 4 + i], expected, "mat[{i}][{j}] wrong");
            }
        }
    }
}
