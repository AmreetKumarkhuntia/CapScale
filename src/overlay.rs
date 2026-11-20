//! UI drawn with egui: the stats overlay and HUD zones on the picture, and the
//! settings window.

use egui_wgpu::ScreenDescriptor;
use winit::window::Window;

use crate::params::{Backend, Matrix, Params, Upscaler, Vsync, Zone, MAX_ZONES};
use crate::shared::{Sources, Stats, PASS_NAMES};

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

const OUTPUT_SIZES: [(&str, (u32, u32)); 4] = [
    ("Fit window", (0, 0)),
    ("1920 x 1080", (1920, 1080)),
    ("2560 x 1440", (2560, 1440)),
    ("3840 x 2160", (3840, 2160)),
];

/// Capture rates offered on the Source tab, as far as the mode allows.
const FPS_CHOICES: [u32; 5] = [120, 60, 50, 30, 25];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Guide,
    Source,
    Output,
    FrameRate,
    Picture,
    Audio,
}

pub struct Overlay {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    jobs: Vec<egui::ClippedPrimitive>,
    screen: ScreenDescriptor,
    free: Vec<egui::TextureId>,
    /// One id per HUD zone for its on-screen frame. egui remembers where a frame was
    /// dragged by id, so a new zone needs a new id or it would appear where an old one was.
    zone_ids: Vec<u64>,
    next_zone_id: u64,
    /// Where the picture sat in the window when those frames were placed.
    zone_image: [f32; 4],
    /// Settings window: the tab showing.
    tab: Tab,
    /// The first device list has been seen, and with it whether to open on the guide.
    tab_chosen: bool,
}

impl Overlay {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, window: &Window) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        Self {
            ctx,
            state,
            renderer: egui_wgpu::Renderer::new(device, format, Default::default()),
            jobs: Vec::new(),
            screen: ScreenDescriptor {
                size_in_pixels: [1, 1],
                pixels_per_point: 1.0,
            },
            free: Vec::new(),
            zone_ids: Vec::new(),
            next_zone_id: 0,
            zone_image: [0.0; 4],
            tab: Tab::Source,
            tab_chosen: false,
        }
    }

    /// Feed a window event to the UI. Returns (the UI used it, it wants a redraw).
    pub fn on_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> (bool, bool) {
        let response = self.state.on_window_event(window, event);
        (response.consumed, response.repaint)
    }

    pub fn wants_keyboard(&self) -> bool {
        self.ctx.egui_wants_keyboard_input()
    }

    /// Build this frame's UI and upload its geometry. With `sources` it is the settings
    /// window, else the picture window's stats and zones. Both edit `params` in place.
    /// Returns what the settings asked for, and whether the UI is mid-animation and
    /// wants another frame straight away.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        window: &Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size_in_pixels: [u32; 2],
        image: [f32; 4],
        params: &mut Params,
        stats: &Stats,
        sources: Option<&mut Sources>,
    ) -> (Action, bool) {
        if image != self.zone_image {
            self.zone_image = image;
            self.zone_ids.clear();
        }
        self.zone_ids.truncate(params.zones.len());
        while self.zone_ids.len() < params.zones.len() {
            self.zone_ids.push(self.next_zone_id);
            self.next_zone_id += 1;
        }
        let zone_ids = &self.zone_ids;

        let input = self.state.take_egui_input(window);
        let mut action = Action::None;
        if let Some(src) = &sources {
            if src.listed && !self.tab_chosen {
                self.tab_chosen = true;
                if !src.remembered {
                    self.tab = Tab::Guide;
                }
            }
        }
        let tab = &mut self.tab;
        let mut out = if let Some(sources) = sources {
            self.ctx.run_ui(input, |ui| {
                action = panel_ui(ui, params, stats, sources, tab);
            })
        } else {
            self.ctx.run_pass(input, |ctx| {
                if params.overlay {
                    stats_ui(ctx, stats, &params.note, &params.hint);
                }
                if params.edit_zones {
                    let scale = 1.0 / ctx.pixels_per_point();
                    let picture = egui::Rect::from_min_size(
                        egui::pos2(image[0], image[1]) * scale,
                        egui::vec2(image[2], image[3]) * scale,
                    );
                    zones_ui(ctx, &mut params.zones, zone_ids, picture);
                }
            })
        };
        self.state
            .handle_platform_output(window, out.platform_output);
        let animating = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|v| v.repaint_delay.is_zero());
        self.jobs = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        self.screen = ScreenDescriptor {
            size_in_pixels,
            pixels_per_point: out.pixels_per_point,
        };
        for (id, deltas) in &out.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        self.free = out.textures_delta.free.iter().copied().collect();
        out.textures_delta.clear();
        self.renderer
            .update_buffers(device, queue, encoder, &self.jobs, &self.screen);
        (action, animating)
    }

    /// Draw the prepared UI into the final pass.
    pub fn paint(&mut self, pass: &mut wgpu::RenderPass<'static>) {
        self.renderer.render(pass, &self.jobs, &self.screen);
        for id in self.free.drain(..) {
            self.renderer.free_texture(&id);
        }
    }
}

