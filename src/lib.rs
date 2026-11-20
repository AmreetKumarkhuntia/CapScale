//! Python bindings: the `Renderer` class. Everything else lives in its own module.

pub mod lut;
mod overlay;
pub mod params;
pub mod pipeline;
pub mod shaders;
mod shared;
pub mod source;
pub mod timing;
mod window;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use numpy::{PyReadonlyArray2, PyReadonlyArrayDyn, PyUntypedArrayMethods};
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use params::{Backend, Matrix, Params, Upscaler, Vsync, Zone};
use shared::{lock, Event, Shared, Wake, PASS_NAMES};
use source::{Fmt, Plane};

/// winit allows one event loop per process, so one Renderer.
static CREATED: AtomicBool = AtomicBool::new(false);

fn closed() -> PyErr {
    PyRuntimeError::new_err("the renderer window is closed")
}

/// A number, or three of them (r, g, b).
fn triple(value: &Bound<'_, PyAny>) -> PyResult<[f32; 3]> {
    value
        .extract::<f32>()
        .map(|v| [v; 3])
        .or_else(|_| value.extract::<[f32; 3]>())
}

/// The YUV to RGB transform the renderer uses, as three rows of four numbers:
/// `rgb[i] = row[i][0] * y + row[i][1] * cb + row[i][2] * cr + row[i][3]`, all 0..1.
/// The AI frame generation converts frames itself and must match the renderer exactly.
#[pyfunction]
fn yuv_matrix(matrix: &str, full_range: bool) -> PyResult<[[f32; 4]; 3]> {
    let matrix = Matrix::parse(matrix)
        .ok_or_else(|| PyValueError::new_err("matrix: expected bt601 or bt709"))?;
    Ok(params::yuv_rows(matrix, full_range))
}

/// How many pictures to show per game frame (1 = no generation) and how long a game
/// frame lasts in seconds: the same plan the renderer follows for its own generation.
#[pyfunction]
fn frame_plan(content_fps: f32, measured_fps: f32, target_fps: f32) -> (u32, f32) {
    params::frame_plan(content_fps, measured_fps, target_fps)
}

/// The window and GPU pipeline. Frames go in with `push_frame`; the newest one is drawn.
#[pyclass(module = "capscale")]
struct Renderer {
    shared: Arc<Shared>,
}

#[pymethods]
impl Renderer {
    /// `out_w`/`out_h` of 0 fit the output to the window. `shader_dir` defaults to the
    /// `shaders` folder inside the installed package. `backend` is "auto", "dx12" or
    /// "vulkan"; left out, it is DirectX 12 on Windows.
    #[new]
    #[pyo3(signature = (out_w=0, out_h=0, fullscreen=true, vsync="mailbox", shader_dir=None, backend=None))]
    fn new(
        py: Python<'_>,
        out_w: u32,
        out_h: u32,
        fullscreen: bool,
        vsync: &str,
        shader_dir: Option<PathBuf>,
        backend: Option<&str>,
    ) -> PyResult<Self> {
        let backend = match backend {
            None => Backend::for_this_platform(),
            Some(name) => Backend::parse(name)
                .ok_or_else(|| PyValueError::new_err("backend: expected auto, dx12 or vulkan"))?,
        };
        let vsync = Vsync::parse(vsync)
            .ok_or_else(|| PyValueError::new_err("vsync: expected mailbox, fifo or immediate"))?;
        let shader_dir = match shader_dir {
            Some(dir) => dir,
            None => {
                let package: PathBuf = py.import("capscale")?.getattr("__file__")?.extract()?;
                package.with_file_name("shaders")
            }
        };
        if CREATED.swap(true, Ordering::SeqCst) {
            return Err(PyRuntimeError::new_err(
                "only one Renderer can exist per process",
            ));
        }
        let params = Params {
            out_size: (out_w, out_h),
            fullscreen,
            vsync,
            backend,
            ..Default::default()
        };
        let shared = py
            .detach(|| window::spawn(shader_dir, params))
            .map_err(|e| {
                CREATED.store(false, Ordering::SeqCst);
                PyRuntimeError::new_err(e)
            })?;
        Ok(Self { shared })
    }

