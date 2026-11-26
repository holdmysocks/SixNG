//! GPU time of the 3D view's frame by part (sky tables, shadow cascades,
//! the main pass, post), from timestamp queries.
//!
//! Only on adapters with timestamps inside command encoders. Each frame
//! writes [`MARKS`] timestamps into the encoder; they are resolved and
//! copied into one of a few read-back buffers that is mapped once the GPU
//! is done with it, so the CPU never waits. The spans are summed into
//! [`GTIME`]; automation runs print and reset them every tick.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use eframe::egui_wgpu::wgpu;

/// Timestamps per frame: start, after the sky tables, after the shadow
/// cascades, after the main pass, after post.
pub const MARKS: u32 = 5;
/// Spans between the marks.
pub const SPANS: usize = MARKS as usize - 1;
/// Read-back buffers in flight.
const RING: usize = 4;
/// Summed span times (ns) and, last, the frames summed.
pub static GTIME: [AtomicU64; SPANS + 1] = [const { AtomicU64::new(0) }; SPANS + 1];

const FREE: u8 = 0;
const COPIED: u8 = 1;
const MAPPING: u8 = 2;
const MAPPED: u8 = 3;

pub struct GpuTimer {
    qs: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: Vec<(wgpu::Buffer, Arc<AtomicU8>)>,
    /// ns per tick
    period: f64,
}

/// The features the timer needs (requested when the adapter has them).
pub fn features() -> wgpu::Features {
    wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
}

impl GpuTimer {
    /// `None` when the device has no timestamps in encoders.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(features()) {
            return None;
        }
        let bytes = MARKS as u64 * wgpu::QUERY_SIZE as u64;
        let qs = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: MARKS,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame timestamps resolve"),
            size: bytes,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = (0..RING)
            .map(|_| {
                let b = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("frame timestamps read-back"),
                    size: bytes,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                (b, Arc::new(AtomicU8::new(FREE)))
            })
            .collect();
        Some(Self { qs, resolve, read, period: queue.get_timestamp_period() as f64 })
    }

    /// Sums the frames the GPU has finished into [`GTIME`], then picks a
    /// free read-back buffer for this frame (`None`: all in flight, this
    /// frame is not timed).
    pub fn begin(&mut self, device: &wgpu::Device) -> Option<usize> {
        let _ = device.poll(wgpu::PollType::Poll);
        for (b, st) in &self.read {
            if st.load(Ordering::Acquire) != MAPPED {
                continue;
            }
            if let Ok(v) = b.slice(..).get_mapped_range() {
                let ts: &[u64] = bytemuck::cast_slice(&v);
                // a span the GPU reports backwards (a counter reset) drops the frame
                if ts.windows(2).all(|w| w[1] >= w[0]) && ts[0] > 0 {
                    for i in 0..SPANS {
                        GTIME[i].fetch_add(((ts[i + 1] - ts[i]) as f64 * self.period) as u64, Ordering::Relaxed);
                    }
                    GTIME[SPANS].fetch_add(1, Ordering::Relaxed);
                }
            }
            b.unmap();
            st.store(FREE, Ordering::Release);
        }
        self.read.iter().position(|(_, st)| st.load(Ordering::Acquire) == FREE)
    }

    /// Writes timestamp `i` (0..MARKS) at this point of the encoder.
    pub fn mark(&self, enc: &mut wgpu::CommandEncoder, i: u32) {
        enc.write_timestamp(&self.qs, i);
    }

    /// Resolves the frame's timestamps into read-back buffer `slot`.
    pub fn end(&self, enc: &mut wgpu::CommandEncoder, slot: usize) {
        enc.resolve_query_set(&self.qs, 0..MARKS, &self.resolve, 0);
        enc.copy_buffer_to_buffer(&self.resolve, 0, &self.read[slot].0, 0, None);
        self.read[slot].1.store(COPIED, Ordering::Release);
    }

    /// After the frame's submit: map `slot` for reading when the GPU is done.
    pub fn after_submit(&self, slot: usize) {
        let (b, st) = &self.read[slot];
        st.store(MAPPING, Ordering::Release);
        let st = st.clone();
        b.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            st.store(if r.is_ok() { MAPPED } else { FREE }, Ordering::Release);
        });
    }
}

/// The summed spans as mean ms per frame `sky|shadows|main|post`, and
/// starts the sums over ("-" without timestamps).
pub fn take_line() -> String {
    let n = GTIME[SPANS].swap(0, Ordering::Relaxed);
    let v: Vec<u64> = (0..SPANS).map(|i| GTIME[i].swap(0, Ordering::Relaxed)).collect();
    if n == 0 {
        return "-".into();
    }
    v.iter().map(|ns| format!("{:.2}", *ns as f64 / n as f64 / 1e6)).collect::<Vec<_>>().join("|")
}
