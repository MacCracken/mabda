//! GPU device and surface management.
//!
//! [`GpuContext`] is the central handle for all GPU operations. It owns the
//! wgpu instance, adapter, device, and queue. Create one per application and
//! share it across subsystems.
//!
//! Use [`GpuContextBuilder`] for custom device features, limits, power
//! preference, or device lost callbacks. The convenience constructors
//! [`GpuContext::new`] and [`GpuContext::new_for_surface`] use defaults.

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
    ///
    /// For custom features, limits, or power preference, use
    /// [`GpuContextBuilder`] instead.
    pub async fn new() -> Result<Self> {
        GpuContextBuilder::new().build().await
    }

    /// Request a GPU context compatible with the given surface.
    ///
    /// Ensures the adapter can present to this surface, so the context
    /// is usable for both rendering and compute.
    pub async fn new_for_surface(surface: &wgpu::Surface<'_>) -> Result<Self> {
        GpuContextBuilder::new()
            .compatible_surface(surface)
            .build()
            .await
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

/// Builder for GPU context creation with custom configuration.
///
/// Use this when you need specific device features, limits, power preference,
/// or device lost callbacks. For defaults, use [`GpuContext::new`] instead.
///
/// # Example
///
/// ```ignore
/// let ctx = GpuContextBuilder::new()
///     .power_preference(wgpu::PowerPreference::LowPower)
///     .features(wgpu::Features::TIMESTAMP_QUERY)
///     .build()
///     .await?;
/// ```
pub struct GpuContextBuilder<'a> {
    power_preference: wgpu::PowerPreference,
    features: wgpu::Features,
    limits: wgpu::Limits,
    compatible_surface: Option<&'a wgpu::Surface<'a>>,
    device_lost_callback: Option<Box<dyn Fn(wgpu::DeviceLostReason, String) + Send + 'static>>,
}

impl Default for GpuContextBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> GpuContextBuilder<'a> {
    /// Create a new builder with default settings.
    ///
    /// Defaults: `HighPerformance`, no extra features, default limits,
    /// no surface, no device lost callback.
    #[must_use]
    pub fn new() -> Self {
        Self {
            power_preference: wgpu::PowerPreference::HighPerformance,
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
            compatible_surface: None,
            device_lost_callback: None,
        }
    }

    /// Set the power preference for adapter selection.
    #[must_use]
    pub fn power_preference(mut self, pref: wgpu::PowerPreference) -> Self {
        self.power_preference = pref;
        self
    }

    /// Request specific device features (e.g., `TIMESTAMP_QUERY`).
    ///
    /// Device creation will fail if the adapter does not support
    /// the requested features.
    #[must_use]
    pub fn features(mut self, features: wgpu::Features) -> Self {
        self.features = features;
        self
    }

    /// Set device limits (e.g., max buffer sizes, bind groups).
    #[must_use]
    pub fn limits(mut self, limits: wgpu::Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Require the adapter to be compatible with this surface.
    #[must_use]
    pub fn compatible_surface(mut self, surface: &'a wgpu::Surface<'a>) -> Self {
        self.compatible_surface = Some(surface);
        self
    }

    /// Set a callback invoked when the device is lost (driver crash, GPU reset).
    ///
    /// The callback receives the reason and a human-readable message.
    /// Use this to trigger resource recreation or graceful shutdown.
    pub fn device_lost_callback(
        mut self,
        callback: impl Fn(wgpu::DeviceLostReason, String) + Send + 'static,
    ) -> Self {
        self.device_lost_callback = Some(Box::new(callback));
        self
    }

    /// Build the GPU context.
    ///
    /// Returns `Err(GpuError::AdapterNotFound)` if no suitable adapter exists,
    /// or `Err(GpuError::DeviceRequest)` if the device cannot be created with
    /// the requested features/limits.
    pub async fn build(self) -> Result<GpuContext> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        desc.backends = wgpu::Backends::all();
        let instance = wgpu::Instance::new(desc);

        tracing::debug!(
            ?self.power_preference,
            "requesting GPU adapter"
        );
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: self.power_preference,
                compatible_surface: self.compatible_surface,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| {
                tracing::error!("no suitable GPU adapter found");
                GpuError::AdapterNotFound
            })?;

        let device_desc = wgpu::DeviceDescriptor {
            label: Some("mabda_device"),
            required_features: self.features,
            required_limits: self.limits,
            ..Default::default()
        };

        let (device, queue) =
            adapter
                .request_device(&device_desc)
                .await
                .map_err(|e: wgpu::RequestDeviceError| {
                    tracing::error!("GPU device request failed: {e}");
                    GpuError::DeviceRequest(e)
                })?;

        if let Some(callback) = self.device_lost_callback {
            device.set_device_lost_callback(callback);
        }

        tracing::info!(
            adapter = adapter.get_info().name,
            backend = ?adapter.get_info().backend,
            "GPU context initialized"
        );

        Ok(GpuContext {
            instance,
            adapter,
            device,
            queue,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_context_types() {
        let _size = std::mem::size_of::<GpuContext>();
    }

    #[test]
    fn builder_defaults() {
        let builder = GpuContextBuilder::new();
        assert_eq!(
            builder.power_preference,
            wgpu::PowerPreference::HighPerformance
        );
        assert_eq!(builder.features, wgpu::Features::empty());
        assert!(builder.compatible_surface.is_none());
        assert!(builder.device_lost_callback.is_none());
    }

    #[test]
    fn builder_chaining() {
        let builder = GpuContextBuilder::new()
            .power_preference(wgpu::PowerPreference::LowPower)
            .features(wgpu::Features::TIMESTAMP_QUERY);
        assert_eq!(builder.power_preference, wgpu::PowerPreference::LowPower);
        assert!(builder.features.contains(wgpu::Features::TIMESTAMP_QUERY));
    }

    #[test]
    fn builder_default_trait() {
        let builder = GpuContextBuilder::default();
        assert_eq!(
            builder.power_preference,
            wgpu::PowerPreference::HighPerformance
        );
    }
}
