//! Frame profiling — CPU timing and GPU timestamp queries.
//!
//! [`FrameProfiler`] tracks CPU-side frame timing with exponential moving average.
//! [`GpuTimestamps`] wraps wgpu timestamp queries for per-pass GPU timing.

use std::time::Instant;

/// Frame profiler — tracks CPU-side frame timing and GPU pass durations.
#[derive(Debug, Clone)]
pub struct FrameProfiler {
    frame_start: Option<Instant>,
    /// CPU-side frame time in milliseconds.
    pub cpu_frame_ms: f64,
    /// Rolling average of frame time (exponential moving average).
    pub avg_frame_ms: f64,
    /// Frames per second (computed from avg_frame_ms).
    pub fps: f64,
    /// Total frames counted.
    pub frame_count: u64,
    /// Per-pass timing labels and durations (populated by GPU queries when available).
    pub pass_times: Vec<PassTiming>,
    alpha: f64,
}

/// Timing for a single render/compute pass.
#[derive(Debug, Clone)]
pub struct PassTiming {
    pub label: String,
    pub duration_ms: f64,
}

impl FrameProfiler {
    /// Create a new profiler with default EMA smoothing (alpha = 0.05).
    pub fn new() -> Self {
        Self::with_alpha(0.05)
    }

    /// Create a profiler with a custom EMA smoothing factor.
    ///
    /// `alpha` controls how quickly the average responds to changes:
    /// - Lower values (e.g. 0.01) = smoother, slower to react
    /// - Higher values (e.g. 0.2) = noisier, faster to react
    pub fn with_alpha(alpha: f64) -> Self {
        Self {
            frame_start: None,
            cpu_frame_ms: 0.0,
            avg_frame_ms: 16.67,
            fps: 60.0,
            frame_count: 0,
            pass_times: Vec::new(),
            alpha: alpha.clamp(0.001, 1.0),
        }
    }

    /// Call at the start of each frame.
    pub fn begin_frame(&mut self) {
        self.frame_start = Some(Instant::now());
        self.pass_times.clear();
    }

    /// Call at the end of each frame. Returns the frame time in ms.
    pub fn end_frame(&mut self) -> f64 {
        let elapsed = self
            .frame_start
            .map(|start| start.elapsed().as_secs_f64() * 1000.0)
            .unwrap_or(0.0);

        self.cpu_frame_ms = elapsed;
        self.avg_frame_ms = self.avg_frame_ms * (1.0 - self.alpha) + elapsed * self.alpha;
        self.fps = if self.avg_frame_ms > 0.0 {
            1000.0 / self.avg_frame_ms
        } else {
            0.0
        };
        self.frame_count += 1;
        self.frame_start = None;

        elapsed
    }

    /// Record a pass timing manually (for CPU-timed passes).
    pub fn record_pass(&mut self, label: impl Into<String>, duration_ms: f64) {
        self.pass_times.push(PassTiming {
            label: label.into(),
            duration_ms,
        });
    }

    /// Total time across all recorded passes.
    #[must_use]
    pub fn total_pass_time_ms(&self) -> f64 {
        self.pass_times.iter().map(|p| p.duration_ms).sum()
    }

    /// Reset all counters.
    pub fn reset(&mut self) {
        self.cpu_frame_ms = 0.0;
        self.avg_frame_ms = 16.67;
        self.fps = 60.0;
        self.frame_count = 0;
        self.pass_times.clear();
    }
}

impl Default for FrameProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// GPU timestamp query set — wraps `wgpu::QuerySet` for per-pass GPU timing.
///
/// Only functional when the device supports `Features::TIMESTAMP_QUERY`.
pub struct GpuTimestamps {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    read_buffer: wgpu::Buffer,
    max_queries: u32,
    timestamp_period: f32,
}

impl GpuTimestamps {
    /// Create GPU timestamp queries. Returns `None` if the device doesn't support timestamps.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, max_passes: u32) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }

