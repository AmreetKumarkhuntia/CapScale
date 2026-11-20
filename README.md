# CapScale

Real-time upscaler for console gameplay coming in through a capture card.
A capture frame goes in at 1080p60 and comes out at 1440p or 4K, upscaled with
FSR 1, sharpened and colour graded on the GPU, with a few milliseconds of added
delay.

- **Rust** (`src/`) owns the window and the GPU pipeline (wgpu, WGSL shaders).
- **Python** (`python/capscale/`) owns capture, hotkeys and config.

## Build

Needs Rust, Python 3.11+ and [uv](https://docs.astral.sh/uv/).

```
uv venv
uv pip install maturin numpy av sounddevice pytest
uv run maturin develop --release
```

## Run

```
uv run capscale                    # settings window; pick the card on the Source tab, then Start capture
uv run capscale --list-devices     # devices and the modes they offer
uv run capscale --windowed --device "Display capture-UVC03" --size 1920x1080 --fps 60
uv run capscale --test-pattern i420 --windowed    # colour bars, no card needed (also nv12, yuy2)
uv run capscale --test-pattern i420:30 --windowed # the same as a 30 fps picture in a 60 Hz signal
```

To start it like any other program, run `scripts\install-shortcut.ps1` once. It puts
a **CapScale** shortcut on the Desktop and in the Start menu: no console window,
opens at the settings window, and shows errors on screen.

Close OBS (or anything else using the card) first: a capture device has one owner at a time.

Game audio plays automatically: the card's sound input (the one named after the
capture device) goes to the default speakers. On Windows it arrives about 50 ms
after the picture, which is the floor for shared audio and below what people
notice. The settings window's Audio tab has mute, volume and an extra delay for the case where
sound arrives before the picture. If the wrong input is picked, set
`audio_device` under `[capture]` in `config.toml`.

## Keys

| Key | Does |
|---|---|
| F1 | Settings window on or off |
| F2 | Stats overlay |
| F3 | Before / after split (drag the divider) |
| F4 | Next upscaler: AMD FSR 1 (EASU), Lanczos-3, bilinear |
| F5 / F6 / F7 | RCAS / colour grading / LUT on or off |
| F8 | Mute or unmute game audio |
| F9 | Show the settings window |
| F11 | Fullscreen on or off |
| Esc | Close the picture window (capture stops, settings stay) |

Keys are set in the `[hotkeys]` section of `config.toml`.

## Settings

Three ways to change the same values:

1. **Settings window** (opens with the app, F1): tabs for source, output size, frame rate,
   upscaler / sharpening / colour / LUT, and audio. It stays open beside the picture, so
   changes show live.
   "Save to config.toml" writes them to the file.
2. **`config.toml`**: saving the file applies it while the app runs.
3. **Python**: `Renderer.set_params(...)`, see below.

RCAS sharpness uses AMD's scale: **0.0 is the sharpest**, each +1.0 halves it, up to 2.0.

The stats overlay shows capture and render rate, added latency (decode time plus
the time from handing a frame over to presenting it; display scan-out is not
included), and GPU time per pass.

### Frame rate: current and target

A console sends 60 pictures a second even when the game runs at 30: every frame
arrives twice. The **Frame rate** tab of the settings window deals with that.

- **Current (game)**: the game's own rate. *auto* measures it by spotting the
  repeated frames; set 30 or 60 yourself if you know it.
- **Target**: the rate to show. Off shows the frames as they come. With a target
  above the game's rate, the frames in between are generated: 30 to 60 is one
  generated frame between each pair, 30 to 120 is three.

Only whole multiples are generated (a target of 100 on a 30 fps game gives 90).

What it costs: a real frame has to wait while the generated ones before it are
shown, so 30 to 60 adds about 17 ms of delay and 30 to 120 about 25 ms. The stats
overlay shows the real figure. GPU cost is under a millisecond.

What to expect from the picture: camera pans and a HUD that stays still are handled
well. A small object moving against the background, and a thin strip at the screen
edges during a pan, can smear or flicker in the generated frames. This built-in
method is motion interpolation done in shaders here (`luma.wgsl`, `flow.wgsl`,
`interp.wgsl`).

