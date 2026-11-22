//! State shared between the Python-facing API and the render thread, and what one draw does.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use pyo3::prelude::*;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

use crate::ui::Overlay;

use crate::lut::Cube;
use crate::params::Params;
use crate::source::Source;

pub const PASS_NAMES: [&str; SLOTS] = [
    "yuv",
    "framegen",
    "denoise",
    "upscale",
    "rcas",
    "grade",
    "lut",
    "composite",
];

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
    RefreshSources,
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

#[derive(Clone, Copy, Debug, Default)]
pub struct Step {
    /// A frame arrived since the last draw: convert it and keep the one before.
    pub new_frame: bool,
    /// Frame generation is switched on (whether or not this draw shows a generated frame).
    pub generating: bool,
    /// Show a generated picture at this point between the two newest frames
    /// (0 = older, 1 = newer). None shows the newest frame as it is.
    pub phase: Option<f32>,
}

/// Which bind group the composite pass reads; handed back by `encode`.
#[derive(Clone, Copy, Debug)]
pub struct Composite {
    pub(super) frame: usize,
    pub(super) in_a: bool,
}

pub(super) struct Gen {
    /// Brightness pyramids of the two newest frames: `[frame][level]`, level 0 = half size.
    pub(super) luma: [Vec<wgpu::TextureView>; 2],
    pub(super) flow: Vec<wgpu::TextureView>,
    pub(super) mid: wgpu::TextureView,
    pub(super) bg_luma: [Vec<wgpu::BindGroup>; 2],
    /// `[newest frame][level]`: older luma, newer luma, motion from the level above.
    pub(super) bg_flow: [Vec<wgpu::BindGroup>; 2],
    /// `[newest frame]`: older frame, newer frame, finest motion.
    pub(super) bg_interp: [wgpu::BindGroup; 2],
    pub(super) bg_mid: wgpu::BindGroup,
}

pub(super) struct Chain {
    /// The two newest converted frames; `Pipeline::newest` says which is which.
    pub(super) rgb: [wgpu::TextureView; 2],
    /// The picture after noise reduction, at capture size.
    pub(super) den: wgpu::TextureView,
    pub(super) bg_den: wgpu::BindGroup,
    pub(super) a: wgpu::TextureView,
    pub(super) b: wgpu::TextureView,
    pub(super) bg_src: wgpu::BindGroup,
    pub(super) bg_rgb: [wgpu::BindGroup; 2],
    pub(super) bg_a: wgpu::BindGroup,
    pub(super) bg_b: wgpu::BindGroup,
    /// `[frame shown as "before"][processed image is in a / b]`
    pub(super) bg_comp: [[wgpu::BindGroup; 2]; 2],
    pub(super) gen: Gen,
}

pub struct Pipeline {
    pub(super) surface_format: wgpu::TextureFormat,
    pub(super) globals_buf: wgpu::Buffer,
    pub(super) globals_bg: wgpu::BindGroup,
    pub(super) tex_layout: wgpu::BindGroupLayout,
    pub(super) layout: wgpu::PipelineLayout,
    pub(super) sampler: wgpu::Sampler,
    pub(super) dummy_2d: wgpu::TextureView,
    pub(super) dummy_3d: wgpu::TextureView,
    pub(super) pipelines: Vec<wgpu::RenderPipeline>,
    pub(super) src_views: Vec<wgpu::TextureView>,
    pub src_size: (u32, u32),
    pub out_size: (u32, u32),
    pub(super) lut: Option<wgpu::TextureView>,
    /// Edge length and input domain of the loaded LUT.
    pub lut_info: Option<(u32, [f32; 3], [f32; 3])>,
    pub(super) chain: Option<Chain>,
    /// Index into `Chain::rgb` of the newest frame.
    pub(super) newest: usize,
    /// Frames converted since the targets were last rebuilt; generation needs two.
    pub(super) converted: u64,
    /// Whether each frame's brightness pyramid matches what is in `rgb`.
    pub(super) luma_ready: [bool; 2],
}

pub struct ShaderDir {
    pub(super) dir: PathBuf,
    pub(super) seen: HashMap<String, Option<SystemTime>>,
}

pub(super) struct Gpu {
    pub(super) set: wgpu::QuerySet,
    pub(super) resolve: wgpu::Buffer,
    pub(super) read: wgpu::Buffer,
    pub(super) period_ns: f32,
}

pub struct Timing {
    pub(super) gpu: Option<Gpu>,
    /// Milliseconds per slot from the last completed readback; 0 for passes that did not run.
    pub ms: [f32; SLOTS],
    pub(super) used: Cell<u8>,
    pub(super) copied: bool,
    pub(super) in_flight: Option<u8>,
    pub(super) ready: Arc<AtomicBool>,
}

