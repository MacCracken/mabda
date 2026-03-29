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
    #[error("GPU device request failed: {0}")]
    DeviceRequest(String),

    /// Surface configuration or presentation error.
    #[error("surface configuration failed: {0}")]
    SurfaceConfig(String),

    /// Surface texture acquisition failed (timeout, lost, outdated).
    #[error("surface texture acquisition failed: {0}")]
    SurfaceTexture(String),

    /// Shader compilation error.
    #[error("shader compilation failed: {0}")]
    Shader(String),

    /// Pipeline creation error.
    #[error("pipeline creation failed: {0}")]
    Pipeline(String),

    /// Texture creation or upload error.
    #[error("texture error: {0}")]
    Texture(String),

    /// Buffer operation error (map, readback, size mismatch).
    #[error("buffer error: {0}")]
    Buffer(String),

    /// GPU readback failed (mapping or channel error).
    #[error("GPU readback failed: {0}")]
    Readback(String),

    /// Transparent wrapper for downstream error types.
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
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
    fn error_variants() {
        let errors = vec![
            GpuError::AdapterNotFound,
            GpuError::DeviceRequest("test".into()),
            GpuError::SurfaceConfig("test".into()),
            GpuError::SurfaceTexture("test".into()),
            GpuError::Shader("test".into()),
            GpuError::Pipeline("test".into()),
            GpuError::Texture("test".into()),
            GpuError::Buffer("test".into()),
            GpuError::Readback("test".into()),
        ];
        for err in &errors {
            assert!(!err.to_string().is_empty());
        }
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
}
