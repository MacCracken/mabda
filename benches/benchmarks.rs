use criterion::{Criterion, criterion_group, criterion_main};
use mabda::{Color, FrameProfiler, GpuCapabilities};

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

criterion_group!(
    benches,
    bench_color_lerp,
    bench_color_from_hex,
    bench_color_luminance,
    bench_workgroups,
    bench_profiler_frame,
    bench_capabilities_report,
);
criterion_main!(benches);