        let max_queries = max_passes * 2; // begin + end per pass
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gpu_timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: max_queries,
        });

        let buffer_size = (max_queries as u64) * 8;
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamp_resolve"),
            size: buffer_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let read_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamp_read"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let timestamp_period = queue.get_timestamp_period();

        Some(Self {
            query_set,
            resolve_buffer,
            read_buffer,
            max_queries,
            timestamp_period,
        })
    }

    /// Get the query set for use in render/compute pass descriptors.
    #[must_use]
    pub fn query_set(&self) -> &wgpu::QuerySet {
        &self.query_set
    }

    /// Maximum number of query pairs (passes) supported.
    #[must_use]
    pub fn max_passes(&self) -> u32 {
        self.max_queries / 2
    }

    /// Resolve queries and copy to read buffer. Call after all passes are submitted.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder, query_count: u32) {
        let count = query_count.min(self.max_queries);
        encoder.resolve_query_set(&self.query_set, 0..count, &self.resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(
            &self.resolve_buffer,
            0,
            &self.read_buffer,
            0,
            (count as u64) * 8,
        );
    }

    /// Read back timestamp results. Blocking — call after `queue.submit` + `device.poll`.
    ///
    /// Returns durations in milliseconds for each pass (begin→end pair).
    pub fn read_results(&self, device: &wgpu::Device, query_count: u32) -> Vec<f64> {
        let count = query_count.min(self.max_queries) as usize;
        let slice = self.read_buffer.slice(..((count * 8) as u64));

        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(wgpu::PollType::Wait {
            timeout: None,
            submission_index: None,
        });

        if rx.recv().ok().and_then(|r| r.ok()).is_none() {
            return Vec::new();
        }

        let data = slice.get_mapped_range();
        let timestamps: &[u64] = bytemuck::cast_slice(&data);

        let mut durations = Vec::with_capacity(count / 2);
        for pair in timestamps.chunks(2) {
            if pair.len() == 2 && pair[1] >= pair[0] {
                let ns = (pair[1] - pair[0]) as f64 * self.timestamp_period as f64;
                durations.push(ns / 1_000_000.0);
            }
        }

        drop(data);
        self.read_buffer.unmap();

        durations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiler_default() {
        let p = FrameProfiler::new();
        assert_eq!(p.frame_count, 0);
        assert!((p.fps - 60.0).abs() < 0.1);
    }

    #[test]
    fn profiler_begin_end() {
        let mut p = FrameProfiler::new();
        p.begin_frame();
        let ms = p.end_frame();
        assert!(ms >= 0.0);
        assert_eq!(p.frame_count, 1);
    }

    #[test]
    fn profiler_fps_updates() {
        let mut p = FrameProfiler::new();
        for _ in 0..100 {
            p.begin_frame();
            p.end_frame();
        }
        assert!(p.fps > 0.0);
        assert_eq!(p.frame_count, 100);
    }

    #[test]
    fn profiler_reset() {
        let mut p = FrameProfiler::new();
        p.begin_frame();
        p.end_frame();
        p.reset();
        assert_eq!(p.frame_count, 0);
    }

    #[test]
    fn profiler_end_without_begin() {
        let mut p = FrameProfiler::new();
        let ms = p.end_frame();
        assert_eq!(ms, 0.0);
    }

    #[test]
    fn profiler_record_pass() {
        let mut p = FrameProfiler::new();
        p.begin_frame();
        p.record_pass("shadow", 0.5);
        p.record_pass("pbr", 2.0);
        p.record_pass("post", 0.3);
        assert_eq!(p.pass_times.len(), 3);
        assert!((p.total_pass_time_ms() - 2.8).abs() < 0.001);
    }

    #[test]
    fn profiler_begin_clears_passes() {
        let mut p = FrameProfiler::new();
        p.record_pass("test", 1.0);
        p.begin_frame();
        assert!(p.pass_times.is_empty());
    }

    #[test]
    fn pass_timing_fields() {
        let t = PassTiming {
            label: "shadow".into(),
            duration_ms: 1.5,
        };
        assert_eq!(t.label, "shadow");
        assert_eq!(t.duration_ms, 1.5);
    }
}
