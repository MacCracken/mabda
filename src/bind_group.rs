//! Bind group layout builder with preset entry helpers.
//!
//! [`BindGroupLayoutBuilder`] provides a fluent API for constructing
//! `wgpu::BindGroupLayoutDescriptor` entries without manually specifying
//! all fields of `wgpu::BindGroupLayoutEntry`.

/// Fluent builder for `wgpu::BindGroupLayout` creation.
///
/// Provides preset methods for common binding types (uniform buffer,
/// storage buffer, texture, sampler) with sensible defaults.
///
/// # Example
///
/// ```ignore
/// let layout = BindGroupLayoutBuilder::new()
///     .uniform_buffer(wgpu::ShaderStages::VERTEX_FRAGMENT)
///     .texture_2d(wgpu::ShaderStages::FRAGMENT)
///     .sampler(wgpu::ShaderStages::FRAGMENT)
///     .build(device, "material_layout");
/// ```
pub struct BindGroupLayoutBuilder {
    entries: Vec<wgpu::BindGroupLayoutEntry>,
}

impl BindGroupLayoutBuilder {
    /// Create a new empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a uniform buffer binding.
    #[must_use]
    pub fn uniform_buffer(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.entries.push(wgpu::BindGroupLayoutEntry {
            binding: self.entries.len() as u32,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        self
    }

    /// Add a uniform buffer binding with dynamic offset support.
    #[must_use]
    pub fn uniform_buffer_dynamic(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.entries.push(wgpu::BindGroupLayoutEntry {
            binding: self.entries.len() as u32,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: None,
            },
            count: None,
        });
        self
    }

    /// Add a storage buffer binding (read-only).
    #[must_use]
    pub fn storage_buffer_readonly(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.entries.push(wgpu::BindGroupLayoutEntry {
            binding: self.entries.len() as u32,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        self
    }

    /// Add a storage buffer binding (read-write).
    #[must_use]
    pub fn storage_buffer(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.entries.push(wgpu::BindGroupLayoutEntry {
            binding: self.entries.len() as u32,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        self
    }

    /// Add a 2D texture binding (float, filterable).
    #[must_use]
    pub fn texture_2d(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.entries.push(wgpu::BindGroupLayoutEntry {
            binding: self.entries.len() as u32,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        self
    }

    /// Add a cube texture binding (float, filterable).
    #[must_use]
    pub fn texture_cube(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.entries.push(wgpu::BindGroupLayoutEntry {
            binding: self.entries.len() as u32,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::Cube,
                multisampled: false,
            },
            count: None,
        });
        self
    }

    /// Add a depth texture binding (for sampling depth in shaders).
    #[must_use]
    pub fn texture_depth_2d(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.entries.push(wgpu::BindGroupLayoutEntry {
            binding: self.entries.len() as u32,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        self
    }

    /// Add a filtering sampler binding.
    #[must_use]
    pub fn sampler(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.entries.push(wgpu::BindGroupLayoutEntry {
            binding: self.entries.len() as u32,
            visibility,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        self
    }

    /// Add a comparison sampler binding (for shadow maps).
    #[must_use]
    pub fn comparison_sampler(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.entries.push(wgpu::BindGroupLayoutEntry {
            binding: self.entries.len() as u32,
            visibility,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
            count: None,
        });
        self
    }

    /// Add a custom entry for binding types not covered by presets.
    ///
    /// Unlike preset methods, this does NOT auto-set the `binding` index.
    /// You must set `entry.binding` manually to the correct value.
    #[must_use]
    pub fn custom(mut self, entry: wgpu::BindGroupLayoutEntry) -> Self {
        self.entries.push(entry);
        self
    }

    /// Get the accumulated entries (for use with compute/render pipeline builders).
    #[must_use]
    pub fn entries(&self) -> &[wgpu::BindGroupLayoutEntry] {
        &self.entries
    }

    /// Consume the builder and return the entries as a Vec.
    #[must_use]
    pub fn into_entries(self) -> Vec<wgpu::BindGroupLayoutEntry> {
        self.entries
    }

    /// Build the bind group layout on the device.
    #[must_use]
    pub fn build(self, device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
        tracing::debug!(
            label,
            bindings = self.entries.len(),
            "creating bind group layout"
        );
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &self.entries,
        })
    }
}

impl Default for BindGroupLayoutBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_empty() {
        let builder = BindGroupLayoutBuilder::new();
        assert!(builder.entries().is_empty());
    }

    #[test]
    fn builder_auto_binding_index() {
        let builder = BindGroupLayoutBuilder::new()
            .uniform_buffer(wgpu::ShaderStages::VERTEX)
            .texture_2d(wgpu::ShaderStages::FRAGMENT)
            .sampler(wgpu::ShaderStages::FRAGMENT);
        let entries = builder.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].binding, 0);
        assert_eq!(entries[1].binding, 1);
        assert_eq!(entries[2].binding, 2);
    }

    #[test]
    fn builder_storage_variants() {
        let builder = BindGroupLayoutBuilder::new()
            .storage_buffer(wgpu::ShaderStages::COMPUTE)
            .storage_buffer_readonly(wgpu::ShaderStages::COMPUTE);
        let entries = builder.entries();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn builder_into_entries() {
        let entries = BindGroupLayoutBuilder::new()
            .uniform_buffer(wgpu::ShaderStages::VERTEX)
            .into_entries();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn builder_default_trait() {
        let builder = BindGroupLayoutBuilder::default();
        assert!(builder.entries().is_empty());
    }

    #[test]
    fn builder_comparison_sampler() {
        let builder = BindGroupLayoutBuilder::new()
            .texture_depth_2d(wgpu::ShaderStages::FRAGMENT)
            .comparison_sampler(wgpu::ShaderStages::FRAGMENT);
        assert_eq!(builder.entries().len(), 2);
    }
}