    /// Show a frame. `array` is one packed uint8 array (nv12, yuy2, or planar planes back
    /// to back) or a sequence of 2-D uint8 arrays, one per plane, whose row length may
    /// include padding. The pixels are copied to the GPU before this returns.
    #[pyo3(signature = (array, fmt, width, height, full_range=None, matrix=None, decode_ms=0.0))]
    #[allow(clippy::too_many_arguments)]
    fn push_frame(
        &self,
        py: Python<'_>,
        array: &Bound<'_, PyAny>,
        fmt: &str,
        width: u32,
        height: u32,
        full_range: Option<bool>,
        matrix: Option<&str>,
        decode_ms: f32,
    ) -> PyResult<()> {
        let started = Instant::now();
        let fmt = Fmt::parse(fmt)
            .ok_or_else(|| PyValueError::new_err("fmt: expected nv12, yuy2, i420, i422 or i444"))?;
        let matrix = match matrix {
            None => None,
            Some(m) => Some(
                Matrix::parse(m)
                    .ok_or_else(|| PyValueError::new_err("matrix: expected bt601 or bt709"))?,
            ),
        };
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(closed());
        }
        let contiguous = |_| PyValueError::new_err("frame arrays must be C-contiguous");
        let size = (width, height);
        if let Ok(packed) = array.extract::<PyReadonlyArrayDyn<'_, u8>>() {
            let planes = fmt.split_packed(width, height, packed.as_slice().map_err(contiguous)?).map_err(PyValueError::new_err)?;
            py.detach(|| self.upload(fmt, size, &planes, full_range, matrix, decode_ms, started))
        } else {
            let arrays: Vec<PyReadonlyArray2<'_, u8>> = array.extract().map_err(|_| {
                PyTypeError::new_err("array must be a uint8 ndarray, or a sequence of 2-D uint8 ndarrays (one per plane)")
            })?;
            let mut planes = Vec::with_capacity(arrays.len());
            for plane in &arrays {
                planes.push(Plane { data: plane.as_slice().map_err(contiguous)?, stride: plane.shape()[1] as u32 });
            }
            py.detach(|| self.upload(fmt, size, &planes, full_range, matrix, decode_ms, started))
        }
        .map_err(PyValueError::new_err)?;
        if self.shared.wake(Wake::Redraw) {
            Ok(())
        } else {
            Err(closed())
        }
    }

    /// Change any tunable: sharpness, exposure, contrast, saturation, vibrance, temperature,
    /// tint, gamma, lift, lgg_gamma, gain, lut_path, lut_strength, upscaler, split_pos,
    /// full_range, matrix, vsync, out_size, fullscreen, volume, audio_delay_ms,
    /// content_fps (0 = measure), target_fps (0 = no frame generation), hint (a line of
    /// text under the stats).
    #[pyo3(signature = (**kwargs))]
    fn set_params(&self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let Some(kwargs) = kwargs else { return Ok(()) };
        let mut p = lock(&self.shared.params).clone();
        let mut new_lut = None;
        for (key, value) in kwargs.iter() {
            let key: String = key.extract()?;
            let bad = |expected: &str| PyValueError::new_err(format!("{key}: expected {expected}"));
            let mut set = || -> PyResult<()> {
                match key.as_str() {
                    "sharpness" => p.sharpness = value.extract()?,
                    "exposure" => p.exposure = value.extract()?,
                    "contrast" => p.contrast = value.extract()?,
                    "saturation" => p.saturation = value.extract()?,
                    "vibrance" => p.vibrance = value.extract()?,
                    "temperature" => p.temperature = value.extract()?,
                    "tint" => p.tint = value.extract()?,
                    "gamma" => p.gamma = value.extract()?,
                    "lift" => p.lift = triple(&value)?,
                    "lgg_gamma" => p.lgg_gamma = triple(&value)?,
                    "gain" => p.gain = triple(&value)?,
                    "lut_strength" => p.lut_strength = value.extract()?,
                    "split_pos" => p.split_pos = value.extract()?,
                    "fullscreen" => p.fullscreen = value.extract()?,
                    "out_size" => p.out_size = value.extract()?,
                    "full_range" => p.full_range = value.extract()?,
                    "hint" => p.hint = value.extract()?,
                    "volume" => p.volume = value.extract()?,
                    "content_fps" => p.content_fps = value.extract()?,
                    "target_fps" => p.target_fps = value.extract()?,
                    "detected_fps" => p.detected_fps = value.extract()?,
                    "ai_framegen" => p.ai_framegen = value.extract()?,
                    "ai_still" => p.ai_still = value.extract()?,
                    "ai_available" => p.ai_available = value.extract()?,
                    "ai_active" => p.ai_active = value.extract()?,
                    "note" => p.note = value.extract()?,
                    "hud_zones" => {
                        p.zones.clear();
                        for zone in value.extract::<Vec<Vec<f32>>>()? {
                            if zone.len() != 4 && zone.len() != 5 {
                                return Err(bad("zones of [left, top, right, bottom] or [left, top, right, bottom, round]"));
                            }
                            p.zones.push(Zone {
                                rect: [zone[0], zone[1], zone[2], zone[3]],
                                ellipse: zone.get(4).is_some_and(|round| *round != 0.0),
                            });
                        }
                    }
                    "audio_delay_ms" => p.audio_delay_ms = value.extract()?,
                    "lut_path" => {
                        let path = value.extract::<Option<String>>()?.unwrap_or_default();
                        if path != p.lut_path {
                            new_lut = Some(if path.is_empty() {
                                None
                            } else {
                                Some(lut::load_cube(&path).map_err(PyValueError::new_err)?)
                            });
                            p.lut_path = path;
                        }
                    }
                    "upscaler" => {
                        let name: String = value.extract()?;
                        p.upscaler = match name.as_str() {
                            "next" => p.upscaler.next(),
                            other => Upscaler::parse(other)
                                .ok_or_else(|| bad("easu, lanczos3, bilinear or next"))?,
                        };
                    }
                    "matrix" => {
                        p.matrix = match value.extract::<Option<String>>()?.as_deref() {
                            None | Some("auto") => None,
                            Some(name) => Some(
                                Matrix::parse(name).ok_or_else(|| bad("bt601, bt709 or auto"))?,
                            ),
                        };
                    }
                    "backend" => {
                        let name: String = value.extract()?;
                        p.backend =
                            Backend::parse(&name).ok_or_else(|| bad("auto, dx12 or vulkan"))?;
                    }
                    "vsync" => {
                        let name: String = value.extract()?;
                        p.vsync =
                            Vsync::parse(&name).ok_or_else(|| bad("mailbox, fifo or immediate"))?;
                    }
                    _ => {
                        return Err(PyTypeError::new_err(format!(
                            "set_params() got an unexpected keyword argument '{key}'"
                        )))
                    }
                }
                Ok(())
            };
            set().map_err(|e| {
                let py = kwargs.py();
                let message = format!("{key}: {}", e.value(py));
                if e.to_string().contains(key.as_str()) {
                    e
                } else if e.is_instance_of::<PyValueError>(py) {
                    PyValueError::new_err(message)
                } else {
                    PyTypeError::new_err(message)
                }
            })?;
        }
        p.clamp();
        if let Some(cube) = new_lut {
            let mut lut = lock(&self.shared.lut);
            lut.0 += 1;
            lut.1 = cube;
        }
        *lock(&self.shared.params) = p;
        self.shared.wake(Wake::Redraw);
        Ok(())
    }

    /// Turn a pass or view on or off; with `on` left out, flip it. Returns the new state.
    /// Names: upscale, rcas, grade, lut, split, overlay, panel (the settings window),
    /// capture (the picture window), fullscreen, audio, zones.
    #[pyo3(signature = (name, on=None))]
    fn toggle_pass(&self, name: &str, on: Option<bool>) -> PyResult<bool> {
        let state = lock(&self.shared.params)
            .toggle(name, on)
            .map_err(PyValueError::new_err)?;
        self.shared.wake(Wake::Redraw);
        Ok(state)
    }

    /// Live numbers: fps, added latency, per-pass GPU milliseconds, what is being shown.
    #[getter]
    fn stats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let s = lock(&self.shared.stats).clone();
        let passes = PyDict::new(py);
        for (name, ms) in PASS_NAMES.iter().zip(s.pass_ms) {
            passes.set_item(name, ms)?;
        }
        let d = PyDict::new(py);
        d.set_item("capture_fps", s.capture_fps)?;
        d.set_item("render_fps", s.render_fps)?;
        d.set_item("decode_ms", s.decode_ms)?;
        d.set_item("present_ms", s.present_ms)?;
        d.set_item("latency_ms", s.decode_ms + s.present_ms)?;
        d.set_item("pass_ms", passes)?;
        d.set_item("gpu_timing", s.gpu_timing)?;
        d.set_item("frames", s.frames)?;
        d.set_item("rendered", s.rendered)?;
        d.set_item("framegen_multiple", s.multiple)?;
        d.set_item("low_fps", s.low_fps)?;
        d.set_item("source", s.source)?;
        d.set_item("output", s.output)?;
        d.set_item("upscaler", s.upscaler)?;
        d.set_item("present_mode", s.present_mode)?;
        d.set_item("adapter", s.adapter)?;
        d.set_item("error", s.error)?;
        Ok(d)
    }

    /// Window events since the last call, as (kind, value):
    /// ("key", "F1"), ("start", "<device>\t<width>\t<height>\t<fps>"), ("stop", ""),
    /// ("refresh_sources", ""), ("pick_lut", ""), ("save", <config text>), ("closed", "").
    fn poll_events(&self) -> Vec<(&'static str, String)> {
        std::mem::take(&mut *lock(&self.shared.events))
            .into_iter()
            .map(|event| match event {
                Event::Key(name) => ("key", name),
                Event::Start(device, w, h, fps) => ("start", format!("{device}\t{w}\t{h}\t{fps}")),
                Event::Stop => ("stop", String::new()),
                Event::RefreshSources => ("refresh_sources", String::new()),
                Event::PickLut => ("pick_lut", String::new()),
                Event::Save(toml) => ("save", toml),
                Event::Closed => ("closed", String::new()),
            })
            .collect()
    }

    /// The capture devices for the Source tab, as (name, label, [(width, height, max fps)]),
    /// with the given one picked if it is there.
    #[pyo3(signature = (devices, device="", width=1920, height=1080, fps=60))]
    fn set_sources(
        &self,
        devices: Vec<shared::Device>,
        device: &str,
        width: u32,
        height: u32,
        fps: u32,
    ) {
        let mut s = lock(&self.shared.sources);
        s.device = devices.iter().position(|d| d.0 == device).unwrap_or(0);
        s.devices = devices;
        (s.size, s.fps, s.listed) = ((width, height), fps, true);
        drop(s);
        self.shared.wake(Wake::Redraw);
    }

    /// Current settings as the [output], [params], [passes] and [audio] sections of config.toml.
    fn config_toml(&self) -> String {
        lock(&self.shared.params).to_toml()
    }

    /// (frame generation is on, the game rate the user set or 0 for "work it out",
    /// the target rate, the AI method is chosen). With it on, capture passes each game
    /// frame once instead of every repeat.
    #[getter]
    fn framegen(&self) -> (bool, f32, f32, bool) {
        let p = lock(&self.shared.params);
        (
            p.target_fps > 0.0,
            p.content_fps,
            p.target_fps,
            p.ai_framegen,
        )
    }

    /// AI method: areas at rest are to be left as the real picture.
    #[getter]
    fn ai_still(&self) -> bool {
        lock(&self.shared.params).ai_still
    }

    /// The HUD zones as (left, top, right, bottom, round), fractions of the picture.
    #[getter]
    fn hud_zones(&self) -> Vec<(f32, f32, f32, f32, bool)> {
        lock(&self.shared.params)
            .zones
            .iter()
            .map(|z| (z.rect[0], z.rect[1], z.rect[2], z.rect[3], z.ellipse))
            .collect()
    }

    /// (on, volume, delay in ms) as set in the options panel or by `set_params`.
    /// Playback is done on the Python side, which reads this.
    #[getter]
    fn audio(&self) -> (bool, f32, f32) {
        let p = lock(&self.shared.params);
        (p.audio, p.volume, p.audio_delay_ms)
    }

    /// Close the window and stop the render thread.
    fn close(&self) {
        self.shared.wake(Wake::Close);
    }
}

