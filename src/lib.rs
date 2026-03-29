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
//! - **Core**: [`context`], [`error`], [`capabilities`], [`color`]
//! - **Compute**: [`compute`], [`buffer`]
//! - **Graphics**: [`texture`], [`render_target`]
//! - **Profiling**: [`profiler`]

pub mod buffer;
pub mod capabilities;
pub mod color;
pub mod compute;
pub mod context;
pub mod error;
pub mod profiler;
pub mod render_target;
pub mod texture;

// ── Core ────────────────────────────────────────────────────────────────────
pub use capabilities::GpuCapabilities;
pub use color::Color;
pub use context::GpuContext;
pub use error::{GpuError, Result};

// ── Compute ─────────────────────────────────────────────────────────────────
pub use buffer::{
    create_staging_buffer, create_storage_buffer, create_storage_buffer_empty,
    create_uniform_buffer, read_buffer,
};
pub use compute::ComputePipeline;

// ── Graphics ────────────────────────────────────────────────────────────────
pub use render_target::RenderTarget;
pub use texture::{Texture, TextureCache, create_default_sampler};

// ── Profiling ───────────────────────────────────────────────────────────────
pub use profiler::{FrameProfiler, GpuTimestamps, PassTiming};
