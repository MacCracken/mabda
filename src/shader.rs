//! Shader module caching.
//!
//! [`ShaderCache`] deduplicates `wgpu::ShaderModule` creation by hashing
//! the WGSL source. Identical shaders used by multiple pipelines share
//! a single compiled module.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Cache of compiled shader modules, keyed by source hash.
///
/// Prevents redundant shader compilation when the same WGSL source is
/// used by multiple pipelines.
pub struct ShaderCache {
    modules: HashMap<u64, wgpu::ShaderModule>,
}

impl ShaderCache {
    /// Create an empty shader cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    /// Get or compile a shader module from WGSL source.
    ///
    /// Returns a cached module if the same source was previously compiled,
    /// otherwise compiles and caches it.
    pub fn get_or_compile(
        &mut self,
        device: &wgpu::Device,
        wgsl_source: &str,
        label: &str,
    ) -> &wgpu::ShaderModule {
        let hash = Self::hash_source(wgsl_source);

        self.modules.entry(hash).or_insert_with(|| {
            tracing::debug!(label, hash, "compiling and caching shader module");
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
            })
        })
    }

    /// Check if a shader with the given source is already cached.
    #[must_use]
    pub fn contains(&self, wgsl_source: &str) -> bool {
        self.modules.contains_key(&Self::hash_source(wgsl_source))
    }

    /// Remove a cached shader (e.g., for hot-reload invalidation).
    ///
    /// Returns `true` if the shader was found and removed.
    pub fn invalidate(&mut self, wgsl_source: &str) -> bool {
        let hash = Self::hash_source(wgsl_source);
        let removed = self.modules.remove(&hash).is_some();
        if removed {
            tracing::debug!(hash, "shader module invalidated");
        }
        removed
    }

    /// Clear all cached shader modules.
    pub fn clear(&mut self) {
        tracing::debug!(count = self.modules.len(), "clearing shader cache");
        self.modules.clear();
    }

    /// Number of cached shader modules.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    fn hash_source(source: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for ShaderCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_empty() {
        let cache = ShaderCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn cache_default() {
        let cache = ShaderCache::default();
        assert!(cache.is_empty());
    }

    #[test]
    fn hash_deterministic() {
        let source = "@compute @workgroup_size(64) fn main() {}";
        let h1 = ShaderCache::hash_source(source);
        let h2 = ShaderCache::hash_source(source);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_different_sources() {
        let h1 = ShaderCache::hash_source("fn a() {}");
        let h2 = ShaderCache::hash_source("fn b() {}");
        assert_ne!(h1, h2);
    }

    #[test]
    fn contains_before_compile() {
        let cache = ShaderCache::new();
        assert!(!cache.contains("fn main() {}"));
    }
}
