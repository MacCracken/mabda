//! Error types for mabda GPU operations.

use thiserror::Error;

/// Errors produced by mabda GPU operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GpuError {
    /// No suitable GPU adapter found on this system.
    #[error("GPU adapter not found")]
    AdapterNotFound,

    /// Device request failed (features, limits, or driver issue).
    #[error("GPU device request failed")]
    DeviceRequest(#[source] wgpu::RequestDeviceError),

    /// Surface configuration or presentation error.
    #[error("surface configuration failed: {0}")]
    SurfaceConfig(String),

    /// Surface texture acquisition timed out (recoverable — retry).
    #[error("surface texture acquisition timed out")]
    SurfaceTimeout,

    /// Surface texture outdated — resize needed (recoverable).
    #[error("surface texture outdated — resize needed")]
    SurfaceOutdated,

    /// Surface lost — full reconfiguration required (fatal).
    #[error("surface lost — reconfiguration required")]
    SurfaceLost,

    /// Shader compilation error.
    #[error("shader compilation failed: {0}")]
    Shader(String),

    /// Pipeline creation error.
    #[error("pipeline creation failed: {0}")]
    Pipeline(String),

    /// Texture creation or upload error.
    #[error("texture error: {0}")]
    Texture(String),

    /// Image decoding failed (PNG/JPEG loading).
    #[cfg(feature = "image")]
    #[error("image decoding failed")]
    ImageDecode(#[source] image::ImageError),

    /// Buffer operation error (size mismatch, alignment, etc.).
    #[error("buffer error: {0}")]
    Buffer(String),

    /// GPU readback buffer mapping failed.
    #[error("GPU readback mapping failed")]
    ReadbackMap(#[source] wgpu::BufferAsyncError),

    /// GPU readback channel closed before result arrived.
    #[error("GPU readback channel closed")]
    ReadbackChannel,

    /// Compute dispatch workgroup count exceeds device limit.
    #[error("dispatch workgroup count {actual} exceeds device limit {limit} on {axis} axis")]
    WorkgroupLimitExceeded {
        axis: &'static str,
        actual: u32,
        limit: u32,
    },

    /// Texture dimension exceeds device limit.
    #[error("texture dimension {actual} exceeds device limit {limit}")]
    TextureDimensionExceeded { actual: u32, limit: u32 },

    /// Transparent wrapper for downstream error types.
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl GpuError {
    /// Whether this error is potentially recoverable (retry or reconfigure may help).
    ///
    /// Recoverable errors include surface timeouts, outdated surfaces, and
    /// surface configuration issues. Fatal errors (device lost, adapter not found,
    /// readback failures) require more drastic recovery.
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::SurfaceTimeout | Self::SurfaceOutdated | Self::SurfaceConfig(_)
        )
    }
}

/// Convenience alias for `std::result::Result<T, GpuError>`.
pub type Result<T> = std::result::Result<T, GpuError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = GpuError::AdapterNotFound;
        assert_eq!(err.to_string(), "GPU adapter not found");
    }

    #[test]
    fn error_surface_variants() {
        let timeout = GpuError::SurfaceTimeout;
        assert!(timeout.to_string().contains("timed out"));

        let outdated = GpuError::SurfaceOutdated;
        assert!(outdated.to_string().contains("outdated"));

        let lost = GpuError::SurfaceLost;
        assert!(lost.to_string().contains("lost"));
    }

    #[test]
    fn error_is_recoverable() {
        assert!(GpuError::SurfaceTimeout.is_recoverable());
        assert!(GpuError::SurfaceOutdated.is_recoverable());
        assert!(GpuError::SurfaceConfig("test".into()).is_recoverable());

        assert!(!GpuError::AdapterNotFound.is_recoverable());
        assert!(!GpuError::SurfaceLost.is_recoverable());
        assert!(!GpuError::ReadbackChannel.is_recoverable());
        assert!(!GpuError::Shader("test".into()).is_recoverable());
    }

    #[test]
    fn error_readback_variants() {
        let channel = GpuError::ReadbackChannel;
        assert!(channel.to_string().contains("channel"));

        let map = GpuError::ReadbackMap(wgpu::BufferAsyncError);
        assert!(map.to_string().contains("mapping"));
    }

    #[test]
    fn error_workgroup_limit() {
        let err = GpuError::WorkgroupLimitExceeded {
            axis: "x",
            actual: 100_000,
            limit: 65535,
        };
        let s = err.to_string();
        assert!(s.contains("100000"));
        assert!(s.contains("65535"));
        assert!(s.contains("x"));
    }

    #[test]
    fn error_texture_dimension() {
        let err = GpuError::TextureDimensionExceeded {
            actual: 32768,
            limit: 16384,
        };
        assert!(err.to_string().contains("32768"));
    }

    #[test]
    fn error_other_variant() {
        let inner: Box<dyn std::error::Error + Send + Sync> = "custom error".into();
        let err = GpuError::Other(inner);
        assert!(err.to_string().contains("custom error"));
    }

    #[test]
    fn error_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<GpuError>();
        assert_sync::<GpuError>();
    }

    #[test]
    fn error_source_chain() {
        use std::error::Error;

        // ReadbackMap should have a source
        let map = GpuError::ReadbackMap(wgpu::BufferAsyncError);
        assert!(map.source().is_some());

        // Unit variants should not
        assert!(GpuError::AdapterNotFound.source().is_none());
        assert!(GpuError::ReadbackChannel.source().is_none());
        assert!(GpuError::SurfaceTimeout.source().is_none());
    }
}
