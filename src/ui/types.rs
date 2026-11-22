use egui_wgpu::ScreenDescriptor;

/// Something the settings window asked for that it cannot do itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    None,
    Start,
    Stop,
    Refresh,
    PickLut,
    ClearLut,
    Save,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Guide,
    Source,
    Output,
    FrameRate,
    Picture,
    Audio,
}

pub struct Overlay {
    pub(super) ctx: egui::Context,
    pub(super) state: egui_winit::State,
    pub(super) renderer: egui_wgpu::Renderer,
    pub(super) jobs: Vec<egui::ClippedPrimitive>,
    pub(super) screen: ScreenDescriptor,
    pub(super) free: Vec<egui::TextureId>,
    /// One id per HUD zone for its on-screen frame. egui remembers where a frame was
    /// dragged by id, so a new zone needs a new id or it would appear where an old one was.
    pub(super) zone_ids: Vec<u64>,
    pub(super) next_zone_id: u64,
    /// Where the picture sat in the window when those frames were placed.
    pub(super) zone_image: [f32; 4],
    /// Settings window: the tab showing.
    pub(super) tab: Tab,
    /// The first device list has been seen, and with it whether to open on the guide.
    pub(super) tab_chosen: bool,
}

pub(super) const OUTPUT_SIZES: [(&str, (u32, u32)); 4] = [
    ("Fit window", (0, 0)),
    ("1920 x 1080", (1920, 1080)),
    ("2560 x 1440", (2560, 1440)),
    ("3840 x 2160", (3840, 2160)),
];

/// Capture rates offered on the Source tab, as far as the mode allows.
pub(super) const FPS_CHOICES: [u32; 5] = [120, 60, 50, 30, 25];
