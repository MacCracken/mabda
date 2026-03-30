//! Mabda — GPU foundation layer for AGNOS
//!
//! **Mabda** (Arabic: مبدأ — origin, principle, starting point) provides the shared
//! GPU foundation that all AGNOS GPU consumers build upon. It owns the wgpu
//! dependency and exposes device lifecycle, buffer management, compute dispatch,
//! texture handling, profiling, and capability detection.
//!
//! # Features
//!
//! - `graphics` — render targets, texture loading, surface helpers
//! - `compute` — compute pipeline, storage buffers, dispatch utilities
//! - `full` — enables both `graphics` and `compute`
//!
//! # Consumers
//!
//! - **soorat** — rendering engine (sprites, PBR, shadows, post-fx)
//! - **rasa** — image editor (GPU compute filters)
//! - **ranga** — image processing library (GPU pixel ops)
//! - **bijli** — electromagnetic simulation (FDTD compute)
//! - **aethersafta** — desktop compositor (GPU compositing)
//! - **kiran** — game engine (via soorat)
//!
//! # Modules
//!
//! - **Core**: [`context`], [`error`], [`capabilities`], [`color`], [`buffer`], [`typed_buffer`], [`debug`], [`resource`]
//! - **Compute**: [`compute`] (feature-gated)
//! - **Graphics**: [`texture`], [`render_target`], [`render_pipeline`], [`render_pass`], [`vertex`], [`sampler`], [`surface`], [`depth`], [`blend`], [`bind_group`], [`instancing`] (feature-gated)
//! - **Caching**: [`shader`], [`pipeline_cache`]
//! - **Profiling**: [`profiler`]

// ── Core (always available) ─────────────────────────────────────────────────
pub mod buffer;
pub mod capabilities;
pub mod color;
pub mod context;
pub mod debug;
pub mod error;
pub mod pipeline_cache;
pub mod profiler;
pub mod resource;
pub mod typed_buffer;

// ── Compute ─────────────────────────────────────────────────────────────────
#[cfg(feature = "compute")]
pub mod compute;

// ── Graphics ────────────────────────────────────────────────────────────────
#[cfg(feature = "graphics")]
pub mod bind_group;
#[cfg(feature = "graphics")]
pub mod blend;
#[cfg(feature = "graphics")]
pub mod depth;
#[cfg(feature = "graphics")]
pub mod instancing;
#[cfg(feature = "graphics")]
pub mod render_pass;

// ── Shared (no feature gate) ────────────────────────────────────────────────
#[cfg(feature = "graphics")]
pub mod render_pipeline;
#[cfg(feature = "graphics")]
pub mod render_target;
#[cfg(feature = "graphics")]
pub mod sampler;
pub mod shader;
#[cfg(feature = "graphics")]
pub mod surface;
#[cfg(feature = "graphics")]
pub mod texture;
#[cfg(feature = "graphics")]
pub mod vertex;

// ── Core re-exports ─────────────────────────────────────────────────────────
pub use capabilities::GpuCapabilities;
pub use color::Color;
pub use context::{GpuContext, GpuContextBuilder};
pub use debug::DebugScope;
pub use error::{GpuError, Result};
pub use resource::FrameResources;

pub use buffer::{
    GrowableBuffer, PendingReadback, create_dispatch_indirect_buffer, create_staging_buffer,
    create_storage_buffer, create_storage_buffer_empty, create_uniform_buffer, read_buffer,
    read_buffer_async, read_buffer_typed,
};
pub use pipeline_cache::PipelineCache;
pub use shader::ShaderCache;

// ── Compute re-exports ──────────────────────────────────────────────────────
#[cfg(feature = "compute")]
pub use compute::{
    ComputePipeline, PingPongBuffer, validate_dispatch, workgroups_1d, workgroups_2d,
};

// ── Graphics re-exports ─────────────────────────────────────────────────────
#[cfg(feature = "graphics")]
pub use bind_group::BindGroupLayoutBuilder;
#[cfg(feature = "graphics")]
pub use blend::{BlendPreset, blend_state};
#[cfg(feature = "graphics")]
pub use buffer::{
    create_draw_indexed_indirect_buffer, create_draw_indirect_buffer, create_index_buffer,
    create_vertex_buffer,
};
#[cfg(feature = "graphics")]
pub use depth::DepthTexture;
#[cfg(feature = "graphics")]
pub use instancing::{InstanceBuffer, InstanceData};
#[cfg(feature = "graphics")]
pub use render_pass::RenderPassBuilder;
#[cfg(feature = "graphics")]
pub use render_pipeline::{DrawCommand, RenderPipeline, RenderPipelineBuilder};
#[cfg(feature = "graphics")]
pub use render_target::{RenderTarget, RenderTargetBuilder};
#[cfg(feature = "graphics")]
pub use sampler::{SamplerPreset, create_sampler, create_sampler_custom};
#[cfg(feature = "graphics")]
pub use surface::{PresentModePreference, SurfaceState};
#[cfg(feature = "graphics")]
pub use texture::{
    CubemapTexture, Texture, TextureCache, copy_texture_to_texture, create_default_sampler,
    mip_level_count, validate_dimensions,
};
#[cfg(feature = "graphics")]
pub use vertex::{SkinnedVertex3D, Vertex2D, Vertex3D, VertexLayout};

// ── Typed buffer re-exports ──────────────────────────────────────────────────
pub use typed_buffer::{StorageBuffer, UniformBuffer};

// ── Profiling re-exports ────────────────────────────────────────────────────
pub use profiler::{FrameProfiler, GpuTimestamps, PassTiming, ProfileScope};
