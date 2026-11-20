//! The render thread: the picture window, the settings window, and the per-frame loop.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::Key;
use winit::window::{Fullscreen, Window, WindowId};

use crate::overlay::{Action, Overlay};
use crate::params::{fit, frame_plan, Backend, FrameInfo, Globals, Params, Vsync};
use crate::pipeline::{Pipeline, Step, FILES};
use crate::shaders::ShaderDir;
use crate::shared::{lock, Event, Shared, Stats, Wake};
use crate::timing::{Timing, SLOTS};

const HOT_RELOAD_EVERY: Duration = Duration::from_millis(250);
/// Redraws that only the UI asked for wait this long after the previous frame. A mouse
/// reports movement up to 1000 times a second; without the gap each report would
/// run the whole pipeline.
const UI_REDRAW_GAP: Duration = Duration::from_millis(16);
/// How many picture-to-picture intervals the steadiness figure looks back over.
const INTERVALS: usize = 600;

type Ready = Sender<Result<Arc<Shared>, String>>;

/// Start the render thread and wait until its window and GPU pipeline are up.
pub fn spawn(shader_dir: PathBuf, params: Params) -> Result<Arc<Shared>, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("capscale-render".into())
        .spawn(move || run(shader_dir, params, tx))
        .map_err(|e| e.to_string())?;
    rx.recv()
        .map_err(|_| "render thread stopped during start-up".to_string())?
}

fn run(shader_dir: PathBuf, params: Params, ready: Ready) {
    let mut builder = EventLoop::<Wake>::with_user_event();
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        builder.with_any_thread(true);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use winit::platform::wayland::EventLoopBuilderExtWayland;
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoopBuilderExtX11::with_any_thread(&mut builder, true);
        EventLoopBuilderExtWayland::with_any_thread(&mut builder, true);
    }
    let event_loop = match builder.build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            let _ = ready.send(Err(format!("cannot create the event loop: {e}")));
            return;
        }
    };
    let proxy = event_loop.create_proxy();
    let mut app = App {
        init: Some((shader_dir, params, ready)),
        proxy,
        gfx: None,
    };
    let _ = event_loop.run_app(&mut app);
    if let Some(gfx) = &app.gfx {
        gfx.shared.closed.store(true, Ordering::Release);
    }
}

struct App {
    init: Option<(PathBuf, Params, Ready)>,
    proxy: EventLoopProxy<Wake>,
    gfx: Option<Gfx>,
}

/// The settings window: egui only, drawn when its UI changes and a few times a second
/// for the live numbers in it.
struct Settings {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    overlay: Overlay,
    /// The UI changed and is waiting for a redraw (see UI_REDRAW_GAP).
    dirty: bool,
    last_render: Instant,
    shown: bool,
}

struct Gfx {
    /// The picture window; hidden while not capturing.
    window: Arc<Window>,
    /// Whether the picture window is showing.
    shown: bool,
    settings: Settings,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    present_modes: Vec<wgpu::PresentMode>,
    shared: Arc<Shared>,
    pipeline: Pipeline,
    shaders: ShaderDir,
    shader_errors: Vec<String>,
    timing: Timing,
    overlay: Overlay,
    source_generation: u64,
    lut_generation: u64,
    shown_frame: u64,
    last_render: Instant,
    render_fps: f32,
    last_hot_reload: Instant,
    fullscreen: bool,
    cursor_visible: bool,
    cursor_x: f64,
    dragging: bool,
    /// The UI changed and is waiting for a redraw (see UI_REDRAW_GAP).
    ui_dirty: bool,
    /// The split divider or the HUD zones are showing, so pointer events matter.
    interactive: bool,
    rendered: u64,
    /// Seconds between the most recent pictures, newest last.
    intervals: VecDeque<f32>,
    /// Frame count at the last draw, to tell when a new frame has arrived.
    converted_frame: u64,
    /// When the next generated picture is due.
    next_tick: Option<Instant>,
}

fn present_mode(supported: &[wgpu::PresentMode], want: Vsync) -> wgpu::PresentMode {
    use wgpu::PresentMode::{Fifo, Immediate, Mailbox};
    let order: &[wgpu::PresentMode] = match want {
        Vsync::Mailbox => &[Mailbox, Fifo],
        Vsync::Immediate => &[Immediate, Mailbox, Fifo],
        Vsync::Fifo => &[Fifo],
    };
    order
        .iter()
        .copied()
        .find(|m| supported.contains(m))
        .unwrap_or(Fifo)
}

fn smooth(average: &mut f32, sample: f32) {
    *average = if *average == 0.0 {
        sample
    } else {
        *average * 0.9 + sample * 0.1
    };
}

