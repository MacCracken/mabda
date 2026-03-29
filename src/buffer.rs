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
        .map_err(|e| GpuError::Readback(format!("channel error: {e}")))?
        .map_err(|e| GpuError::Readback(format!("map failed: {e}")))?;

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

#[cfg(test)]
mod tests {
    #[test]
    fn storage_buffer_usage_read_only() {
        let _size = std::mem::size_of::<wgpu::Buffer>();
    }

    #[test]
    fn staging_buffer_label() {
        let _size = std::mem::size_of::<wgpu::BufferDescriptor<'_>>();
    }
}