/// Each HUD zone as a red frame over the picture that can be dragged and resized.
fn zones_ui(ctx: &egui::Context, zones: &mut [Zone], ids: &[u64], picture: egui::Rect) {
    let red = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 60, 60));
    for (zone, id) in zones.iter_mut().zip(ids) {
        let [left, top, right, bottom] = zone.rect;
        let placed = egui::Rect::from_min_max(
            picture.min + egui::vec2(left, top) * picture.size(),
            picture.min + egui::vec2(right, bottom) * picture.size(),
        );
        let shown = egui::Window::new("HUD zone")
            .id(egui::Id::unique(("hud zone", *id)))
            .title_bar(false)
            .resizable(true)
            .default_rect(placed)
            .min_size([16.0, 16.0])
            .frame(egui::Frame::new().stroke(if zone.ellipse {
                egui::Stroke::NONE
            } else {
                red
            }))
            .show(ctx, |ui| {
                let (area, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
                if zone.ellipse {
                    ui.painter().add(egui::epaint::EllipseShape::stroke(
                        area.center(),
                        area.size() / 2.0,
                        red,
                    ));
                }
            });
        if let Some(shown) = shown {
            let now = shown.response.rect;
            let (min, max) = (
                (now.min - picture.min) / picture.size(),
                (now.max - picture.min) / picture.size(),
            );
            let moved = [min.x, min.y, max.x, max.y];
            if moved
                .iter()
                .zip(zone.rect)
                .any(|(a, b)| (a - b).abs() > 1e-4)
            {
                zone.rect = moved;
            }
        }
    }
}

fn stats_ui(ctx: &egui::Context, s: &Stats, note: &str, hint: &str) {
    egui::Area::new(egui::Id::unique("stats"))
        .fixed_pos([8.0, 8.0])
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(180))
                .inner_margin(egui::Margin::same(8))
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.visuals_mut().override_text_color = Some(egui::Color32::from_gray(235));
                    if s.frames == 0 {
                        ui.label("waiting for frames...");
                    }
                    if s.multiple > 1 {
                        ui.label(format!(
                            "game {:4.1} fps  x{} = {:.0} fps shown   render {:5.1} fps",
                            s.content_fps,
                            s.multiple,
                            s.content_fps * s.multiple as f32,
                            s.render_fps
                        ));
                    } else {
                        ui.label(format!(
                            "capture {:5.1} fps   render {:5.1} fps",
                            s.capture_fps, s.render_fps
                        ));
                    }
                    ui.label(format!(
                        "added latency  decode {:.1} + present {:.1} = {:.1} ms",
                        s.decode_ms,
                        s.present_ms,
                        s.decode_ms + s.present_ms
                    ));
                    if s.low_fps > 0.0 {
                        ui.label(format!("steadiness     1% low {:.1} fps", s.low_fps));
                    }
                    ui.label(format!(
                        "{}  ->  {}x{} {}  [{}]",
                        s.source, s.output.0, s.output.1, s.upscaler, s.present_mode
                    ));
                    if s.gpu_timing {
                        let passes: Vec<String> = PASS_NAMES
                            .iter()
                            .zip(s.pass_ms)
                            .filter(|(_, ms)| *ms > 0.0)
                            .map(|(name, ms)| format!("{name} {ms:.2}"))
                            .collect();
                        ui.label(format!(
                            "gpu ms  {}  = {:.2}",
                            passes.join("  "),
                            s.pass_ms.iter().sum::<f32>()
                        ));
                    } else {
                        ui.label("gpu ms  n/a (no timestamp queries on this adapter)");
                    }
                    if !s.error.is_empty() {
                        ui.colored_label(egui::Color32::from_rgb(255, 110, 110), &s.error);
                    }
                    if !note.is_empty() {
                        ui.label(note);
                    }
                    if !hint.is_empty() {
                        ui.weak(hint);
                    }
                });
        });
}

