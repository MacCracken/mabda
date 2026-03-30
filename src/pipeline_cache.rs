//! Pipeline caching — deduplicates GPU pipeline creation by descriptor hash.
//!
//! [`PipelineCache`] stores compiled render and compute pipelines keyed by
//! a caller-provided `u64` hash. Consumers compute their own cache keys
//! from pipeline configuration (shader source, vertex layout, blend state, etc.)
//! and call `get_or_insert` to avoid redundant compilation.

#[cfg(any(feature = "graphics", feature = "compute"))]
use std::collections::HashMap;

/// Cached render and compute pipelines, keyed by descriptor hash.
///
/// The cache does NOT compute hashes itself — callers provide a `u64` key
/// derived from their pipeline configuration. This keeps the cache generic
/// and avoids imposing a specific hashing strategy.
///
/// # Example
///
/// ```ignore
/// let key = hash_my_pipeline_config(&config);
/// let pipeline = cache.get_or_insert_render(key, || {
///     RenderPipelineBuilder::new(device, shader, "vs_main", "fs_main")
///         .color_target(format, None)
///         .build()
///         .unwrap()
/// });
/// ```
pub struct PipelineCache {
    #[cfg(feature = "graphics")]
    render_pipelines: HashMap<u64, crate::render_pipeline::RenderPipeline>,
    #[cfg(feature = "compute")]
    compute_pipelines: HashMap<u64, crate::compute::ComputePipeline>,
}

impl PipelineCache {
    /// Create an empty pipeline cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "graphics")]
            render_pipelines: HashMap::new(),
            #[cfg(feature = "compute")]
            compute_pipelines: HashMap::new(),
        }
    }

    /// Get a cached render pipeline, or insert one by calling `create_fn`.
    ///
    /// The `key` should be a hash of the pipeline descriptor (shader source,
    /// vertex layout, blend state, etc.). If the key exists, returns the
    /// cached pipeline without calling `create_fn`.
    #[cfg(feature = "graphics")]
    pub fn get_or_insert_render(
        &mut self,
        key: u64,
        create_fn: impl FnOnce() -> crate::render_pipeline::RenderPipeline,
    ) -> &crate::render_pipeline::RenderPipeline {
        self.render_pipelines.entry(key).or_insert_with(|| {
            tracing::debug!(key, "pipeline cache: compiling render pipeline");
            create_fn()
        })
    }

    /// Get a cached compute pipeline, or insert one by calling `create_fn`.
    #[cfg(feature = "compute")]
    pub fn get_or_insert_compute(
        &mut self,
        key: u64,
        create_fn: impl FnOnce() -> crate::compute::ComputePipeline,
    ) -> &crate::compute::ComputePipeline {
        self.compute_pipelines.entry(key).or_insert_with(|| {
            tracing::debug!(key, "pipeline cache: compiling compute pipeline");
            create_fn()
        })
    }

    /// Check if a render pipeline is cached.
    #[cfg(feature = "graphics")]
    #[must_use]
    #[inline]
    pub fn contains_render(&self, key: u64) -> bool {
        self.render_pipelines.contains_key(&key)
    }

    /// Check if a compute pipeline is cached.
    #[cfg(feature = "compute")]
    #[must_use]
    #[inline]
    pub fn contains_compute(&self, key: u64) -> bool {
        self.compute_pipelines.contains_key(&key)
    }

    /// Get a cached render pipeline by key.
    #[cfg(feature = "graphics")]
    #[must_use]
    pub fn get_render(&self, key: u64) -> Option<&crate::render_pipeline::RenderPipeline> {
        self.render_pipelines.get(&key)
    }

    /// Get a cached compute pipeline by key.
    #[cfg(feature = "compute")]
    #[must_use]
    pub fn get_compute(&self, key: u64) -> Option<&crate::compute::ComputePipeline> {
        self.compute_pipelines.get(&key)
    }

    /// Remove a cached render pipeline (e.g., after shader hot-reload).
    #[cfg(feature = "graphics")]
    pub fn invalidate_render(&mut self, key: u64) -> bool {
        let removed = self.render_pipelines.remove(&key).is_some();
        if removed {
            tracing::debug!(key, "pipeline cache: invalidated render pipeline");
        }
        removed
    }

    /// Remove a cached compute pipeline.
    #[cfg(feature = "compute")]
    pub fn invalidate_compute(&mut self, key: u64) -> bool {
        let removed = self.compute_pipelines.remove(&key).is_some();
        if removed {
            tracing::debug!(key, "pipeline cache: invalidated compute pipeline");
        }
        removed
    }

    /// Clear all cached pipelines.
    pub fn clear(&mut self) {
        #[cfg(feature = "graphics")]
        self.render_pipelines.clear();
        #[cfg(feature = "compute")]
        self.compute_pipelines.clear();
    }

    /// Total number of cached pipelines (render + compute).
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        let total = 0;
        #[cfg(feature = "graphics")]
        let total = total + self.render_pipelines.len();
        #[cfg(feature = "compute")]
        let total = total + self.compute_pipelines.len();
        total
    }

    /// Whether the cache is empty.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for PipelineCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_empty() {
        let cache = PipelineCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(!cache.contains_render(42));
        assert!(!cache.contains_compute(42));
    }

    #[test]
    fn cache_default() {
        let cache = PipelineCache::default();
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_get_render_none() {
        let cache = PipelineCache::new();
        assert!(cache.get_render(1).is_none());
    }

    #[test]
    fn cache_get_compute_none() {
        let cache = PipelineCache::new();
        assert!(cache.get_compute(1).is_none());
    }

    #[test]
    fn cache_types() {
        let _size = std::mem::size_of::<PipelineCache>();
    }
}
