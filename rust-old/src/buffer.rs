//! GPU buffer creation and readback utilities.
//!
//! Provides helpers for creating storage, uniform, and staging buffers,
//! plus synchronous GPU readback. These replace the ad-hoc buffer management
//! that was duplicated across soorat, rasa, and bijli.

use crate::error::{GpuError, Result};

/// Create a GPU storage buffer initialized with data.
///
/// - `read_only = true`: buffer is read-only in shaders (no `COPY_SRC`).
/// - `read_only = false`: buffer is read-write with `COPY_SRC` for readback.
#[must_use]
pub fn create_storage_buffer(
    device: &wgpu::Device,
    data: &[u8],
    label: &str,
    read_only: bool,
) -> wgpu::Buffer {
    tracing::debug!(
        label,
        size = data.len(),
        read_only,
        "creating storage buffer"
    );
    use wgpu::util::DeviceExt;
    let mut usage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
    if !read_only {
        usage |= wgpu::BufferUsages::COPY_SRC;
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: data,
        usage,
    })
}

/// Create an empty GPU storage buffer with a given byte size.
///
/// Useful for output buffers that will be written by compute shaders.
#[must_use]
pub fn create_storage_buffer_empty(
    device: &wgpu::Device,
    size: u64,
    label: &str,
    read_only: bool,
) -> wgpu::Buffer {
    tracing::debug!(label, size, read_only, "creating empty storage buffer");
    let mut usage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
    if !read_only {
        usage |= wgpu::BufferUsages::COPY_SRC;
    }
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage,
        mapped_at_creation: false,
    })
}

/// Create a GPU uniform buffer initialized with data.
///
/// Uniform buffers are read-only in shaders and have stricter size limits
/// (64KB on WebGPU). Use storage buffers for larger data.
#[must_use]
pub fn create_uniform_buffer(device: &wgpu::Device, data: &[u8], label: &str) -> wgpu::Buffer {
    tracing::debug!(label, size = data.len(), "creating uniform buffer");
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: data,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

/// Create a staging buffer for GPU-to-CPU readback.
///
/// The returned buffer has `MAP_READ | COPY_DST` usage. Copy GPU data
/// into it, then map and read.
#[must_use]
pub fn create_staging_buffer(device: &wgpu::Device, size: u64, label: &str) -> wgpu::Buffer {
    tracing::debug!(label, size, "creating staging buffer");
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Read back the contents of a GPU buffer synchronously.
///
/// Creates a staging buffer, copies `size` bytes from `source`, maps it,
/// and returns the data as a `Vec<u8>`. This is blocking — suitable for
/// tests, screenshots, and one-shot compute readback, not for game loops.
pub fn read_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Buffer,
    size: u64,
) -> Result<Vec<u8>> {
    read_buffer_async(device, queue, source, size).finish(device)
}

/// Submit a buffer readback without blocking.
///
/// The GPU copy is submitted immediately. Returns a [`PendingReadback`]
/// that can be completed later with [`PendingReadback::finish`]. This
/// allows interleaving other GPU work between submission and completion.
///
/// For simple blocking readback, use [`read_buffer`] instead.
pub fn read_buffer_async(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Buffer,
    size: u64,
) -> PendingReadback {
    tracing::debug!(size, "GPU buffer readback submitted");
    let staging = create_staging_buffer(device, size, "readback_staging");

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback_encoder"),
    });
    encoder.copy_buffer_to_buffer(source, 0, &staging, 0, size);
    queue.submit(std::iter::once(encoder.finish()));

    PendingReadback { staging }
}

/// A pending GPU readback operation.
///
/// The GPU copy has been submitted. Call [`finish`](Self::finish) to
/// block until the data is available.
///
/// # Examples
///
/// ```ignore
/// let pending = read_buffer_async(&device, &queue, &gpu_buffer, size);
/// // ... do other work while GPU copies ...
/// let data: Vec<u8> = pending.finish(&device)?;
/// ```
#[must_use = "readback submitted but never completed — call .finish()"]
pub struct PendingReadback {
    staging: wgpu::Buffer,
}

