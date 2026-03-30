use criterion::{Criterion, criterion_group, criterion_main};
use mabda::{Color, FrameProfiler, GpuCapabilities};

// ── CPU-only benchmarks ─────────────────────────────────────────────────────

fn bench_color_lerp(c: &mut Criterion) {
    let a = Color::BLACK;
    let b = Color::WHITE;
    c.bench_function("color_lerp", |bench| {
        bench.iter(|| {
            std::hint::black_box(a.lerp(b, 0.5));
        });
    });
}

fn bench_color_from_hex(c: &mut Criterion) {
    c.bench_function("color_from_hex", |bench| {
        bench.iter(|| {
            std::hint::black_box(Color::from_hex(0xFF8040FF));
        });
    });
}

fn bench_color_luminance(c: &mut Criterion) {
    let color = Color::new(0.5, 0.3, 0.8, 1.0);
    c.bench_function("color_luminance", |bench| {
        bench.iter(|| {
            std::hint::black_box(color.luminance());
        });
    });
}

fn bench_workgroups(c: &mut Criterion) {
    #[cfg(feature = "compute")]
    {
        use mabda::compute::{workgroups_1d, workgroups_2d};
        c.bench_function("workgroups_1d", |bench| {
            bench.iter(|| {
                std::hint::black_box(workgroups_1d(1_000_000, 256));
            });
        });
        c.bench_function("workgroups_2d", |bench| {
            bench.iter(|| {
                std::hint::black_box(workgroups_2d(1920, 1080, 16, 16));
            });
        });
    }
}

fn bench_profiler_frame(c: &mut Criterion) {
    c.bench_function("profiler_frame_cycle", |bench| {
        let mut profiler = FrameProfiler::new();
        bench.iter(|| {
            profiler.begin_frame();
            profiler.record_pass("test", 1.0);
            std::hint::black_box(profiler.end_frame());
        });
    });
}

fn bench_capabilities_report(c: &mut Criterion) {
    let caps = GpuCapabilities {
        adapter_name: "NVIDIA GeForce RTX 4090".into(),
        backend: "Vulkan".into(),
        timestamp_query: true,
        compute_shaders: true,
        max_texture_dimension_2d: 16384,
        max_uniform_buffer_size: 65536,
        max_storage_buffer_size: 134_217_728,
        max_buffer_size: 268_435_456,
        max_bind_groups: 4,
        max_vertex_buffers: 8,
        max_compute_workgroup_size: [256, 256, 64],
        max_compute_workgroups_per_dimension: 65535,
        multi_draw_indirect: true,
    };
    c.bench_function("capabilities_report", |bench| {
        bench.iter(|| {
            std::hint::black_box(caps.report());
        });
    });
}

// ── GPU benchmarks ──────────────────────────────────────────────────────────

fn try_gpu() -> Option<mabda::GpuContext> {
    pollster::block_on(mabda::GpuContext::new()).ok()
}

fn bench_buffer_creation(c: &mut Criterion) {
    let Some(ctx) = try_gpu() else { return };
    let data = vec![0u8; 4096];

    c.bench_function("create_storage_buffer_4k", |bench| {
        bench.iter(|| {
            std::hint::black_box(mabda::create_storage_buffer(
                &ctx.device,
                &data,
                "bench",
                false,
            ));
        });
    });

    c.bench_function("create_uniform_buffer_64", |bench| {
        let small = [0u8; 64];
        bench.iter(|| {
            std::hint::black_box(mabda::create_uniform_buffer(&ctx.device, &small, "bench"));
        });
    });
}

fn bench_typed_buffer(c: &mut Criterion) {
    let Some(ctx) = try_gpu() else { return };

    #[repr(C)]
    #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
    struct Uniforms {
        data: [f32; 16], // 64 bytes
    }

    let uniforms = Uniforms { data: [1.0; 16] };
    let buf = mabda::UniformBuffer::new(&ctx.device, &uniforms, "bench").unwrap();

    c.bench_function("uniform_buffer_write", |bench| {
        let updated = Uniforms { data: [2.0; 16] };
        bench.iter(|| {
            buf.write(&ctx.queue, &updated);
        });
    });
}

fn bench_shader_cache(c: &mut Criterion) {
    let Some(ctx) = try_gpu() else { return };
    let source = "@compute @workgroup_size(64) fn main() {}";

    c.bench_function("shader_cache_hit", |bench| {
        let mut cache = mabda::ShaderCache::new();
        // Prime the cache
        let _ = cache.get_or_compile(&ctx.device, source, "bench");
        bench.iter(|| {
            std::hint::black_box(cache.get_or_compile(&ctx.device, source, "bench"));
        });
    });

    c.bench_function("shader_cache_miss", |bench| {
        bench.iter_with_setup(mabda::ShaderCache::new, |mut cache| {
            std::hint::black_box(cache.get_or_compile(&ctx.device, source, "bench"));
        });
    });
}

