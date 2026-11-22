//! The tunable settings, the enums they are made of, and the uniform block the shaders read.

use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Upscaler {
    Easu,
    Lanczos3,
    Bilinear,
}

impl Upscaler {
    pub const ALL: [Upscaler; 3] = [Upscaler::Easu, Upscaler::Lanczos3, Upscaler::Bilinear];

    pub fn name(self) -> &'static str {
        match self {
            Upscaler::Easu => "easu",
            Upscaler::Lanczos3 => "lanczos3",
            Upscaler::Bilinear => "bilinear",
        }
    }

    /// What the options panel shows; `name` stays the value used in config.toml.
    pub fn label(self) -> &'static str {
        match self {
            Upscaler::Easu => "AMD FSR 1",
            Upscaler::Lanczos3 => "Lanczos-3",
            Upscaler::Bilinear => "Bilinear",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.name() == s)
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|u| *u == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Matrix {
    Bt601,
    Bt709,
}

impl Matrix {
    pub fn name(self) -> &'static str {
        match self {
            Matrix::Bt601 => "bt601",
            Matrix::Bt709 => "bt709",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "bt601" => Some(Matrix::Bt601),
            "bt709" => Some(Matrix::Bt709),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vsync {
    Mailbox,
    Fifo,
    Immediate,
}

impl Vsync {
    pub const ALL: [Vsync; 3] = [Vsync::Mailbox, Vsync::Fifo, Vsync::Immediate];

    pub fn name(self) -> &'static str {
        match self {
            Vsync::Mailbox => "mailbox",
            Vsync::Fifo => "fifo",
            Vsync::Immediate => "immediate",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == s)
    }
}

/// Which graphics API to draw with. Fixed when the window is created.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// Whatever wgpu picks (or the WGPU_BACKEND environment variable says).
    Auto,
    Dx12,
    Vulkan,
}

impl Backend {
    pub const ALL: [Backend; 3] = [Backend::Auto, Backend::Dx12, Backend::Vulkan];

    pub fn name(self) -> &'static str {
        match self {
            Backend::Auto => "auto",
            Backend::Dx12 => "dx12",
            Backend::Vulkan => "vulkan",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.name() == s)
    }

    /// DirectX 12 on Windows: graphics driver features that hook into a program's
    /// drawing, such as NVIDIA Smooth Motion, are documented for DirectX and only
    /// patchy on Vulkan. Elsewhere there is no DirectX.
    pub fn for_this_platform() -> Self {
        if cfg!(windows) {
            Backend::Dx12
        } else {
            Backend::Auto
        }
    }
}

pub const MAX_ZONES: usize = 8;

/// A part of the picture that frame generation leaves alone: a minimap, a HUD panel.
/// Inside it, generated frames show the real frame, so nothing there can smear or flicker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zone {
    /// left, top, right, bottom as fractions of the picture
    pub rect: [f32; 4],
    pub ellipse: bool,
}

