use criterion::{Criterion, criterion_group, criterion_main};
use mabda::compute::{workgroups_1d, workgroups_2d};
use mabda::{Color, FrameProfiler};

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

fn bench_workgroups_1d(c: &mut Criterion) {
    c.bench_function("workgroups_1d", |bench| {
        bench.iter(|| {
            std::hint::black_box(workgroups_1d(1_000_000, 256));
        });
    });
}

fn bench_workgroups_2d(c: &mut Criterion) {
    c.bench_function("workgroups_2d", |bench| {
        bench.iter(|| {
            std::hint::black_box(workgroups_2d(1920, 1080, 16, 16));
        });
    });
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

criterion_group!(
    benches,
    bench_color_lerp,
    bench_color_from_hex,
    bench_workgroups_1d,
    bench_workgroups_2d,
    bench_profiler_frame,
);
criterion_main!(benches);
