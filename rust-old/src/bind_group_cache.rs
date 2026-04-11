//! Bind group caching — deduplicates bind group creation by caller-provided key.
//!
//! [`BindGroupCache`] stores `wgpu::BindGroup` instances keyed by a caller-provided
//! `u64` hash. Consumers compute their own cache keys from the bind group descriptor
//! (buffer handles, texture views, sampler config) and call `get_or_insert` to avoid
//! redundant bind group creation.
//!
//! Bind groups that reference reallocated resources (e.g., a [`GrowableBuffer`](crate::buffer::GrowableBuffer)
//! that grew) must be invalidated via [`invalidate`](BindGroupCache::invalidate).

use std::collections::HashMap;

/// Cached bind groups, keyed by descriptor hash.
///
/// The cache does NOT compute hashes — callers provide a `u64` key.
/// This keeps the cache generic and avoids imposing a hashing strategy.
///
/// # Examples
///
/// ```ignore
/// use mabda::bind_group_cache::BindGroupCache;
///
/// let mut cache = BindGroupCache::new();
/// let key = hash_bind_group_descriptor(&layout, &buffers);
/// let bg = cache.get_or_insert(key, || {
///     device.create_bind_group(&descriptor)
/// });
/// ```
pub struct BindGroupCache {
    groups: HashMap<u64, wgpu::BindGroup>,
}

impl BindGroupCache {
    /// Create an empty bind group cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
        }
    }

    /// Get a cached bind group, or create one by calling `create_fn`.
    ///
    /// If the key exists, returns the cached bind group without calling
    /// `create_fn`. The key should encode everything that affects the
    /// bind group: layout, buffer handles, offsets, texture views, samplers.
    pub fn get_or_insert(
        &mut self,
        key: u64,
        create_fn: impl FnOnce() -> wgpu::BindGroup,
    ) -> &wgpu::BindGroup {
        self.groups.entry(key).or_insert_with(|| {
            tracing::debug!(key, "bind group cache: creating bind group");
            create_fn()
        })
    }

    /// Get a cached bind group by key.
    #[must_use]
    pub fn get(&self, key: u64) -> Option<&wgpu::BindGroup> {
        self.groups.get(&key)
    }

    /// Check if a bind group is cached.
    #[must_use]
    #[inline]
    pub fn contains(&self, key: u64) -> bool {
        self.groups.contains_key(&key)
    }

    /// Remove a cached bind group (e.g., after buffer reallocation).
    ///
    /// Returns `true` if the bind group was found and removed.
    pub fn invalidate(&mut self, key: u64) -> bool {
        let removed = self.groups.remove(&key).is_some();
        if removed {
            tracing::debug!(key, "bind group cache: invalidated");
        }
        removed
    }

    /// Remove all bind groups whose keys match a predicate.
    ///
    /// Useful for bulk invalidation when a shared resource (e.g., a buffer
    /// generation counter) changes.
    pub fn invalidate_where(&mut self, predicate: impl Fn(u64) -> bool) {
        let before = self.groups.len();
        self.groups.retain(|k, _| !predicate(*k));
        let removed = before - self.groups.len();
        if removed > 0 {
            tracing::debug!(removed, "bind group cache: bulk invalidation");
        }
    }

    /// Clear all cached bind groups.
    pub fn clear(&mut self) {
        tracing::debug!(count = self.groups.len(), "clearing bind group cache");
        self.groups.clear();
    }

    /// Number of cached bind groups.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

impl Default for BindGroupCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_empty() {
        let cache = BindGroupCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn cache_default() {
        let cache = BindGroupCache::default();
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_contains_miss() {
        let cache = BindGroupCache::new();
        assert!(!cache.contains(42));
        assert!(cache.get(42).is_none());
    }

    #[test]
    fn cache_invalidate_miss() {
        let mut cache = BindGroupCache::new();
        assert!(!cache.invalidate(42));
    }

    #[test]
    fn cache_clear() {
        let mut cache = BindGroupCache::new();
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_types() {
        let _size = std::mem::size_of::<BindGroupCache>();
    }

    fn try_gpu() -> Option<crate::context::GpuContext> {
        pollster::block_on(crate::context::GpuContext::new()).ok()
    }

    #[test]
    fn gpu_insert_and_get() {
        let Some(ctx) = try_gpu() else { return };

        let layout = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("test_layout"),
                entries: &[],
            });

        let mut cache = BindGroupCache::new();
        let _bg = cache.get_or_insert(1, || {
            ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("test_bg"),
                layout: &layout,
                entries: &[],
            })
        });

        assert!(cache.contains(1));
        assert_eq!(cache.len(), 1);
        assert!(cache.get(1).is_some());
    }

    #[test]
    fn gpu_deduplication() {
        let Some(ctx) = try_gpu() else { return };

        let layout = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("test_layout"),
                entries: &[],
            });

        let mut cache = BindGroupCache::new();
        let _bg1 = cache.get_or_insert(1, || {
            ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("first"),
                layout: &layout,
                entries: &[],
            })
        });
        // Second call with same key — should not call create_fn
        let _bg2 = cache.get_or_insert(1, || {
            panic!("should not be called — key already cached");
        });
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn gpu_invalidate() {
        let Some(ctx) = try_gpu() else { return };

        let layout = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("test_layout"),
                entries: &[],
            });

        let mut cache = BindGroupCache::new();
        let _bg = cache.get_or_insert(1, || {
            ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("test_bg"),
                layout: &layout,
                entries: &[],
            })
        });

        assert!(cache.invalidate(1));
        assert!(!cache.contains(1));
        assert!(!cache.invalidate(1));
    }

    #[test]
    fn gpu_invalidate_where() {
        let Some(ctx) = try_gpu() else { return };

        let layout = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("test_layout"),
                entries: &[],
            });

        let mut cache = BindGroupCache::new();
        for key in 0..5 {
            let _bg = cache.get_or_insert(key, || {
                ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("test_bg"),
                    layout: &layout,
                    entries: &[],
                })
            });
        }
        assert_eq!(cache.len(), 5);

        // Invalidate even keys
        cache.invalidate_where(|k| k % 2 == 0);
        assert_eq!(cache.len(), 2); // keys 1, 3 remain
        assert!(!cache.contains(0));
        assert!(cache.contains(1));
        assert!(!cache.contains(2));
        assert!(cache.contains(3));
        assert!(!cache.contains(4));
    }
}