impl Renderer {
    #[allow(clippy::too_many_arguments)]
    fn upload(
        &self,
        fmt: Fmt,
        size: (u32, u32),
        planes: &[Plane<'_>],
        full_range: Option<bool>,
        matrix: Option<Matrix>,
        decode_ms: f32,
        started: Instant,
    ) -> Result<(), String> {
        let shared = &self.shared;
        let mut source = lock(&shared.source);
        source.upload(&shared.device, &shared.queue, fmt, size, planes)?;
        let gap = started.duration_since(source.pushed_at).as_secs_f32();
        if source.frames > 0 && gap > 0.0 {
            let fps = 1.0 / gap;
            source.capture_fps = if source.capture_fps == 0.0 {
                fps
            } else {
                source.capture_fps * 0.9 + fps * 0.1
            };
        }
        source.pushed_at = started;
        source.frames += 1;
        source.decode_ms = decode_ms;
        source.full_range = full_range.unwrap_or(false);
        source.matrix = matrix.unwrap_or(if size.1 >= 720 {
            Matrix::Bt709
        } else {
            Matrix::Bt601
        });
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.close();
    }
}

#[pymodule(name = "_core")]
fn core_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(yuv_matrix, m)?)?;
    m.add_function(wrap_pyfunction!(frame_plan, m)?)?;
    m.add_class::<Renderer>()
}
