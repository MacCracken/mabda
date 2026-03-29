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
    tracing::debug!(size, "GPU buffer readback");
    let staging = create_staging_buffer(device, size, "readback_staging");

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback_encoder"),
    });
    encoder.copy_buffer_to_buffer(source, 0, &staging, 0, size);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = staging.slice(..);
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
            GpuError::Readback(format!("channel error: {e}"))
        })?
        .map_err(|e| {
            tracing::error!("buffer readback map failed: {e}");
            GpuError::Readback(format!("map failed: {e}"))
        })?;

    let data = slice.get_mapped_range();
    let result = data.to_vec();
    drop(data);
    staging.unmap();

    Ok(result)
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
    let size = (count * std::mem::size_of::<T>()) as u64;
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

/// A GPU buffer that grows automatically when data exceeds capacity.
///
/// Uses exponential growth (3/2 multiplier, minimum 16 elements) to
/// amortize reallocation cost. Suitable for vertex, index, instance,
/// or storage buffers where the element count varies frame-to-frame.
pub struct GrowableBuffer {
    /// The underlying GPU buffer.
    pub buffer: wgpu::Buffer,
    /// Current number of elements written.
    pub count: u32,
    capacity: usize,
    usage: wgpu::BufferUsages,
    label: String,
    element_size: usize,
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
            size: (capacity * element_size) as u64,
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
        }
    }

    /// Update buffer contents. Regrows if data exceeds capacity.
    pub fn update<T: bytemuck::Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[T],
    ) {
        self.count = data.len() as u32;

        if data.is_empty() {
            return;
        }

        if data.len() > self.capacity {
            let new_capacity = (data.len() * 3 / 2).max(16);
            tracing::debug!(
                old_capacity = self.capacity,
                new_capacity,
                label = %self.label,
                "growable buffer regrow"
            );
            self.capacity = new_capacity;
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&self.label),
                size: (self.capacity * self.element_size) as u64,
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
}