pub const TOGGLES: [&str; 11] = [
    "audio",
    "zones",
    "upscale",
    "rcas",
    "grade",
    "lut",
    "split",
    "overlay",
    "panel",
    "capture",
    "fullscreen",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    pub upscaler: Upscaler,
    /// RCAS, AMD's scale: 0 is sharpest, each +1 halves it.
    pub sharpness: f32,
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub temperature: f32,
    pub tint: f32,
    pub gamma: f32,
    pub lift: [f32; 3],
    pub lgg_gamma: [f32; 3],
    pub gain: [f32; 3],
    pub lut_path: String,
    pub lut_strength: f32,
    pub split_pos: f32,
    /// None: use what each frame reports.
    pub full_range: Option<bool>,
    pub matrix: Option<Matrix>,
    pub vsync: Vsync,
    /// Takes effect the next time the window is created.
    pub backend: Backend,
    /// (0, 0): fit the window.
    pub out_size: (u32, u32),
    pub fullscreen: bool,
    pub upscale: bool,
    pub rcas: bool,
    pub grade: bool,
    pub lut: bool,
    pub split: bool,
    pub overlay: bool,
    /// The settings window is shown.
    pub panel: bool,
    /// The picture window is shown (capture is running).
    pub capture: bool,
    /// One line shown under the stats, e.g. which keys do what. Set from Python,
    /// which owns the hotkeys.
    pub hint: String,
    /// Game audio. Playback itself lives in Python (audio.py); these are here so the
    /// options panel can change them and Save can write them.
    pub audio: bool,
    pub volume: f32,
    /// Extra delay on the sound, for when the picture is the slower of the two.
    pub audio_delay_ms: f32,
    /// Frame generation. `content_fps` is the game's own rate inside the capture
    /// signal (a 30 fps game arrives as 60 with every frame repeated); 0 measures it.
    /// `target_fps` is what to show; 0 turns generation off.
    pub content_fps: f32,
    pub target_fps: f32,
    /// The game rate that capture worked out from the rhythm of repeated frames; 0
    /// until it has. Used when `content_fps` is 0.
    pub detected_fps: f32,
    pub zones: Vec<Zone>,
    /// The user's choice of method: true = the AI model (the ai package), false = the shaders here.
    pub ai_framegen: bool,
    /// AI method: where the picture is at rest, a generated frame is the real one.
    pub ai_still: bool,
    /// Whether the AI runtime is installed; the panel greys the choice out without it.
    pub ai_available: bool,
    /// Set by the Python side while its AI model is the one making the frames: they
    /// then arrive ready-made and the generation here stands down.
    pub ai_active: bool,
    pub note: String,
    /// The zones are being shown and dragged in the window (not saved).
    pub edit_zones: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            upscaler: Upscaler::Easu,
            sharpness: 0.4,
            exposure: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            vibrance: 0.0,
            temperature: 0.0,
            tint: 0.0,
            gamma: 1.0,
            lift: [0.0; 3],
            lgg_gamma: [1.0; 3],
            gain: [1.0; 3],
            lut_path: String::new(),
            lut_strength: 1.0,
            split_pos: 0.5,
            full_range: None,
            matrix: None,
            vsync: Vsync::Mailbox,
            backend: Backend::for_this_platform(),
            out_size: (0, 0),
            fullscreen: true,
            upscale: true,
            rcas: true,
            grade: true,
            lut: true,
            split: false,
            overlay: true,
            panel: true,
            capture: false,
            hint: String::new(),
            audio: true,
            volume: 1.0,
            audio_delay_ms: 0.0,
            content_fps: 0.0,
            target_fps: 0.0,
            detected_fps: 0.0,
            zones: Vec::new(),
            ai_framegen: false,
            ai_still: true,
            ai_available: false,
            ai_active: false,
            note: String::new(),
            edit_zones: false,
        }
    }
}

pub struct FrameInfo {
    pub src: (u32, u32),
    pub out: (u32, u32),
    pub dst: (u32, u32),
    /// 0 = NV12, 1 = YUY2, 2 = planar; matches yuv2rgb.wgsl
    pub layout: u32,
    pub full_range: bool,
    pub matrix: Matrix,
    /// LUT edge length, domain min, domain max
    pub lut: Option<(u32, [f32; 3], [f32; 3])>,
    /// Frame generation phase for this draw: 0 at the older frame, 1 at the newer.
    pub phase: f32,
}

/// Mirrors `struct Globals` in shaders/common.wgsl. All vec4, so no padding surprises.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    pub src_size: [f32; 4],
    pub out_size: [f32; 4],
    pub dst_size: [f32; 4],
    pub yuv: [[f32; 4]; 3],
    pub grade: [f32; 4],
    pub wb: [f32; 4],
    pub lift: [f32; 4],
    pub inv_lgg_gamma: [f32; 4],
    pub gain: [f32; 4],
    pub lut_min: [f32; 4],
    pub lut_scale: [f32; 4],
    pub misc: [f32; 4],
    pub flags: [u32; 4],
    pub zones: [[f32; 4]; MAX_ZONES],
}