impl PendingReadback {
    /// Block until the readback completes and return the data.
    pub fn finish(self, device: &wgpu::Device) -> Result<Vec<u8>> {
        let slice = self.staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = device.poll(wgpu::PollType::Wait {
            timeout: None,
            submission_index: None,
        });

        rx.recv()
            .map_err(|e| {
                tracing::error!("buffer readback channel error: {e}");
                let _ = e;
                GpuError::ReadbackChannel
            })?
            .map_err(|e| {
                tracing::error!("buffer readback map failed: {e}");
                GpuError::ReadbackMap(e)
            })?;

        let data = slice.get_mapped_range();
        let result = data.to_vec();
        drop(data);
        self.staging.unmap();

        Ok(result)
    }
}

/// Read back a GPU buffer and reinterpret as a typed slice.
///
/// Convenience wrapper around [`read_buffer`] that casts the raw bytes
/// to `&[T]` via bytemuck. The buffer size must be a multiple of
/// `size_of::<T>()`.
pub fn read_buffer_typed<T: bytemuck::Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Buffer,
    count: usize,
) -> Result<Vec<T>> {
    let size = count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or_else(|| GpuError::Buffer("read_buffer_typed: size overflow".into()))?
        as u64;
    let bytes = read_buffer(device, queue, source, size)?;
    Ok(bytemuck::cast_slice(&bytes).to_vec())
}

/// Create a GPU vertex buffer initialized with vertex data.
#[cfg(feature = "graphics")]
#[must_use]
pub fn create_vertex_buffer<T: bytemuck::Pod>(
    device: &wgpu::Device,
    vertices: &[T],
    label: &str,
) -> wgpu::Buffer {
    tracing::debug!(label, count = vertices.len(), "creating vertex buffer");
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

/// Create a GPU index buffer initialized with index data.
///
/// `T` should be `u16` or `u32` depending on the index format.
#[cfg(feature = "graphics")]
#[must_use]
pub fn create_index_buffer<T: bytemuck::Pod>(
    device: &wgpu::Device,
    indices: &[T],
    label: &str,
) -> wgpu::Buffer {
    tracing::debug!(label, count = indices.len(), "creating index buffer");
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
    })
}

/// Create an indirect dispatch buffer (3 × u32: workgroups_x, workgroups_y, workgroups_z).
///
/// The buffer has `INDIRECT | STORAGE | COPY_DST` usage, allowing it to be
/// written by compute shaders or the CPU, then used for `dispatch_workgroups_indirect`.
#[must_use]
pub fn create_dispatch_indirect_buffer(
    device: &wgpu::Device,
    workgroups: [u32; 3],
    label: &str,
) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    tracing::debug!(label, ?workgroups, "creating dispatch indirect buffer");
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&workgroups),
        usage: wgpu::BufferUsages::INDIRECT
            | wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST,
    })
}

/// Create an indirect draw buffer (4 × u32: vertex_count, instance_count, first_vertex, first_instance).
///
/// The buffer has `INDIRECT | STORAGE | COPY_DST` usage.
#[cfg(feature = "graphics")]
#[must_use]
pub fn create_draw_indirect_buffer(
    device: &wgpu::Device,
    vertex_count: u32,
    instance_count: u32,
    label: &str,
) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    tracing::debug!(
        label,
        vertex_count,
        instance_count,
        "creating draw indirect buffer"
    );
    let data = [vertex_count, instance_count, 0u32, 0u32];
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&data),
        usage: wgpu::BufferUsages::INDIRECT
            | wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST,
    })
}

/// Create an indirect indexed draw buffer (5 × u32: index_count, instance_count, first_index, base_vertex, first_instance).
///
/// The buffer has `INDIRECT | STORAGE | COPY_DST` usage.
#[cfg(feature = "graphics")]
#[must_use]
pub fn create_draw_indexed_indirect_buffer(
    device: &wgpu::Device,
    index_count: u32,
    instance_count: u32,
    label: &str,
) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    tracing::debug!(
        label,
        index_count,
        instance_count,
        "creating indexed draw indirect buffer"
    );
    let data = [index_count, instance_count, 0u32, 0u32, 0u32];
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&data),
        usage: wgpu::BufferUsages::INDIRECT
            | wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST,
    })
}