#[cfg(feature = "graphics")]
fn bench_texture_creation(c: &mut Criterion) {
    let Some(ctx) = try_gpu() else { return };

    c.bench_function("texture_1x1_solid", |bench| {
        bench.iter(|| {
            std::hint::black_box(
                mabda::texture::Texture::from_color(&ctx.device, &ctx.queue, Color::RED).unwrap(),
            );
        });
    });

    c.bench_function("texture_256x256_rgba", |bench| {
        let data = vec![128u8; 256 * 256 * 4];
        bench.iter(|| {
            std::hint::black_box(
                mabda::texture::Texture::from_rgba(
                    &ctx.device,
                    &ctx.queue,
                    &data,
                    256,
                    256,
                    "bench",
                )
                .unwrap(),
            );
        });
    });
}

#[cfg(feature = "graphics")]
fn bench_depth_texture(c: &mut Criterion) {
    let Some(ctx) = try_gpu() else { return };

    c.bench_function("depth_texture_1080p", |bench| {
        bench.iter(|| {
            std::hint::black_box(mabda::DepthTexture::new_default(&ctx.device, 1920, 1080));
        });
    });
}

#[cfg(feature = "graphics")]
fn bench_render_target(c: &mut Criterion) {
    let Some(ctx) = try_gpu() else { return };

    c.bench_function("render_target_1080p", |bench| {
        bench.iter(|| {
            std::hint::black_box(mabda::render_target::RenderTarget::new(
                &ctx.device,
                1920,
                1080,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ));
        });
    });

    c.bench_function("render_target_msaa4_1080p", |bench| {
        bench.iter(|| {
            std::hint::black_box(
                mabda::render_target::RenderTargetBuilder::new(&ctx.device, 1920, 1080)
                    .msaa(4)
                    .depth(mabda::DepthTexture::DEFAULT_FORMAT)
                    .build(),
            );
        });
    });
}

#[cfg(feature = "compute")]
fn bench_compute_dispatch(c: &mut Criterion) {
    let Some(ctx) = try_gpu() else { return };

    let shader = r#"
        @group(0) @binding(0) var<storage, read_write> out: array<f32>;
        @compute @workgroup_size(64)
        fn main(@builtin(global_invocation_id) id: vec3u) {
            out[id.x] = f32(id.x);
        }
    "#;

    let pipeline = mabda::ComputePipeline::new(&ctx.device, shader, "main", 1);
    let buf = mabda::create_storage_buffer_empty(&ctx.device, 4096, "bench", false);
    let bg = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bench"),
        layout: pipeline.bind_group_layout(0).unwrap(),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });

    c.bench_function("compute_dispatch_1024", |bench| {
        bench.iter(|| {
            pipeline.dispatch(&ctx.device, &ctx.queue, &bg, 16, 1, 1);
        });
    });
}

#[cfg(feature = "graphics")]
fn bench_render_pipeline_build(c: &mut Criterion) {
    let Some(ctx) = try_gpu() else { return };

    let shader = r#"
        @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
            return vec4f(0.0, 0.0, 0.0, 1.0);
        }
        @fragment fn fs() -> @location(0) vec4f {
            return vec4f(1.0, 0.0, 0.0, 1.0);
        }
    "#;

    c.bench_function("render_pipeline_build", |bench| {
        bench.iter(|| {
            std::hint::black_box(
                mabda::RenderPipelineBuilder::new(&ctx.device, shader, "vs", "fs")
                    .color_target(wgpu::TextureFormat::Rgba8UnormSrgb, None)
                    .build()
                    .unwrap(),
            );
        });
    });
}

fn bench_bind_group_cache(c: &mut Criterion) {
    let Some(ctx) = try_gpu() else { return };

    let layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bench_layout"),
            entries: &[],
        });

    c.bench_function("bind_group_cache_hit", |bench| {
        let mut cache = mabda::BindGroupCache::new();
        let _ = cache.get_or_insert(0, || {
            ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bench"),
                layout: &layout,
                entries: &[],
            })
        });
        bench.iter(|| {
            std::hint::black_box(cache.get_or_insert(0, || {
                panic!("should not be called");
            }));
        });
    });
}

// ── Group registration ──────────────────────────────────────────────────────

criterion_group!(
    cpu_benches,
    bench_color_lerp,
    bench_color_from_hex,
    bench_color_luminance,
    bench_workgroups,
    bench_profiler_frame,
    bench_capabilities_report,
);

criterion_group!(
    gpu_benches,
    bench_buffer_creation,
    bench_typed_buffer,
    bench_shader_cache,
    bench_bind_group_cache,
);

#[cfg(feature = "graphics")]
criterion_group!(
    graphics_benches,
    bench_texture_creation,
    bench_depth_texture,
    bench_render_target,
    bench_render_pipeline_build,
);

#[cfg(feature = "compute")]
criterion_group!(compute_benches, bench_compute_dispatch,);

#[cfg(all(feature = "graphics", feature = "compute"))]
criterion_main!(cpu_benches, gpu_benches, graphics_benches, compute_benches);

#[cfg(all(feature = "graphics", not(feature = "compute")))]
criterion_main!(cpu_benches, gpu_benches, graphics_benches);

#[cfg(all(not(feature = "graphics"), feature = "compute"))]
criterion_main!(cpu_benches, gpu_benches, compute_benches);

#[cfg(not(any(feature = "graphics", feature = "compute")))]
criterion_main!(cpu_benches, gpu_benches);
