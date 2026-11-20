//! State shared between the Python-facing API and the render thread.

use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard};

use winit::event_loop::EventLoopProxy;

use crate::lut::Cube;
use crate::params::Params;
use crate::source::Source;
use crate::timing::SLOTS;

pub const PASS_NAMES: [&str; SLOTS] = [
    "yuv",
    "framegen",
    "upscale",
    "rcas",
    "grade",
    "lut",
    "composite",
];

/// Sent to the render thread to wake its event loop.
#[derive(Debug, Clone, Copy)]
pub enum Wake {
    Redraw,
    Close,
}

/// Things the window reports back to Python through `Renderer.poll_events()`.
pub enum Event {
    Key(String),
    /// "Start capture" in the settings window: device name, width, height, fps
    Start(String, u32, u32, u32),
    /// The picture window was closed, or "Stop" pressed
    Stop,
    /// "Refresh" on the Source tab
    RefreshSources,
    /// "Load .cube" in the settings window
    PickLut,
    /// "Save" in the settings window; carries the config.toml sections to write
    Save(String),
    /// The settings window was closed: quit
    Closed,
}

/// A capture device: (name to open it by, label to show, modes as (width, height, max fps)).
pub type Device = (String, String, Vec<(u32, u32, f32)>);

/// The capture devices Python found, and the one picked on the Source tab.
#[derive(Default)]
pub struct Sources {
    /// false until Python has listed the devices once
    pub listed: bool,
    /// a device was saved from an earlier run, so the user has been through setup before
    pub remembered: bool,
    pub devices: Vec<Device>,
    pub device: usize,
    pub size: (u32, u32),
    pub fps: u32,
}

#[derive(Clone, Default)]
pub struct Stats {
    pub capture_fps: f32,
    pub render_fps: f32,
    pub decode_ms: f32,
    /// push_frame call to present call
    pub present_ms: f32,
    pub pass_ms: [f32; SLOTS],
    pub gpu_timing: bool,
    /// frames pushed in, and frames drawn
    pub frames: u64,
    pub rendered: u64,
    /// Pictures shown per content frame; 1 when frame generation is off.
    pub multiple: u32,
    /// The game rate the frame generation schedule is built on.
    pub content_fps: f32,
    /// Rate of the slowest 1% of pictures over the last few seconds; 0 until measured.
    pub low_fps: f32,
    pub source: String,
    pub output: (u32, u32),
    pub upscaler: &'static str,
    pub present_mode: String,
    pub adapter: String,
    pub error: String,
}

pub struct Shared {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub source: Mutex<Source>,
    pub params: Mutex<Params>,
    /// (generation, parsed LUT); the render thread uploads it when the generation moves.
    pub lut: Mutex<(u64, Option<Cube>)>,
    pub stats: Mutex<Stats>,
    pub events: Mutex<Vec<Event>>,
    pub sources: Mutex<Sources>,
    pub proxy: Mutex<EventLoopProxy<Wake>>,
    pub closed: AtomicBool,
}

impl Shared {
    pub fn wake(&self, wake: Wake) -> bool {
        lock(&self.proxy).send_event(wake).is_ok()
    }
}

/// Lock that survives a panic on the other thread; the data is plain values either way.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