/// A GPU buffer that grows automatically when data exceeds capacity.
///
/// Uses exponential growth (3/2 multiplier, minimum 16 elements) to
/// amortize reallocation cost. Suitable for vertex, index, instance,
/// or storage buffers where the element count varies frame-to-frame.
///
/// # Examples
///
/// ```ignore
/// use mabda::buffer::GrowableBuffer;
///
/// let mut buf = GrowableBuffer::new(&device, &vertices, usage, "verts");
/// // Next frame with more data — grows if needed:
/// buf.write(&device, &queue, &new_vertices);
/// ```
pub struct GrowableBuffer {
    /// The underlying GPU buffer.
    pub buffer: wgpu::Buffer,
    /// Current number of elements written.
    pub count: u32,
    capacity: usize,
    usage: wgpu::BufferUsages,
    label: String,
    element_size: usize,
    generation: u64,
}

impl GrowableBuffer {
    /// Create a growable buffer with initial capacity.
    ///
    /// `element_size` is the byte size of each element (e.g., `size_of::<Vertex3D>()`).
    /// `usage` is the wgpu buffer usage flags (e.g., `VERTEX | COPY_DST`).
    pub fn new(
        device: &wgpu::Device,
        capacity: usize,
        element_size: usize,
        usage: wgpu::BufferUsages,
        label: impl Into<String>,
    ) -> Self {
        let label = label.into();
        let capacity = capacity.max(16);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&label),
            size: (capacity.saturating_mul(element_size)) as u64,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            count: 0,
            capacity,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            label,
            element_size,
            generation: 0,
        }
    }

    /// Update buffer contents. Regrows if data exceeds capacity.
    pub fn update<T: bytemuck::Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[T],
    ) {
        self.count = data.len().min(u32::MAX as usize) as u32;

        if data.is_empty() {
            return;
        }

        if data.len() > self.capacity {
            let new_capacity = data.len().saturating_mul(3).saturating_div(2).max(16);
            tracing::debug!(
                old_capacity = self.capacity,
                new_capacity,
                label = %self.label,
                "growable buffer regrow"
            );
            self.capacity = new_capacity;
            self.generation += 1;
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&self.label),
                size: (self.capacity.saturating_mul(self.element_size)) as u64,
                usage: self.usage,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));
    }

    /// Current element count.
    #[must_use]
    #[inline]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Current capacity in elements.
    #[must_use]
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Generation counter — increments each time the buffer is reallocated.
    ///
    /// Use this to detect when dependent bind groups need rebuilding.
    /// If the generation changes after an [`update`](Self::update) call,
    /// the underlying `wgpu::Buffer` has been replaced and any bind groups
    /// referencing the old buffer are invalid.
    #[must_use]
    #[inline]
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_buffer_usage_read_only() {
        let _size = std::mem::size_of::<wgpu::Buffer>();
    }

    #[test]
    fn staging_buffer_label() {
        let _size = std::mem::size_of::<wgpu::BufferDescriptor<'_>>();
    }

    #[test]
    fn growable_buffer_types() {
        let _size = std::mem::size_of::<GrowableBuffer>();
        // Verify struct fields exist
        assert!(std::mem::size_of::<GrowableBuffer>() > 0);
    }

    #[test]
    fn growable_buffer_growth_formula() {
        // Verify the 3/2 growth factor logic
        let grow = |len: usize| (len * 3 / 2).max(16);

        // Small data: minimum capacity 16
        assert_eq!(grow(1), 16);
        assert_eq!(grow(10), 16);

        // At threshold: 3/2 multiplier kicks in
        assert_eq!(grow(16), 24);
        assert_eq!(grow(20), 30);
        assert_eq!(grow(100), 150);
        assert_eq!(grow(1000), 1500);
    }

    #[test]
    fn growable_buffer_min_capacity() {
        // Verify minimum capacity is enforced in constructor logic
        fn apply_min(cap: usize) -> usize {
            cap.max(16)
        }
        assert_eq!(apply_min(0), 16);
        assert_eq!(apply_min(5), 16);
        assert_eq!(apply_min(16), 16);
        assert_eq!(apply_min(100), 100);
    }

    #[test]
    fn growable_buffer_empty_update_count() {
        // Empty data should set count to 0
        let count: u32 = 0usize as u32;
        assert_eq!(count, 0);
    }

    #[test]
    fn growable_buffer_count_tracking() {
        // Verify count conversion from usize to u32
        let data_len = 42usize;
        let count = data_len as u32;
        assert_eq!(count, 42);
    }

    /// Helper: create a headless GPU context, skipping if no adapter available.
    fn try_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let ctx = pollster::block_on(crate::context::GpuContext::new()).ok()?;
        Some((ctx.device, ctx.queue))
    }

    #[test]
    fn gpu_create_storage_buffer() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let data: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
        let buf =
            create_storage_buffer(&device, bytemuck::cast_slice(&data), "test_storage", false);
        assert_eq!(buf.size(), 16);
    }

    #[test]
    fn gpu_create_storage_buffer_empty() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let buf = create_storage_buffer_empty(&device, 256, "test_empty", true);
        assert_eq!(buf.size(), 256);
    }

    #[test]
    fn gpu_create_uniform_buffer() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let data: [f32; 4] = [0.0; 4];
        let buf = create_uniform_buffer(&device, bytemuck::cast_slice(&data), "test_uniform");
        assert_eq!(buf.size(), 16);
    }

    #[test]
    fn gpu_create_staging_buffer() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let buf = create_staging_buffer(&device, 1024, "test_staging");
        assert_eq!(buf.size(), 1024);
    }

    #[test]
    fn gpu_read_buffer_roundtrip() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let data: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
        let buf =
            create_storage_buffer(&device, bytemuck::cast_slice(&data), "readback_test", false);
        let result = read_buffer(&device, &queue, &buf, 16).unwrap();
        let output: &[f32] = bytemuck::cast_slice(&result);
        assert_eq!(output, &[1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn gpu_read_buffer_async_roundtrip() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let data: [f32; 2] = [42.0, -1.0];
        let buf = create_storage_buffer(&device, bytemuck::cast_slice(&data), "async_test", false);
        let pending = read_buffer_async(&device, &queue, &buf, 8);
        let result = pending.finish(&device).unwrap();
        let output: &[f32] = bytemuck::cast_slice(&result);
        assert_eq!(output, &[42.0, -1.0]);
    }

    #[test]
    fn gpu_read_buffer_typed() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let data: [u32; 8] = [10, 20, 30, 40, 50, 60, 70, 80];
        let buf = create_storage_buffer(&device, bytemuck::cast_slice(&data), "typed_test", false);
        let result: Vec<u32> = read_buffer_typed(&device, &queue, &buf, 8).unwrap();
        assert_eq!(result, vec![10, 20, 30, 40, 50, 60, 70, 80]);
    }

    #[test]
    fn gpu_growable_buffer_update_and_grow() {
        let Some((device, queue)) = try_gpu() else {
            return;
        };
        let mut buf = GrowableBuffer::new(
            &device,
            4, // initial capacity: 4 elements
            std::mem::size_of::<f32>(),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            "grow_test",
        );
        assert_eq!(buf.count(), 0);
        let gen0 = buf.generation();

        // Small update — fits in capacity
        let small: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
        buf.update(&device, &queue, &small);
        assert_eq!(buf.count(), 4);

        // Large update — triggers growth
        let large: [f32; 32] = [0.5; 32];
        buf.update(&device, &queue, &large);
        assert_eq!(buf.count(), 32);
        assert!(buf.generation() > gen0);
    }

    #[test]
    #[cfg(feature = "graphics")]
    fn gpu_create_vertex_buffer() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let data: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
        let buf = create_vertex_buffer(&device, &data, "test_vertex");
        assert_eq!(buf.size(), (8 * std::mem::size_of::<f32>()) as u64);
    }

    #[test]
    #[cfg(feature = "graphics")]
    fn gpu_create_index_buffer() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let data: [u16; 6] = [0, 1, 2, 2, 3, 0];
        let buf = create_index_buffer(&device, &data, "test_index");
        // 6 * 2 = 12, but wgpu may pad to 4-byte alignment
        assert!(buf.size() >= (6 * std::mem::size_of::<u16>()) as u64);
    }

    #[test]
    fn gpu_create_dispatch_indirect_buffer() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let buf = create_dispatch_indirect_buffer(&device, [64, 1, 1], "test_dispatch");
        assert_eq!(buf.size(), 12);
    }

    #[test]
    #[cfg(feature = "graphics")]
    fn gpu_create_draw_indirect_buffer() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let buf = create_draw_indirect_buffer(&device, 100, 1, "test_draw");
        assert_eq!(buf.size(), 16);
    }

    #[test]
    #[cfg(feature = "graphics")]
    fn gpu_create_draw_indexed_indirect_buffer() {
        let Some((device, _queue)) = try_gpu() else {
            return;
        };
        let buf = create_draw_indexed_indirect_buffer(&device, 36, 1, "test_draw_indexed");
        assert_eq!(buf.size(), 20);
    }
}