fn section(ui: &mut egui::Ui, title: &str, open: bool, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title)
        .default_open(open)
        .show(ui, body);
}

fn slider(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, text: &str) {
    ui.add(egui::Slider::new(value, range).text(text));
}

/// The Source tab: which capture card and mode, and starting or stopping capture.
fn source_ui(ui: &mut egui::Ui, p: &mut Params, s: &Stats, src: &mut Sources) -> Action {
    let mut action = Action::None;
    if !src.listed {
        ui.label("Looking for capture devices...");
    } else if src.devices.is_empty() {
        ui.label("No capture devices found. Close OBS or anything else using the card, then Refresh.");
    } else {
        src.device = src.device.min(src.devices.len() - 1);
        egui::ComboBox::from_label("Device")
            .selected_text(src.devices[src.device].1.as_str())
            .show_ui(ui, |ui| {
                for (i, (_, label, _)) in src.devices.iter().enumerate() {
                    ui.selectable_value(&mut src.device, i, label.as_str());
                }
            });
        let modes = &src.devices[src.device].2;
        let mut sizes: Vec<(u32, u32)> = Vec::new();
        for &(w, h, _) in modes {
            if !sizes.contains(&(w, h)) {
                sizes.push((w, h));
            }
        }
        if !sizes.contains(&src.size) {
            src.size = sizes.first().copied().unwrap_or_default();
        }
        egui::ComboBox::from_label("Resolution")
            .selected_text(format!("{} x {}", src.size.0, src.size.1))
            .show_ui(ui, |ui| {
                for size in sizes {
                    ui.selectable_value(&mut src.size, size, format!("{} x {}", size.0, size.1));
                }
            });
        let top = modes
            .iter()
            .filter(|m| (m.0, m.1) == src.size)
            .fold(0.0, |top: f32, m| top.max(m.2));
        let rates: Vec<u32> = FPS_CHOICES
            .into_iter()
            .filter(|&f| f as f32 <= top + 0.5)
            .collect();
        if !rates.contains(&src.fps) {
            src.fps = rates.first().copied().unwrap_or_default();
        }
        egui::ComboBox::from_label("Frame rate")
            .selected_text(src.fps.to_string())
            .show_ui(ui, |ui| {
                for &fps in &rates {
                    ui.selectable_value(&mut src.fps, fps, fps.to_string());
                }
            });
        if modes.is_empty() {
            ui.label("This device offers no MJPEG, NV12 or YUY2 mode.");
        }
        ui.horizontal(|ui| {
            let start = if p.capture { "Restart capture" } else { "Start capture" };
            if ui.add_enabled(!rates.is_empty(), egui::Button::new(start)).clicked() {
                action = Action::Start;
            }
            if ui.add_enabled(p.capture, egui::Button::new("Stop")).clicked() {
                action = Action::Stop;
            }
        });
    }
    if ui.button("Refresh devices").clicked() {
        action = Action::Refresh;
    }
    ui.separator();
    ui.label(if !p.capture {
        "not capturing"
    } else if s.frames == 0 {
        "no frames yet"
    } else {
        s.source.as_str()
    });
    ui.horizontal(|ui| {
        ui.label("Range");
        ui.selectable_value(&mut p.full_range, None, "auto");
        ui.selectable_value(&mut p.full_range, Some(false), "limited");
        ui.selectable_value(&mut p.full_range, Some(true), "full");
    });
    ui.horizontal(|ui| {
        ui.label("Matrix");
        ui.selectable_value(&mut p.matrix, None, "auto");
        ui.selectable_value(&mut p.matrix, Some(Matrix::Bt601), "bt601");
        ui.selectable_value(&mut p.matrix, Some(Matrix::Bt709), "bt709");
    });
    action
}

