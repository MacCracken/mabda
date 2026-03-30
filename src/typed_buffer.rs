//! Typed GPU buffer wrappers with alignment enforcement.
//!
//! [`UniformBuffer<T>`] and [`StorageBuffer<T>`] provide type-safe GPU buffer
//! management with automatic byte casting via `bytemuck`. `UniformBuffer`
//! enforces 16-byte alignment (WebGPU `minUniformBufferOffsetAlignment`).
//!
//! For raw byte-level control, use the functions in [`crate::buffer`] instead.

use crate::error::{GpuError, Result};

/// A typed uniform buffer on the GPU.
///
/// `T` must be `bytemuck::Pod` (implies `repr(C)`, `Copy`, `Zeroable`).
/// The size of `T` must be a multiple of 16 bytes to satisfy WebGPU's
/// `minUniformBufferOffsetAlignment` requirement.
///
/// Use the raw [`create_uniform_buffer`](crate::buffer::create_uniform_buffer)
/// function if you need to bypass alignment validation.
pub struct UniformBuffer<T: bytemuck::Pod> {
    buffer: wgpu::Buffer,
    _marker: std::marker::PhantomData<T>,
}

impl<T: bytemuck::Pod> UniformBuffer<T> {
    /// Create a uniform buffer initialized with `data`.
    ///
    /// Returns `Err(GpuError::Buffer)` if `size_of::<T>()` is not a
    /// multiple of 16 bytes.
    #[must_use = "GPU buffer allocated but not used"]
    pub fn new(device: &wgpu::Device, data: &T, label: &str) -> Result<Self> {
        let size = std::mem::size_of::<T>();
        if size == 0 {
            return Err(GpuError::Buffer("uniform buffer type has zero size".into()));
        }
        if !size.is_multiple_of(16) {
            return Err(GpuError::Buffer(format!(
                "uniform buffer type size ({size} bytes) must be a multiple of 16"
            )));
        }
        tracing::debug!(label, size, "creating typed uniform buffer");
        let buffer = crate::buffer::create_uniform_buffer(device, bytemuck::bytes_of(data), label);
        Ok(Self {
            buffer,
            _marker: std::marker::PhantomData,
        })
    }

    /// Write new data to the buffer.
    #[inline]
    pub fn write(&self, queue: &wgpu::Queue, data: &T) {
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(data));
    }

    /// Get the underlying wgpu buffer for bind group creation.
    #[must_use]
    #[inline]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

/// A typed storage buffer on the GPU.
///
/// `T` must be `bytemuck::Pod`. Storage buffers have no alignment
/// restriction beyond the element size itself (std430 rules).
pub struct StorageBuffer<T: bytemuck::Pod> {
    buffer: wgpu::Buffer,
    count: usize,
    _marker: std::marker::PhantomData<T>,
}

impl<T: bytemuck::Pod> StorageBuffer<T> {
    /// Create a storage buffer initialized with `data`.
    ///
    /// - `read_only = true`: buffer is read-only in shaders (no `COPY_SRC`).
    /// - `read_only = false`: buffer is read-write with `COPY_SRC` for readback.
    #[must_use = "GPU buffer allocated but not used"]
    pub fn new(device: &wgpu::Device, data: &[T], label: &str, read_only: bool) -> Self {
        tracing::debug!(
            label,
            count = data.len(),
            element_size = std::mem::size_of::<T>(),
            read_only,
            "creating typed storage buffer"
        );
        let buffer = crate::buffer::create_storage_buffer(
            device,
            bytemuck::cast_slice(data),
            label,
            read_only,
        );
        Self {
            buffer,
            count: data.len(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Create an empty storage buffer with capacity for `count` elements.
    #[must_use = "GPU buffer allocated but not used"]
    pub fn empty(device: &wgpu::Device, count: usize, label: &str, read_only: bool) -> Self {
        let size = (count * std::mem::size_of::<T>()) as u64;
        tracing::debug!(
            label,
            count,
            element_size = std::mem::size_of::<T>(),
            read_only,
            "creating empty typed storage buffer"
        );
        let buffer = crate::buffer::create_storage_buffer_empty(device, size, label, read_only);
        Self {
            buffer,
            count,
            _marker: std::marker::PhantomData,
        }
    }

    /// Write new data to the buffer.
    ///
    /// The data length must not exceed the buffer's original capacity
    /// (as set by [`new`](Self::new) or [`empty`](Self::empty)).
    ///
    /// Returns `Err(GpuError::Buffer)` if `data.len()` exceeds capacity.
    pub fn write(&self, queue: &wgpu::Queue, data: &[T]) -> Result<()> {
        if data.len() > self.count {
            return Err(GpuError::Buffer(format!(
                "storage buffer write exceeds capacity: {} > {}",
                data.len(),
                self.count
            )));
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));
        Ok(())
    }

    /// Get the underlying wgpu buffer for bind group creation.
    #[must_use]
    #[inline]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Number of elements the buffer was created with.
    #[must_use]
    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 16-byte aligned struct for uniform buffer tests
    #[repr(C)]
    #[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Aligned16 {
        data: [f32; 4], // 16 bytes
    }

    // 32-byte aligned struct
    #[repr(C)]
    #[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Aligned32 {
        a: [f32; 4],
        b: [f32; 4],
    }

    // 12-byte struct (NOT aligned to 16)
    #[repr(C)]
    #[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Unaligned12 {
        data: [f32; 3], // 12 bytes
    }

    #[test]
    fn uniform_buffer_alignment_check() {
        assert_eq!(std::mem::size_of::<Aligned16>(), 16);
        assert_eq!(std::mem::size_of::<Aligned32>(), 32);
        assert_eq!(std::mem::size_of::<Unaligned12>(), 12);
        // Verify alignment requirement: 16 and 32 pass, 12 does not
        assert!(std::mem::size_of::<Aligned16>().is_multiple_of(16));
        assert!(std::mem::size_of::<Aligned32>().is_multiple_of(16));
        assert!(!std::mem::size_of::<Unaligned12>().is_multiple_of(16));
    }

    #[test]
    fn storage_buffer_types() {
        let _size = std::mem::size_of::<StorageBuffer<f32>>();
    }

    #[test]
    fn uniform_buffer_types() {
        let _size = std::mem::size_of::<UniformBuffer<Aligned16>>();
    }

    #[test]
    fn storage_buffer_phantom_data() {
        // Verify PhantomData doesn't affect struct size
        assert_eq!(
            std::mem::size_of::<StorageBuffer<f32>>(),
            std::mem::size_of::<StorageBuffer<[f32; 4]>>()
        );
    }
}