pub(super) type Ready = Sender<Result<Arc<Shared>, String>>;

pub(super) struct App {
    pub(super) init: Option<(PathBuf, Params, Ready)>,
    pub(super) proxy: EventLoopProxy<Wake>,
    pub(super) gfx: Option<Gfx>,
}

/// The settings window: egui only, drawn when its UI changes and a few times a second
/// for the live numbers in it.
pub(super) struct Settings {
    pub(super) window: Arc<Window>,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) overlay: Overlay,
    /// The UI changed and is waiting for a redraw (see UI_REDRAW_GAP).
    pub(super) dirty: bool,
    pub(super) last_render: Instant,
    pub(super) shown: bool,
}

pub(super) struct Gfx {
    /// The picture window; hidden while not capturing.
    pub(super) window: Arc<Window>,
    pub(super) shown: bool,
    pub(super) settings: Settings,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) present_modes: Vec<wgpu::PresentMode>,
    pub(super) shared: Arc<Shared>,
    pub(super) pipeline: Pipeline,
    pub(super) shaders: ShaderDir,
    pub(super) shader_errors: Vec<String>,
    pub(super) timing: Timing,
    pub(super) overlay: Overlay,
    pub(super) source_generation: u64,
    pub(super) lut_generation: u64,
    pub(super) shown_frame: u64,
    pub(super) last_render: Instant,
    pub(super) render_fps: f32,
    pub(super) last_hot_reload: Instant,
    pub(super) fullscreen: bool,
    pub(super) cursor_visible: bool,
    pub(super) cursor_x: f64,
    pub(super) dragging: bool,
    /// The UI changed and is waiting for a redraw (see UI_REDRAW_GAP).
    pub(super) ui_dirty: bool,
    /// The split divider or the HUD zones are showing, so pointer events matter.
    pub(super) interactive: bool,
    pub(super) rendered: u64,
    /// Seconds between the most recent pictures, newest last.
    pub(super) intervals: VecDeque<f32>,
    /// Frame count at the last draw, to tell when a new frame has arrived.
    pub(super) converted_frame: u64,
    /// When the next generated picture is due.
    pub(super) next_tick: Option<Instant>,
}

/// The window and GPU pipeline. Frames go in with `push_frame`; the newest one is drawn.
#[pyclass(module = "capscale")]
pub(crate) struct Renderer {
    pub(super) shared: Arc<Shared>,
}

/// Shader file names, without `.wgsl`. Indices are used throughout this module.
pub const FILES: [&str; 12] = [
    "yuv2rgb",
    "bilinear",
    "lanczos3",
    "easu",
    "rcas",
    "grade",
    "lut3d",
    "composite",
    "luma",
    "flow",
    "interp",
    "denoise",
];

pub(super) const YUV: usize = 0;

pub(super) const BILINEAR: usize = 1;

pub(super) const LANCZOS3: usize = 2;

pub(super) const EASU: usize = 3;

pub(super) const RCAS: usize = 4;

pub(super) const GRADE: usize = 5;

pub(super) const LUT: usize = 6;

pub(super) const COMPOSITE: usize = 7;

pub(super) const LUMA: usize = 8;

pub(super) const FLOW: usize = 9;

pub(super) const INTERP: usize = 10;

pub(super) const DENOISE: usize = 11;

/// Timing slots, in the order of `PASS_NAMES`.
pub(super) const SLOT_YUV: usize = 0;

pub(super) const SLOT_FRAMEGEN: usize = 1;

pub(super) const SLOT_DENOISE: usize = 2;

pub(super) const SLOT_UPSCALE: usize = 3;

pub(super) const SLOT_RCAS: usize = 4;

pub(super) const SLOT_GRADE: usize = 5;

pub(super) const SLOT_LUT: usize = 6;

pub(super) const INTERMEDIATE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub(super) const COMMON: &str = "common";

// shortcut: Timing keeps "which slots ran" in a u8, which 8 slots fill; widen it for a ninth
pub const SLOTS: usize = 8;

pub(super) const QUERIES: u32 = SLOTS as u32 * 2;

pub(super) const BYTES: u64 = QUERIES as u64 * 8;

pub(super) const HOT_RELOAD_EVERY: Duration = Duration::from_millis(250);

/// Redraws that only the UI asked for wait this long after the previous frame. A mouse
/// reports movement up to 1000 times a second; without the gap each report would
/// run the whole pipeline.
pub(super) const UI_REDRAW_GAP: Duration = Duration::from_millis(16);

/// How many picture-to-picture intervals the steadiness figure looks back over.
pub(super) const INTERVALS: usize = 600;