/// The Getting started tab: what the app does and the steps to a first picture.
fn guide_ui(ui: &mut egui::Ui, tab: &mut Tab) {
    ui.heading("Welcome to CapScale");
    ui.label(
        "CapScale shows your console through a capture card, upscaled to your monitor \
         and optionally smoothed to a higher frame rate. Everything runs on your graphics card.",
    );
    ui.add_space(8.0);
    ui.strong("Getting a picture");
    for step in [
        "1. Connect the console's HDMI to the capture card, and the card to this PC by USB.",
        "2. Close OBS, Discord streaming, or anything else using the card.",
        "3. On the Source tab pick the card, 1920 x 1080 and 60, then press Start capture.",
        "4. The picture opens in its own window. F11 toggles fullscreen, Escape stops.",
    ] {
        ui.label(step);
    }
    ui.add_space(8.0);
    ui.strong("Making it look better");
    for (name, what) in [
        ("Picture", "the upscaler, sharpening, colour grading and a .cube LUT."),
        ("Frame rate", "generates in-between frames, for example a 30 fps game to 60. Built in, or AI (RIFE) on an NVIDIA RTX card."),
        ("Output", "window size, vsync and the stats overlay."),
        ("Audio", "plays the game's sound; move the delay slider if sound comes before the picture."),
    ] {
        ui.horizontal_wrapped(|ui| {
            ui.strong(name);
            ui.label(what);
        });
    }
    ui.add_space(8.0);
    ui.strong("Keys (on the picture window)");
    ui.label("F1 settings · F2 stats · F3 before/after split · F4 next upscaler · F11 fullscreen · Escape stop");
    ui.add_space(8.0);
    ui.label("Changes apply at once. Press Save to config.toml to keep them for next time.");
    ui.add_space(8.0);
    if ui.button("Go to Source").clicked() {
        *tab = Tab::Source;
    }
}