impl Gfx {
    fn new(
        el: &ActiveEventLoop,
        shader_dir: PathBuf,
        params: Params,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<Self, String> {
        let attributes = Window::default_attributes()
            .with_title("CapScale")
            .with_inner_size(PhysicalSize::new(1280, 720))
            .with_visible(false);
        let window = Arc::new(
            el.create_window(attributes)
                .map_err(|e| format!("cannot create the window: {e}"))?,
        );
        let settings_window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title("CapScale")
                    .with_inner_size(LogicalSize::new(460, 760)),
            )
            .map_err(|e| format!("cannot create the settings window: {e}"))?,
        );

        let mut instance_options = wgpu::InstanceDescriptor::new_with_display_handle_from_env(
            Box::new(el.owned_display_handle()),
        );
        match params.backend {
            Backend::Auto => {}
            Backend::Dx12 => instance_options.backends = wgpu::Backends::DX12,
            Backend::Vulkan => instance_options.backends = wgpu::Backends::VULKAN,
        }
        let instance = wgpu::Instance::new(instance_options);
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let settings_surface = instance
            .create_surface(settings_window.clone())
            .map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| {
            format!(
                "no usable GPU for backend \"{}\": {e}",
                params.backend.name()
            )
        })?;
        let timestamps = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("capscale"),
            required_features: if timestamps {
                wgpu::Features::TIMESTAMP_QUERY
            } else {
                wgpu::Features::empty()
            },
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;

        let caps = surface.get_capabilities(&adapter);
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("the GPU cannot present to this window")?;
        config.format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(config.format);
        config.present_mode = present_mode(&caps.present_modes, params.vsync);
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);
        let size = settings_window.inner_size();
        let settings_config = wgpu::SurfaceConfiguration {
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            ..config.clone()
        };
        settings_surface.configure(&device, &settings_config);
        let settings = Settings {
            overlay: Overlay::new(&device, config.format, &settings_window),
            window: settings_window,
            surface: settings_surface,
            config: settings_config,
            dirty: true,
            last_render: Instant::now(),
            shown: true,
        };

        let shaders = ShaderDir::new(shader_dir);
        let pipeline = Pipeline::new(&device, config.format, &shaders)?;
        let timing = Timing::new(&device, &queue, timestamps);
        let overlay = Overlay::new(&device, config.format, &window);
        let stats = Stats {
            adapter: format!(
                "{} ({:?})",
                adapter.get_info().name,
                adapter.get_info().backend
            ),
            ..Default::default()
        };
        let shared = Arc::new(Shared {
            device,
            queue,
            source: Mutex::default(),
            params: Mutex::new(params),
            lut: Mutex::new((0, None)),
            stats: Mutex::new(stats),
            events: Mutex::default(),
            sources: Mutex::default(),
            proxy: Mutex::new(proxy),
            closed: false.into(),
        });
        Ok(Self {
            window,
            shown: false,
            settings,
            surface,
            config,
            present_modes: caps.present_modes,
            shared,
            pipeline,
            shaders,
            shader_errors: vec![String::new(); FILES.len()],
            timing,
            overlay,
            source_generation: 0,
            lut_generation: 0,
            shown_frame: 0,
            last_render: Instant::now(),
            render_fps: 0.0,
            last_hot_reload: Instant::now(),
            fullscreen: false,
            cursor_visible: true,
            cursor_x: 0.0,
            dragging: false,
            ui_dirty: false,
            interactive: false,
            rendered: 0,
            intervals: VecDeque::with_capacity(INTERVALS),
            converted_frame: 0,
            next_tick: None,
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.shared.device, &self.config);
            self.window.request_redraw();
        }
    }

    /// Recompile shaders saved since the last look.
    fn hot_reload(&mut self) {
        if self.last_hot_reload.elapsed() < HOT_RELOAD_EVERY {
            return;
        }
        self.last_hot_reload = Instant::now();
        let changed = self.shaders.changed(&FILES);
        for &index in &changed {
            self.shader_errors[index] = self
                .pipeline
                .reload(&self.shared.device, &self.shaders, index)
                .err()
                .unwrap_or_default();
        }
        if !changed.is_empty() {
            self.window.request_redraw();
        }
    }

    /// Move the split-screen divider to the cursor.
    fn drag_divider(&mut self) {
        let (out_w, out_h) = self.pipeline.out_size;
        if out_w == 0 || out_h == 0 {
            return;
        }
        let (dst_w, dst_h) = (self.config.width as f64, self.config.height as f64);
        let image_w = out_w as f64 * (dst_w / out_w as f64).min(dst_h / out_h as f64);
        let position = (self.cursor_x - (dst_w - image_w) / 2.0) / image_w;
        let mut params = lock(&self.shared.params);
        if params.split {
            params.split_pos = position.clamp(0.0, 1.0) as f32;
            self.ui_dirty = true;
        }
    }

    /// Show or hide the two windows to match `capture` and `panel`.
    fn sync_windows(&mut self) {
        let (capture, panel, fullscreen) = {
            let mut p = lock(&self.shared.params);
            if !p.capture {
                p.panel = true;
            }
            (p.capture, p.panel, p.fullscreen)
        };
        if capture != self.shown {
            self.shown = capture;
            if capture {
                self.fullscreen = fullscreen;
                self.window
                    .set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
                self.window.set_visible(true);
                self.window.focus_window();
                self.window.request_redraw();
            } else {
                self.window.set_visible(false);
            }
        }
        if panel != self.settings.shown {
            self.settings.shown = panel;
            self.settings.window.set_visible(panel);
            if panel {
                self.settings.window.focus_window();
                self.settings.dirty = true;
            }
        }
    }

    /// Draw the settings window and act on what was clicked in it.
    fn render_settings(&mut self) {
        let shared = self.shared.clone();
        let (device, queue) = (&shared.device, &shared.queue);
        let st = &mut self.settings;
        st.dirty = false;
        st.last_render = Instant::now();
        use wgpu::CurrentSurfaceTexture as Current;
        let frame = match st.surface.get_current_texture() {
            Current::Success(frame) | Current::Suboptimal(frame) => frame,
            Current::Outdated | Current::Lost => {
                st.surface.configure(device, &st.config);
                st.dirty = true;
                return;
            }
            Current::Timeout | Current::Occluded | Current::Validation => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut params = lock(&shared.params).clone();
        let before = params.clone();
        let stats = lock(&shared.stats).clone();
        let mut sources = lock(&shared.sources);
        let (action, animating) = st.overlay.prepare(
            &st.window,
            device,
            queue,
            &mut encoder,
            [st.config.width, st.config.height],
            [0.0; 4],
            &mut params,
            &stats,
            Some(&mut sources),
        );
        match action {
            Action::None => {}
            Action::Start => {
                if let Some((name, _, _)) = sources.devices.get(sources.device) {
                    let (w, h) = sources.size;
                    lock(&shared.events).push(Event::Start(name.clone(), w, h, sources.fps));
                }
            }
            Action::Stop => {
                params.capture = false;
                lock(&shared.events).push(Event::Stop);
            }
            Action::Refresh => {
                sources.listed = false;
                lock(&shared.events).push(Event::RefreshSources);
            }
            Action::PickLut => lock(&shared.events).push(Event::PickLut),
            Action::Save => lock(&shared.events).push(Event::Save(params.to_toml())),
            Action::ClearLut => {
                params.lut_path.clear();
                let mut lut = lock(&shared.lut);
                lut.0 += 1;
                lut.1 = None;
            }
        }
        drop(sources);
        if params != before {
            *lock(&shared.params) = params.clone();
            self.window.request_redraw();
        }
        st.dirty = animating;
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("settings"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            st.overlay.paint(&mut pass);
        }
        queue.submit([encoder.finish()]);
        st.window.pre_present_notify();
        queue.present(frame);
    }

    /// An event for the settings window.
    fn settings_event(&mut self, el: &ActiveEventLoop, event: WindowEvent) {
        let st = &mut self.settings;
        let (_, repaint) = st.overlay.on_event(&st.window, &event);
        if repaint && !matches!(event, WindowEvent::RedrawRequested) {
            st.dirty = true;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.shared.closed.store(true, Ordering::Release);
                lock(&self.shared.events).push(Event::Closed);
                el.exit();
            }
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                st.config.width = size.width;
                st.config.height = size.height;
                st.surface.configure(&self.shared.device, &st.config);
                st.dirty = true;
            }
            WindowEvent::RedrawRequested => self.render_settings(),
            _ => {}
        }
    }

    fn render(&mut self) {
        let shared = self.shared.clone();
        let (device, queue) = (&shared.device, &shared.queue);
        self.ui_dirty = false;
        self.timing.begin_frame(device);

        let (fmt, src_size, frames, src_range, src_matrix, decode_ms, pushed_at, capture_fps) = {
            let s = lock(&shared.source);
            if s.generation != self.source_generation {
                self.pipeline.set_source(device, &s.views, s.size);
                self.source_generation = s.generation;
            }
            (
                s.fmt,
                s.size,
                s.frames,
                s.full_range,
                s.matrix,
                s.decode_ms,
                s.pushed_at,
                s.capture_fps,
            )
        };
        {
            let lut = lock(&shared.lut);
            if lut.0 != self.lut_generation {
                self.pipeline.set_lut(device, queue, lut.1.as_ref());
                self.lut_generation = lut.0;
            }
        }
        let mut params = lock(&shared.params).clone();
        if !params.capture {
            return;
        }
        let before = params.clone();

        if params.fullscreen != self.fullscreen {
            self.fullscreen = params.fullscreen;
            self.window
                .set_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)));
        }
        let mode = present_mode(&self.present_modes, params.vsync);
        if mode != self.config.present_mode {
            self.config.present_mode = mode;
            self.surface.configure(device, &self.config);
        }
        let cursor_visible = !self.fullscreen || params.split || params.edit_zones;
        if cursor_visible != self.cursor_visible {
            self.cursor_visible = cursor_visible;
            self.window.set_cursor_visible(cursor_visible);
        }

        let dst = (self.config.width, self.config.height);
        let limit = device.limits().max_texture_dimension_2d;
        let out = match params.out_size {
            (0, 0) if frames > 0 => fit(src_size, dst),
            (0, 0) => dst,
            (w, h) => (w.min(limit), h.min(limit)),
        };
        self.pipeline.set_out_size(device, out);

        use wgpu::CurrentSurfaceTexture as Current;
        let frame = match self.surface.get_current_texture() {
            Current::Success(frame) | Current::Suboptimal(frame) => frame,
            Current::Outdated | Current::Lost => {
                self.surface.configure(device, &self.config);
                self.ui_dirty = true;
                self.last_render = Instant::now();
                return;
            }
            Current::Timeout | Current::Occluded | Current::Validation => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());

        let stats = lock(&shared.stats).clone();
        let fit_scale = (dst.0 as f32 / out.0 as f32).min(dst.1 as f32 / out.1 as f32);
        let (image_w, image_h) = (out.0 as f32 * fit_scale, out.1 as f32 * fit_scale);
        let image = [
            ((dst.0 as f32 - image_w) / 2.0).floor(),
            ((dst.1 as f32 - image_h) / 2.0).floor(),
            image_w,
            image_h,
        ];
        let (_, animating) = self.overlay.prepare(
            &self.window,
            device,
            queue,
            &mut encoder,
            [dst.0, dst.1],
            image,
            &mut params,
            &stats,
            None,
        );
        if params != before {
            *lock(&shared.params) = params.clone();
        }
        self.ui_dirty = animating || params != before;
        self.interactive = params.split || params.edit_zones;

        let new_frame = frames != self.converted_frame;
        self.converted_frame = frames;
        let measured = if params.detected_fps > 0.0 {
            params.detected_fps
        } else {
            capture_fps
        };
        let (multiple, period) = frame_plan(params.content_fps, measured, params.target_fps);
        let generating = multiple > 1 && frames >= 2 && !params.ai_active;
        let mut phase = None;
        self.next_tick = None;
        if generating {
            let step = period / multiple as f32;
            let since = Instant::now().duration_since(pushed_at).as_secs_f32();
            let index = ((since / step) as u32).min(multiple - 1);
            if index + 1 < multiple {
                phase = Some((index + 1) as f32 / multiple as f32);
                self.next_tick =
                    Some(pushed_at + Duration::from_secs_f32(step * (index + 1) as f32));
            }
        }

        let full_range = params.full_range.unwrap_or(src_range);
        let matrix = params.matrix.unwrap_or(src_matrix);
        let globals = Globals::new(
            &params,
            &FrameInfo {
                src: src_size,
                out,
                dst,
                layout: fmt.layout(),
                full_range,
                matrix,
                lut: self.pipeline.lut_info,
                phase: phase.unwrap_or(1.0),
            },
        );
        let composite = if frames > 0 {
            let step = Step {
                new_frame,
                generating,
                phase,
            };
            self.pipeline
                .encode(&mut encoder, queue, &globals, &params, &self.timing, step)
        } else {
            None
        };
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: self.timing.writes(SLOTS - 1),
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            if let Some(input) = composite {
                self.pipeline.draw_composite(&mut pass, input);
            }
            self.overlay.paint(&mut pass);
        }
        self.timing.resolve(&mut encoder);
        queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        queue.present(frame);
        self.timing.end_frame();

        let now = Instant::now();
        let interval = now.duration_since(self.last_render).as_secs_f32().max(1e-6);
        smooth(&mut self.render_fps, 1.0 / interval);
        self.last_render = now;
        let mut stats = lock(&shared.stats);
        stats.render_fps = self.render_fps;
        stats.capture_fps = capture_fps;
        stats.decode_ms = decode_ms;
        if frames != self.shown_frame && phase.is_none() {
            self.shown_frame = frames;
            stats.present_ms = now.duration_since(pushed_at).as_secs_f32() * 1e3;
        }
        stats.multiple = if generating { multiple } else { 1 };
        stats.content_fps = if generating {
            1.0 / period
        } else {
            capture_fps
        };
        let held = (stats.pass_ms[0], stats.pass_ms[1]);
        stats.pass_ms = self.timing.ms;
        if generating {
            if stats.pass_ms[0] == 0.0 {
                stats.pass_ms[0] = held.0;
            }
            if stats.pass_ms[1] == 0.0 {
                stats.pass_ms[1] = held.1;
            }
        }
        stats.gpu_timing = self.timing.available();
        stats.frames = frames;
        self.rendered += 1;
        stats.rendered = self.rendered;
        if self.intervals.len() == INTERVALS {
            self.intervals.pop_front();
        }
        self.intervals.push_back(interval);
        if self.rendered.is_multiple_of(30) && self.intervals.len() >= 120 {
            let mut sorted: Vec<f32> = self.intervals.iter().copied().collect();
            sorted.sort_by(f32::total_cmp);
            stats.low_fps = 1.0 / sorted[sorted.len() * 99 / 100].max(1e-6);
        }
        stats.source = format!(
            "{}x{} {} {} {}",
            src_size.0,
            src_size.1,
            fmt.name(),
            if full_range { "full" } else { "limited" },
            matrix.name()
        );
        stats.output = out;
        stats.upscaler = if params.upscale {
            params.upscaler.name()
        } else {
            "bilinear"
        };
        stats.present_mode = format!("{:?}", self.config.present_mode).to_lowercase();
        stats.error = self
            .shader_errors
            .iter()
            .filter(|e| !e.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
    }
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let Some((shader_dir, params, ready)) = self.init.take() else {
            return;
        };
        match Gfx::new(el, shader_dir, params, self.proxy.clone()) {
            Ok(gfx) => {
                let _ = ready.send(Ok(gfx.shared.clone()));
                self.gfx = Some(gfx);
            }
            Err(e) => {
                let _ = ready.send(Err(e));
                el.exit();
            }
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, wake: Wake) {
        match (wake, &self.gfx) {
            (Wake::Redraw, Some(gfx)) => gfx.window.request_redraw(),
            (Wake::Close, _) => el.exit(),
            _ => {}
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        if id == gfx.settings.window.id() {
            gfx.settings_event(el, event);
            return;
        }
        let (used_by_ui, repaint) = gfx.overlay.on_event(&gfx.window, &event);
        if repaint && gfx.interactive && !matches!(event, WindowEvent::RedrawRequested) {
            gfx.ui_dirty = true;
        }
        match event {
            WindowEvent::CloseRequested => {
                lock(&gfx.shared.params).capture = false;
                lock(&gfx.shared.events).push(Event::Stop);
            }
            WindowEvent::Resized(size) => gfx.resize(size),
            WindowEvent::RedrawRequested => gfx.render(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } if !gfx.overlay.wants_keyboard() => {
                let name = match logical_key {
                    Key::Named(named) => format!("{named:?}"),
                    Key::Character(c) => c.to_lowercase(),
                    _ => return,
                };
                lock(&gfx.shared.events).push(Event::Key(name));
            }
            WindowEvent::CursorMoved { position, .. } => {
                gfx.cursor_x = position.x;
                if gfx.dragging {
                    gfx.drag_divider();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                gfx.dragging = state == ElementState::Pressed && !used_by_ui;
                if gfx.dragging {
                    gfx.drag_divider();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let mut wake = Instant::now() + HOT_RELOAD_EVERY;
        if let Some(gfx) = self.gfx.as_mut() {
            gfx.sync_windows();
            gfx.hot_reload();
            let st = &mut gfx.settings;
            if st.shown {
                let due = st.last_render + if st.dirty { UI_REDRAW_GAP } else { HOT_RELOAD_EVERY };
                if Instant::now() >= due {
                    st.window.request_redraw();
                } else {
                    wake = wake.min(due);
                }
            }
            if gfx.ui_dirty {
                let due = gfx.last_render + UI_REDRAW_GAP;
                if Instant::now() >= due {
                    gfx.window.request_redraw();
                } else {
                    wake = due;
                }
            }
            if let Some(tick) = gfx.next_tick {
                if Instant::now() >= tick {
                    gfx.next_tick = None;
                    gfx.window.request_redraw();
                } else {
                    wake = wake.min(tick);
                }
            }
        }
        el.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}
