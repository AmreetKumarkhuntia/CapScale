//! Per-pass GPU time from timestamp queries, read back a frame or two later.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const SLOTS: usize = 7;
const QUERIES: u32 = SLOTS as u32 * 2;
const BYTES: u64 = QUERIES as u64 * 8;

struct Gpu {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    period_ns: f32,
}

pub struct Timing {
    gpu: Option<Gpu>,
    /// Milliseconds per slot from the last completed readback; 0 for passes that did not run.
    pub ms: [f32; SLOTS],
    used: Cell<u8>,
    copied: bool,
    in_flight: Option<u8>,
    ready: Arc<AtomicBool>,
}

impl Timing {
    /// `enabled` is whether the device was created with `Features::TIMESTAMP_QUERY`.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, enabled: bool) -> Self {
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: BYTES,
                usage,
                mapped_at_creation: false,
            })
        };
        let gpu = enabled.then(|| Gpu {
            set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("pass timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: QUERIES,
            }),
            resolve: buffer(
                "timestamps resolve",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            read: buffer(
                "timestamps readback",
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            ),
            period_ns: queue.get_timestamp_period(),
        });
        Self {
            gpu,
            ms: [0.0; SLOTS],
            used: Cell::new(0),
            copied: false,
            in_flight: None,
            ready: Arc::default(),
        }
    }

    pub fn available(&self) -> bool {
        self.gpu.is_some()
    }

    /// Call first each frame: picks up a finished readback.
    pub fn begin_frame(&mut self, device: &wgpu::Device) {
        self.used.set(0);
        let Some(gpu) = &self.gpu else { return };
        let _ = device.poll(wgpu::PollType::Poll);
        let Some(used) = self.in_flight else { return };
        if !self.ready.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Ok(view) = gpu.read.slice(..).get_mapped_range() {
            let ticks: Vec<u64> = view
                .as_chunks::<8>()
                .0
                .iter()
                .map(|b| u64::from_le_bytes(*b))
                .collect();
            for slot in 0..SLOTS {
                let ran = used & (1 << slot) != 0;
                let span = ticks[slot * 2 + 1].saturating_sub(ticks[slot * 2]);
                self.ms[slot] = if ran {
                    span as f32 * gpu.period_ns * 1e-6
                } else {
                    0.0
                };
            }
        }
        gpu.read.unmap();
        self.in_flight = None;
    }

    /// Timestamp writes for the render pass occupying `slot`.
    pub fn writes(&self, slot: usize) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.part(slot, true, true)
    }

    /// For a slot timed across several passes: the first records the start, the last
    /// the end, the ones in between nothing.
    pub fn part(
        &self,
        slot: usize,
        first: bool,
        last: bool,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let gpu = self.gpu.as_ref()?;
        if !(first || last) {
            return None;
        }
        self.used.set(self.used.get() | 1 << slot);
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &gpu.set,
            beginning_of_pass_write_index: first.then_some(slot as u32 * 2),
            end_of_pass_write_index: last.then_some(slot as u32 * 2 + 1),
        })
    }

    /// Call after the last pass, before the encoder is finished.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(gpu) = &self.gpu else { return };
        encoder.resolve_query_set(&gpu.set, 0..QUERIES, &gpu.resolve, 0);
        self.copied = self.in_flight.is_none();
        if self.copied {
            encoder.copy_buffer_to_buffer(&gpu.resolve, 0, &gpu.read, 0, BYTES);
        }
    }

    /// Call after `queue.submit`.
    pub fn end_frame(&mut self) {
        let Some(gpu) = &self.gpu else { return };
        if std::mem::take(&mut self.copied) {
            let ready = self.ready.clone();
            gpu.read
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    if result.is_ok() {
                        ready.store(true, Ordering::Release);
                    }
                });
            self.in_flight = Some(self.used.get());
        }
    }
}
