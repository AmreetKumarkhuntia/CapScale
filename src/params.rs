//! Tunable parameters, the uniform block every shader sees, and the colour maths behind it.

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

/// How many HUD zones the shaders take.
pub const MAX_ZONES: usize = 8;

/// A part of the picture that frame generation leaves alone: a minimap, a HUD panel.
/// Inside it, generated frames show the real frame, so nothing there can smear or flicker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zone {
    /// left, top, right, bottom as fractions of the picture
    pub rect: [f32; 4],
    /// an ellipse filling the rectangle, for round minimaps
    pub ellipse: bool,
}

pub const TOGGLES: [&str; 10] = [
    "audio",
    "zones",
    "upscale",
    "rcas",
    "grade",
    "lut",
    "split",
    "overlay",
    "panel",
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
    pub panel: bool,
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
    /// The user's choice of method: true = the AI model (ai.py), false = the shaders here.
    pub ai_framegen: bool,
    /// AI method: where the picture is at rest, a generated frame is the real one.
    pub ai_still: bool,
    /// Whether the AI runtime is installed; the panel greys the choice out without it.
    pub ai_available: bool,
    /// Set by the Python side while its AI model is the one making the frames: they
    /// then arrive ready-made and the generation here stands down.
    pub ai_active: bool,
    /// One line of status from the Python side, shown under the stats.
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
            panel: false,
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

impl Params {
    /// Set a named toggle, or flip it when `on` is None. Returns the new state.
    pub fn toggle(&mut self, name: &str, on: Option<bool>) -> Result<bool, String> {
        let slot = match name {
            "upscale" => &mut self.upscale,
            "rcas" => &mut self.rcas,
            "grade" => &mut self.grade,
            "lut" => &mut self.lut,
            "split" => &mut self.split,
            "overlay" => &mut self.overlay,
            "panel" => &mut self.panel,
            "fullscreen" => &mut self.fullscreen,
            "audio" => &mut self.audio,
            // show the HUD zone outlines for dragging (needs the panel open)
            "zones" => &mut self.edit_zones,
            _ => {
                return Err(format!(
                    "unknown pass '{name}', expected one of {TOGGLES:?}"
                ))
            }
        };
        *slot = on.unwrap_or(!*slot);
        Ok(*slot)
    }

    /// Keep values inside the ranges the shaders are written for.
    pub fn clamp(&mut self) {
        self.sharpness = self.sharpness.clamp(0.0, 2.0);
        self.exposure = self.exposure.clamp(-5.0, 5.0);
        self.contrast = self.contrast.clamp(0.1, 3.0);
        self.saturation = self.saturation.clamp(0.0, 3.0);
        self.vibrance = self.vibrance.clamp(-1.0, 1.0);
        self.temperature = self.temperature.clamp(-1.0, 1.0);
        self.tint = self.tint.clamp(-1.0, 1.0);
        self.gamma = self.gamma.clamp(0.2, 5.0);
        self.lut_strength = self.lut_strength.clamp(0.0, 1.0);
        self.split_pos = self.split_pos.clamp(0.0, 1.0);
        self.volume = self.volume.clamp(0.0, 2.0);
        self.audio_delay_ms = self.audio_delay_ms.clamp(0.0, 500.0);
        self.content_fps = self.content_fps.clamp(0.0, 240.0);
        self.target_fps = self.target_fps.clamp(0.0, 480.0);
        self.zones.truncate(MAX_ZONES);
        for zone in &mut self.zones {
            let r = zone.rect.map(|v| v.clamp(0.0, 1.0));
            zone.rect = [
                r[0].min(r[2]),
                r[1].min(r[3]),
                r[0].max(r[2]),
                r[1].max(r[3]),
            ];
        }
        for c in 0..3 {
            self.lift[c] = self.lift[c].clamp(-1.0, 1.0);
            self.lgg_gamma[c] = self.lgg_gamma[c].clamp(0.1, 10.0);
            self.gain[c] = self.gain[c].clamp(0.0, 4.0);
        }
    }

    /// Put sharpening and grading back to their defaults.
    pub fn reset_look(&mut self) {
        let d = Params::default();
        (
            self.sharpness,
            self.exposure,
            self.contrast,
            self.saturation,
            self.vibrance,
        ) = (
            d.sharpness,
            d.exposure,
            d.contrast,
            d.saturation,
            d.vibrance,
        );
        (self.temperature, self.tint, self.gamma, self.lut_strength) =
            (d.temperature, d.tint, d.gamma, d.lut_strength);
        (self.lift, self.lgg_gamma, self.gain) = (d.lift, d.lgg_gamma, d.gain);
    }

    /// The config.toml sections the options panel owns ([output], [params], [passes],
    /// [audio]), holding the current values.
    /// `panel` is left out on purpose: saving from the panel should not pin it open.
    pub fn to_toml(&self) -> String {
        // numbers as TOML floats; a bare `1` would come back as an integer
        let f = |v: f32| format!("{v:?}");
        let v3 = |v: [f32; 3]| {
            if v[0] == v[1] && v[1] == v[2] {
                f(v[0])
            } else {
                format!("[{}, {}, {}]", f(v[0]), f(v[1]), f(v[2]))
            }
        };
        let decode = match (self.full_range, self.matrix) {
            (None, None) => {
                "# full_range and matrix are left out: trust what the capture card reports\n"
                    .to_string()
            }
            (range, matrix) => {
                range
                    .map(|r| format!("full_range = {r}\n"))
                    .unwrap_or_default()
                    + &matrix
                        .map(|m| format!("matrix = \"{}\"\n", m.name()))
                        .unwrap_or_default()
            }
        };
        // each zone: left, top, right, bottom as fractions of the picture, then 1.0 for an ellipse
        let zones = self
            .zones
            .iter()
            .map(|z| {
                let [left, top, right, bottom] = z.rect.map(f);
                format!(
                    "[{left}, {top}, {right}, {bottom}, {}]",
                    f(z.ellipse as u8 as f32)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "[output]\n\
             width = {}            # 0 x 0: fit the display\n\
             height = {}\n\
             fullscreen = {}\n\
             vsync = \"{}\"    # mailbox | fifo | immediate\n\
             backend = \"{}\"     # auto | dx12 | vulkan; used at the next start\n\
             \n\
             [params]\n\
             upscaler = \"{}\"    # easu | lanczos3 | bilinear\n\
             sharpness = {}      # RCAS, AMD's scale: 0.0 is sharpest, each +1.0 halves it, up to 2.0\n\
             exposure = {}       # stops\n\
             contrast = {}\n\
             saturation = {}\n\
             vibrance = {}\n\
             temperature = {}    # -1 cool .. +1 warm\n\
             tint = {}           # -1 green .. +1 magenta\n\
             gamma = {}\n\
             lift = {}           # a number, or [r, g, b]\n\
             lgg_gamma = {}\n\
             gain = {}\n\
             lut_path = {}       # path to a .cube file\n\
             lut_strength = {}\n\
             split_pos = {}\n\
             content_fps = {}    # the game's own frame rate; 0.0 measures it\n\
             target_fps = {}     # frame generation up to this rate; 0.0 is off\n\
             hud_zones = [{}]    # kept out of frame generation: [left, top, right, bottom, 1.0 if round]\n\
             ai_framegen = {}   # true: frames from the AI model (see README); false: built-in\n\
             ai_still = {}       # AI method: still areas are left as the real picture\n\
             {decode}\
             \n\
             [passes]\n\
             upscale = {}       # false: plain bilinear\n\
             rcas = {}\n\
             grade = {}\n\
             lut = {}\n\
             split = {}        # before / after, drag the divider\n\
             overlay = {}       # fps, latency, per-pass GPU time\n\
             \n\
             [audio]\n\
             enabled = {}\n\
             volume = {}\n\
             delay_ms = {}       # hold the sound back, if it arrives before the picture\n",
            self.out_size.0,
            self.out_size.1,
            self.fullscreen,
            self.vsync.name(),
            self.backend.name(),
            self.upscaler.name(),
            f(self.sharpness),
            f(self.exposure),
            f(self.contrast),
            f(self.saturation),
            f(self.vibrance),
            f(self.temperature),
            f(self.tint),
            f(self.gamma),
            v3(self.lift),
            v3(self.lgg_gamma),
            v3(self.gain),
            // Rust's quoted form doubles the backslashes of a Windows path, as TOML needs
            format_args!("{:?}", self.lut_path),
            f(self.lut_strength),
            f(self.split_pos),
            f(self.content_fps),
            f(self.target_fps),
            zones,
            self.ai_framegen,
            self.ai_still,
            self.upscale,
            self.rcas,
            self.grade,
            self.lut,
            self.split,
            self.overlay,
            self.audio,
            f(self.volume),
            f(self.audio_delay_ms),
        )
    }
}

/// Per-frame facts the uniform block needs besides `Params`.
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

impl Globals {
    pub fn new(p: &Params, f: &FrameInfo) -> Self {
        let size = |(w, h): (u32, u32)| {
            [
                w as f32,
                h as f32,
                1.0 / w.max(1) as f32,
                1.0 / h.max(1) as f32,
            ]
        };
        let v4 = |v: [f32; 3], w: f32| [v[0], v[1], v[2], w];
        let (lut_n, lut_min, lut_max) = f.lut.unwrap_or((2, [0.0; 3], [1.0; 3]));
        let lut_scale = std::array::from_fn(|i| 1.0 / (lut_max[i] - lut_min[i]).max(1e-6));
        let wb = wb_gains(p.temperature, p.tint);
        Self {
            src_size: size(f.src),
            out_size: size(f.out),
            dst_size: size(f.dst),
            yuv: yuv_rows(f.matrix, f.full_range),
            grade: [p.exposure.exp2(), p.contrast, p.saturation, p.vibrance],
            wb: v4(wb, 1.0 / p.gamma),
            lift: v4(p.lift, f.phase),
            inv_lgg_gamma: v4(p.lgg_gamma.map(|g| 1.0 / g), 0.0),
            gain: v4(p.gain, 0.0),
            lut_min: v4(lut_min, 0.0),
            lut_scale: v4(lut_scale, 0.0),
            misc: [
                (-p.sharpness).exp2(),
                p.lut_strength,
                p.split_pos,
                lut_n as f32,
            ],
            flags: [
                f.layout,
                p.split as u32,
                p.zones.len().min(MAX_ZONES) as u32,
                // one bit per zone: 1 = ellipse
                p.zones
                    .iter()
                    .take(MAX_ZONES)
                    .enumerate()
                    .fold(0, |bits, (i, z)| bits | (z.ellipse as u32) << i),
            ],
            zones: std::array::from_fn(|i| p.zones.get(i).map_or([0.0; 4], |z| z.rect)),
        }
    }
}

/// Rows of the YUV -> R'G'B' transform: `rgb[i] = dot(rows[i], vec4(y, cb, cr, 1))`,
/// with y/cb/cr as sampled (0..1).
pub fn yuv_rows(matrix: Matrix, full_range: bool) -> [[f32; 4]; 3] {
    let (kr, kb) = match matrix {
        Matrix::Bt601 => (0.299_f64, 0.114_f64),
        Matrix::Bt709 => (0.2126, 0.0722),
    };
    let kg = 1.0 - kr - kb;
    // limited range: luma 16..235, chroma 16..240, of 255
    let (y_scale, y_off, c_scale) = if full_range {
        (1.0, 0.0, 1.0)
    } else {
        (255.0 / 219.0, 16.0 / 255.0, 255.0 / 224.0)
    };
    let c_off = 128.0 / 255.0;
    let coeffs = [
        [1.0, 0.0, 2.0 * (1.0 - kr)],
        [
            1.0,
            -2.0 * kb * (1.0 - kb) / kg,
            -2.0 * kr * (1.0 - kr) / kg,
        ],
        [1.0, 2.0 * (1.0 - kb), 0.0],
    ];
    coeffs.map(|[y, cb, cr]| {
        let (y, cb, cr) = (y * y_scale, cb * c_scale, cr * c_scale);
        [
            y as f32,
            cb as f32,
            cr as f32,
            -(y * y_off + (cb + cr) * c_off) as f32,
        ]
    })
}

/// White balance as RGB gains, normalised so grey keeps its brightness.
// shortcut: plain channel gains, not a chromatic adaptation; switch to Bradford if tints look off
pub fn wb_gains(temperature: f32, tint: f32) -> [f32; 3] {
    let g = [
        1.0 + 0.25 * temperature,
        1.0 - 0.25 * tint,
        1.0 - 0.25 * temperature,
    ];
    let luma = 0.2126 * g[0] + 0.7152 * g[1] + 0.0722 * g[2];
    g.map(|c| c / luma)
}

/// A measured frame rate wobbles around the true one; pull it to the standard rate it is near.
pub fn snap_fps(measured: f32) -> f32 {
    const RATES: [f32; 7] = [24.0, 25.0, 30.0, 50.0, 60.0, 100.0, 120.0];
    let nearest = RATES
        .into_iter()
        .min_by(|a, b| (measured - a).abs().total_cmp(&(measured - b).abs()))
        .unwrap_or(measured);
    if (measured - nearest).abs() <= nearest * 0.06 {
        nearest
    } else {
        measured
    }
}

/// Frame generation plan: how many pictures to show per content frame (1 = off), and
/// how long a content frame lasts in seconds. Only whole multiples are generated, so
/// a target of 100 on 30 fps content gives 3x = 90.
pub fn frame_plan(content_setting: f32, measured_fps: f32, target_fps: f32) -> (u32, f32) {
    let content = if content_setting > 0.0 {
        content_setting
    } else {
        snap_fps(measured_fps)
    };
    if target_fps <= 0.0 || content <= 0.0 {
        return (1, 0.0);
    }
    let multiple = (target_fps / content).round().clamp(1.0, 8.0) as u32;
    (multiple, 1.0 / content)
}

/// Largest size with the source's aspect ratio that fits inside the window.
pub fn fit(src: (u32, u32), win: (u32, u32)) -> (u32, u32) {
    let (sw, sh, ww, wh) = (
        src.0.max(1) as u64,
        src.1.max(1) as u64,
        win.0.max(1) as u64,
        win.1.max(1) as u64,
    );
    if ww * sh <= wh * sw {
        (ww as u32, ((ww * sh + sw / 2) / sw).max(1) as u32)
    } else {
        (((wh * sw + sh / 2) / sh).max(1) as u32, wh as u32)
    }
}