In `config.toml` these are `content_fps` (0 = auto) and `target_fps` (0 = off)
under `[params]`.

**AI method (optional).** The **AI frames (RIFE)** switch replaces the built-in
method with the open-source [RIFE](https://github.com/hzwer/Practical-RIFE) model, run on an
NVIDIA RTX card. It is a neural network trained on real video, where the built-in
method is a block search; try both on your game. It is not part of the app; install
it once with

```
scripts\install-ai.ps1
```

which puts about 4 GB (NVIDIA's CUDA and TensorRT libraries, ONNX Runtime and the
model) in `%LOCALAPPDATA%\CapScale\ai`. Delete that folder to remove it. Until it
is installed the switch is greyed out.

- On an RTX 5070 one generated 1080p frame takes 10 to 12 ms, so a real frame is
  shown about 28 ms late at 30 to 60, against 17 ms for the built-in method.
- A generated frame has to be ready within its slot. 30 to 60 and 30 to 90 fit; where
  it does not fit (30 to 120, or a 60 fps game to 120), the built-in method is used
  and the stats overlay says so.
- The first use of a capture size prepares the model for the graphics card, which
  takes a minute or two (the installer does this for 1080p). The built-in method
  runs meanwhile.
- Where the picture is at rest (a menu, a still scene, a large HUD panel) the
  generated frame is the real picture, untouched. Small still things on top of
  motion, like a crosshair or a minimap, are drawn by the model and can look a little
  soft: put a HUD zone over them. The **AI: leave still areas untouched** switch
  turns this off, and the model then draws the whole frame.
- It handles the 4:2:0 video a capture card normally gives (MJPEG, NV12).
  `uv run python -m capscale.ai` times it and checks its output.

In `config.toml` these are `ai_framegen` and `ai_still` under `[params]`.

**HUD zones.** A minimap that turns over a moving road cannot be interpolated
cleanly. On the Frame rate tab, "+ box" or "+ circle" adds a zone; tick "Show
and move zones" and drag the red outline over the minimap or HUD panel. Inside a
zone generated frames show the real picture, so nothing there can smear or flicker.
Up to eight zones; they are saved as `hud_zones`.

**Steadiness.** The stats overlay shows "1% low": the rate of the slowest 1% of
pictures over the last ten seconds. Close to the render rate means even pacing;
well below it means stutter.

### Console and hardware checklist

Most picture and sound problems come from the chain before the app:

- **PS5: turn HDCP off** (Settings > System > HDMI). With it on, a capture card
  gets a black picture.
- **PS5: for 30 fps games keep 120 Hz output and VRR off.** In a 120 Hz signal each
  frame arrives four times, not two, and the repeat handling here expects two.
- **PS5 audio: Linear PCM, stereo.** With surround output, dialogue sits in the
  centre channel, which a stereo capture drops.
- **RGB range:** leave the PS5 on Automatic or Limited; if blacks look crushed or
  grey, switch Range on the settings window's Source tab.
- **Capture card format matters.** A card that delivers uncompressed NV12 or YUY2
  has less delay and a cleaner picture than one that only offers MJPEG, and
  compression blocks make frame generation less accurate. `--list-devices` shows
  what a card offers.
- **USB:** a rear motherboard port and the cable that came with the card. A card
  can fall back to slow, compressed modes on a weak port or cable.
- **Monitor refresh:** a whole multiple of the shown rate paces evenly (60 on a
  120, 180 or 240 Hz display). Use the mailbox present mode; immediate tears.
- **Sound to the PC:** wired or 2.4 GHz headphones. Bluetooth adds well over 100 ms.

### Colour looks wrong?

Capture cards do not always label their video correctly. If the picture is washed
out or too dark, switch **Range** on the settings window's Source tab; if colours are slightly
tinted, switch **Matrix**. Then save.

## The pipeline

Each pass is one file in `python/capscale/shaders/`. Saving a shader recompiles
it while the app runs; if it does not compile, the previous version keeps running
and the error shows in the stats overlay.

| Pass | Shader | Notes |
|---|---|---|
| YUV to RGB | `yuv2rgb.wgsl` | NV12, YUY2 or planar; BT.601 / BT.709, limited or full range |
| Frame generation | `luma.wgsl`, `flow.wgsl`, `interp.wgsl` | optional; motion search on a brightness pyramid, then a warp. The AI method runs before the pipeline instead (`ai.py`) |
| Upscale | `easu.wgsl`, `lanczos3.wgsl`, `bilinear.wgsl` | switchable while running |
| Sharpen | `rcas.wgsl` | FSR 1 RCAS |
| Grade | `grade.wgsl` | in linear light: exposure, white balance, contrast, saturation, vibrance, lift / gamma / gain |
| LUT | `lut3d.wgsl` | `.cube` file, trilinear, with strength |
| Composite | `composite.wgsl` | fits the image to the window, draws the split |

`common.wgsl` holds the shared uniform block and is placed in front of every pass.

## Python API

```python
import capscale

r = capscale.Renderer(2560, 1440, fullscreen=True, vsync="mailbox")   # 0, 0 fits the window
r.push_frame(array, fmt="nv12", width=1920, height=1080)               # newest frame wins
r.set_params(sharpness=0.4, exposure=0.0, saturation=1.1,
             lut_path="film.cube", lut_strength=0.6, upscaler="easu")
r.toggle_pass("rcas", False)      # upscale, rcas, grade, lut, split, overlay, panel, capture, fullscreen
r.stats                           # dict: fps, latency_ms, pass_ms, ...
r.poll_events()                   # [("key", "F1"), ("closed", ""), ...]
r.close()
```

`push_frame` takes one packed `uint8` array (`nv12`, `yuy2`) or a sequence of 2-D
arrays, one per plane (`i420`, `i422`, `i444`). Rows may be padded. The pixels are
copied to the GPU before the call returns, and the Python lock is released while
that happens. One `Renderer` per process.

## Layout

```
src/                  Rust: one module per job
  lib.rs              Python bindings (Renderer)
  window.rs           render thread: window, surface, frame loop
  pipeline.rs         the pass chain
  source.rs           frame upload
  params.rs           settings, uniform block, colour maths
  shaders.rs          shader loading and hot reload
  lut.rs              .cube parser
  timing.rs           GPU timestamp queries
  overlay.rs          stats overlay, HUD zones and the settings window
  shared.rs           state shared between threads
python/capscale/     Python package
  app.py              command line and main loop
  devices.py          list devices and modes
  capture.py          open the card, yield frames
  audio.py            the card's sound to the speakers
  repeats.py          spot repeated frames (a 30 fps game in a 60 Hz signal)
  ai.py               optional AI frame generation (RIFE)
  picker.py           LUT file dialog, error messages
  config.py           config.toml load, save, live reload
  pattern.py          colour bar generator
  shaders/            one WGSL file per pass
tests/rust/           cargo test
tests/python/         pytest
```

## Build the installer

```
scripts\build-installer.ps1
```

Needs [Inno Setup 6](https://jrsoftware.org/isinfo.php). It produces:

- `dist\CapScale\CapScale.exe` — the app as a standalone program, no Python needed.
- `dist\CapScale-Setup-<version>.exe` — an installer for the current user (no
  administrator prompt), with Start menu entry, optional desktop shortcut and uninstaller.

The installed app keeps `config.toml` and the remembered capture source in
`%APPDATA%\CapScale`. Having its own `.exe` also means graphics driver settings
can target it: in the NVIDIA app, add `CapScale.exe` under Graphics > Program
settings (for example to switch on Smooth Motion).

Set the environment variable `WGPU_BACKEND` to `dx12` or `vulkan` to choose the
graphics API; the settings window's Output tab shows which one is in use.

## Tests

```
cargo test            # colour maths, .cube parser, every shader compiled, and frame generation
                      # scored on the real GPU against a known moving scene (no window opens)
uv run pytest         # device mode parsing, config load and save, audio delay line, AI frame
                      # generation (the real model too, where it is installed)
```

## Credits

The EASU and RCAS shaders are ports of AMD FidelityFX Super Resolution 1.0 (MIT).
See [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