/// The whole settings window: a row of tabs, the tab's settings, Save at the bottom.
fn panel_ui(ui: &mut egui::Ui, p: &mut Params, s: &Stats, src: &mut Sources, tab: &mut Tab) -> Action {
    let mut action = Action::None;
    egui::Panel::bottom("save").show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Save to config.toml").clicked() {
                action = Action::Save;
            }
            if ui.button("Reset look").clicked() {
                p.reset_look();
            }
        });
    });
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            for (t, name) in [
                (Tab::Guide, "Getting started"),
                (Tab::Source, "Source"),
                (Tab::Output, "Output"),
                (Tab::FrameRate, "Frame rate"),
                (Tab::Picture, "Picture"),
                (Tab::Audio, "Audio"),
            ] {
                ui.selectable_value(tab, t, name);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| match *tab {
            Tab::Guide => guide_ui(ui, tab),
            Tab::Source => action = source_ui(ui, p, s, src),
            Tab::Output => {
                let current = OUTPUT_SIZES
                    .iter()
                    .find(|(_, size)| *size == p.out_size)
                    .map_or_else(
                        || format!("{} x {}", p.out_size.0, p.out_size.1),
                        |(label, _)| label.to_string(),
                    );
                egui::ComboBox::from_label("render size")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (label, size) in OUTPUT_SIZES {
                            ui.selectable_value(&mut p.out_size, size, label);
                        }
                    });
                ui.checkbox(&mut p.fullscreen, "Borderless fullscreen");
                ui.horizontal(|ui| {
                    ui.label("Present");
                    for v in Vsync::ALL {
                        ui.selectable_value(&mut p.vsync, v, v.name());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Graphics API (next start)");
                    for b in Backend::ALL {
                        ui.selectable_value(&mut p.backend, b, b.name());
                    }
                });
                ui.label(format!("now: {}", s.adapter));
                ui.separator();
                ui.checkbox(&mut p.overlay, "Stats overlay on the picture");
                ui.checkbox(&mut p.split, "Before / after split (drag the divider)");
            }
            Tab::FrameRate => {
                ui.horizontal(|ui| {
                    ui.label("Current (game)");
                    ui.selectable_value(&mut p.content_fps, 0.0, "auto");
                    ui.selectable_value(&mut p.content_fps, 30.0, "30");
                    ui.selectable_value(&mut p.content_fps, 60.0, "60");
                });
                ui.horizontal(|ui| {
                    ui.label("Target");
                    ui.selectable_value(&mut p.target_fps, 0.0, "off");
                    for fps in [60.0, 90.0, 120.0, 180.0] {
                        ui.selectable_value(&mut p.target_fps, fps, format!("{fps:.0}"));
                    }
                });
                ui.add_enabled_ui(p.ai_available, |ui| {
                    ui.checkbox(&mut p.ai_framegen, "AI frames (RIFE)")
                        .on_hover_text("On: generated frames come from the RIFE model. Off: the built-in method.")
                        .on_disabled_hover_text("Not installed. Run CapScale AI setup from the Start menu (about 4 GB), then restart.");
                });
                ui.add_enabled_ui(p.ai_available && p.ai_framegen, |ui| {
                    ui.checkbox(&mut p.ai_still, "AI: leave still areas untouched")
                        .on_hover_text("On: where nothing moves, a generated frame shows the real picture. Off: the model draws everything.");
                });
                ui.horizontal(|ui| {
                    ui.label("HUD zones");
                    let room = p.zones.len() < MAX_ZONES;
                    if ui.add_enabled(room, egui::Button::new("+ box")).clicked() {
                        p.zones.push(Zone {
                            rect: [0.02, 0.70, 0.20, 0.97],
                            ellipse: false,
                        });
                        p.edit_zones = true;
                    }
                    if ui.add_enabled(room, egui::Button::new("+ circle")).clicked() {
                        p.zones.push(Zone {
                            rect: [0.02, 0.67, 0.19, 0.97],
                            ellipse: true,
                        });
                        p.edit_zones = true;
                    }
                    if ui.add_enabled(!p.zones.is_empty(), egui::Button::new("remove last")).clicked() {
                        p.zones.pop();
                    }
                });
                ui.checkbox(&mut p.edit_zones, "Show and move zones");
                ui.label(format!(
                    "{} zone(s). Inside a zone generated frames show the real picture, so a minimap or HUD cannot smear.",
                    p.zones.len()
                ));
                ui.label(if p.target_fps <= 0.0 {
                    "Frame generation is off.".to_string()
                } else if s.multiple > 1 {
                    let wait = 1000.0 / s.content_fps.max(1.0) * (s.multiple - 1) as f32
                        / s.multiple as f32;
                    format!(
                        "Generating {}x: {:.0} -> {:.0} fps, about {wait:.0} ms extra delay.",
                        s.multiple,
                        s.content_fps,
                        s.content_fps * s.multiple as f32
                    )
                } else {
                    format!(
                        "Game is at {:.0} fps: already at the target, nothing to generate.",
                        s.capture_fps
                    )
                });
            }
            Tab::Picture => {
                section(ui, "Upscale and sharpen", true, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut p.upscale, "Upscaler");
                        for u in Upscaler::ALL {
                            ui.selectable_value(&mut p.upscaler, u, u.label());
                        }
                    });
                    ui.checkbox(&mut p.rcas, "AMD FSR 1 sharpening (RCAS)");
                    slider(ui, &mut p.sharpness, 0.0..=2.0, "sharpness (0 = sharpest)");
                });
                section(ui, "Colour", true, |ui| {
                    ui.checkbox(&mut p.grade, "Colour grading");
                    slider(ui, &mut p.exposure, -3.0..=3.0, "exposure (stops)");
                    slider(ui, &mut p.contrast, 0.5..=2.0, "contrast");
                    slider(ui, &mut p.saturation, 0.0..=2.0, "saturation");
                    slider(ui, &mut p.vibrance, -1.0..=1.0, "vibrance");
                    slider(ui, &mut p.temperature, -1.0..=1.0, "temperature");
                    slider(ui, &mut p.tint, -1.0..=1.0, "tint");
                    slider(ui, &mut p.gamma, 0.5..=2.0, "gamma");
                    for (triple, range, text) in [
                        (&mut p.lift, -0.25..=0.25, "lift"),
                        (&mut p.lgg_gamma, 0.5..=2.0, "lgg gamma"),
                        (&mut p.gain, 0.5..=2.0, "gain"),
                    ] {
                        let mut v = triple[0];
                        slider(ui, &mut v, range, text);
                        if v != triple[0] {
                            *triple = [v; 3];
                        }
                    }
                });
                section(ui, "LUT", true, |ui| {
                    ui.checkbox(&mut p.lut, "Apply 3D LUT");
                    ui.label(if p.lut_path.is_empty() {
                        "none loaded"
                    } else {
                        p.lut_path.as_str()
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Load .cube...").clicked() {
                            action = Action::PickLut;
                        }
                        if !p.lut_path.is_empty() && ui.button("Clear").clicked() {
                            action = Action::ClearLut;
                        }
                    });
                    slider(ui, &mut p.lut_strength, 0.0..=1.0, "strength");
                });
            }
            Tab::Audio => {
                ui.checkbox(&mut p.audio, "Game audio");
                slider(ui, &mut p.volume, 0.0..=1.5, "volume");
                slider(
                    ui,
                    &mut p.audio_delay_ms,
                    0.0..=300.0,
                    "delay ms (if sound leads picture)",
                );
            }
        });
    });
    action
}
