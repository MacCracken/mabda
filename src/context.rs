//! GPU device and surface management.
//!
//! [`GpuContext`] is the central handle for all GPU operations. It owns the
//! wgpu instance, adapter, device, and queue. Create one per application and
//! share it across subsystems.

use crate::error::{GpuError, Result};

/// Holds the wgpu device, queue, adapter, and instance.
///
/// This is the shared GPU foundation. Every consumer (rendering, compute,
/// image processing) takes a `&GpuContext` rather than managing its own
/// device lifecycle.
pub struct GpuContext {
    /// The wgpu instance (entry point for adapter/surface creation).
    pub instance: wgpu::Instance,
    /// The selected GPU adapter.
    pub adapter: wgpu::Adapter,
    /// The logical device handle.
    pub device: wgpu::Device,
    /// The command submission queue.
    pub queue: wgpu::Queue,
}

impl GpuContext {
    /// Request a GPU context (adapter + device + queue) without a surface.
    ///
    /// Suitable for headless compute workloads. The adapter may not support
    /// presentation — use [`new_for_surface`](Self::new_for_surface) when
    /// rendering to a window.
    pub async fn new() -> Result<Self> {
        Self::new_inner(None).await
    }

    /// Request a GPU context compatible with the given surface.
    ///
    /// Ensures the adapter can present to this surface, so the context
    /// is usable for both rendering and compute.
    pub async fn new_for_surface(surface: &wgpu::Surface<'_>) -> Result<Self> {
        Self::new_inner(Some(surface)).await
    }

    async fn new_inner(compatible_surface: Option<&wgpu::Surface<'_>>) -> Result<Self> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        desc.backends = wgpu::Backends::all();
        let instance = wgpu::Instance::new(desc);

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| GpuError::AdapterNotFound)?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e: wgpu::RequestDeviceError| GpuError::DeviceRequest(e.to_string()))?;

        tracing::info!(
            adapter = adapter.get_info().name,
            backend = ?adapter.get_info().backend,
            "GPU context initialized"
        );

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// Get adapter info (name, backend, vendor, etc.).
    #[must_use]
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.adapter.get_info()
    }

    /// Get device limits.
    #[must_use]
    pub fn limits(&self) -> wgpu::Limits {
        self.device.limits()
    }

    /// Get device features.
    #[must_use]
    pub fn features(&self) -> wgpu::Features {
        self.device.features()
    }

    /// Poll the device (process completed work, map callbacks, etc.).
    ///
    /// Call this after submitting GPU work if you need results synchronously.
    pub fn poll_wait(&self) {
        let _ = self.device.poll(wgpu::PollType::Wait {
            timeout: None,
            submission_index: None,
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gpu_context_types() {
        let _size = std::mem::size_of::<super::GpuContext>();
    }
}
