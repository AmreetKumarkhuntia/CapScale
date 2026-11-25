# CapScale

Real-time upscaler for console gameplay coming in through a capture card.
A capture frame goes in at 1080p60 and comes out at 1440p or 4K, upscaled with
FSR 1, sharpened and colour graded on the GPU, with a few milliseconds of added
delay.

- **Rust** (`src/`) owns the window and the GPU pipeline (wgpu, WGSL shaders).
- **Python** (`python/capscale/`) owns capture, hotkeys and config.

Full documentation is in [docs/](docs/README.md): user guide, configuration
reference, architecture, render pipeline, frame generation, Python API and
development.

## Install

Download `CapScale-Setup-<version>.exe` from the
[Releases](https://github.com/AmreetKumarkhuntia/CapScale/releases) page and run it.
It installs for your user only (no administrator rights) and adds CapScale to the
Start menu, plus the Desktop if you tick it.

On the options page:

- **Create a desktop shortcut**
- **Download AI frame generation** (off by default; about 4.6 GB for an NVIDIA RTX card,
  about 150 MB for any other).
  A window shows the download; press Enter when it says it is done. You can add it
  later with **CapScale AI setup** in the Start menu.

The last page of the installer shows the getting-started guide below. To remove
CapScale, use Windows' Installed apps; that also removes the AI download. Your
settings stay in `%APPDATA%\CapScale`.

## Getting started

The first time CapScale opens, the settings window shows a **Getting started** tab
with these steps. It is always there as the first tab.

1. Connect the console's HDMI to the capture card, and the card to the PC by USB.
2. Close OBS, Discord streaming, or anything else using the card. A capture device
   has one owner at a time.
3. On the **Source** tab pick the card, 1920 x 1080 and 60, then press **Start capture**.
4. The picture opens in its own window. F11 toggles fullscreen, Escape stops.

Then tune it on the other tabs: **Picture** (upscaler, sharpening, colour, LUT),
**Frame rate** (in-between frames, e.g. a 30 fps game shown at 60), **Output**
(window size, vsync, stats overlay) and **Audio**. Changes apply at once; press
**Save to config.toml** to keep them. See [Keys](#keys) and [Settings](#settings)
for the details.

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
| F11 | Fullscreen on or off |
| Esc | Close the picture window (capture stops, settings stay) |

Keys are set in the `[hotkeys]` section of `config.toml`.

## Settings

Three ways to change the same values:

1. **Settings window** (opens with the app, F1): tabs for getting started, source, output size, frame rate,
   upscaler / sharpening / colour / LUT, and audio. It stays open beside the picture, so
   changes show live.
   "Save to config.toml" writes them to the file.
2. **`config.toml`**: saving the file applies it while the app runs.
3. **Python**: `Renderer.set_params(...)`, see below.

RCAS sharpness uses AMD's scale: **0.0 is the sharpest**, each +1.0 halves it, up to 2.0.

A card that sends MJPEG leaves compression speckle in the picture, and sharpening makes it
stand out. **Noise reduction** on the Picture tab removes it before the upscale.

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

**AI method (optional).** **Method** on the Frame rate tab replaces the built-in
method with an AI model, run on the graphics card: the open-source
[RIFE](https://github.com/hzwer/Practical-RIFE), or on an NVIDIA RTX card one of NVIDIA's
own three (Low, Medium, High). They are neural networks trained on real video, where the
built-in method is a block search; try them on your game. They are not part of the app; install
it once by ticking **Download AI frame generation** in the installer, or later from
the **CapScale AI setup** Start-menu entry. From source, run

```
scripts\install-ai.ps1
```

Either way it looks at the graphics card and puts the model and one of two runtimes in
`%LOCALAPPDATA%\CapScale\ai`. Delete that folder to remove it. Until it is installed
only the built-in method can be chosen.

| Graphics card | Runtime | Download | One generated frame |
|---|---|---|---|
| NVIDIA RTX | ONNX Runtime with NVIDIA's CUDA and TensorRT, and NVIDIA's video models (`nvidia-vfx`) | about 4.6 GB | RIFE 10 to 12 ms; NVIDIA Low 3 ms, Medium 5 ms, High 18 ms (too slow for 30 to 60 on this card). At 1080p on an RTX 5070 |
| any other DirectX 12 card (AMD, Intel, older NVIDIA) | ONNX Runtime with DirectML | about 150 MB | about 33 ms at 1080p, 13 ms at 720p. Measured on the same RTX 5070; no AMD or Intel card has been timed |

- With TensorRT on an RTX 5070 a real frame is shown about 28 ms late at 30 to 60,
  against 17 ms for the built-in method.
- A generated frame has to be ready within its slot (16.7 ms for 30 to 60). With
  TensorRT, 30 to 60 and 30 to 90 fit at 1080p. With DirectML at the speed above,
  30 to 60 fits a 720p capture and not a 1080p one. Where it does not fit, the
  built-in method is used and the stats overlay says so.
- With TensorRT, the first use of a capture size prepares the model for the graphics card, which
  takes a minute or two (`install-ai.ps1` from source does this for 1080p). The built-in method
  runs meanwhile.
- Where the picture is at rest (a menu, a still scene, a large HUD panel) the
  generated frame is the real picture, untouched. Small still things on top of
  motion, like a crosshair or a minimap, are drawn by the model and can look a little
  soft: put a HUD zone over them. The **AI: leave still areas untouched** switch
  turns this off, and the model then draws the whole frame.
- It handles the 4:2:0 video a capture card normally gives (MJPEG, NV12).
  `uv run python -m capscale.ai` times it and checks its output.

In `config.toml` these are `ai_framegen`, `ai_model` and `ai_still` under `[params]`.
Engines and models are both tables that can be added to: see
[Development](docs/development.md#adding-an-engine-or-a-model).

**AI upscaling (optional, NVIDIA RTX).** **AI upscale first** on the Picture tab hands
each frame to NVIDIA's video upscaler before the steps above: it enlarges the frame to
the size shown and removes compression noise as it does, which suits a card that sends
MJPEG. Seven levels, from Low (4 ms a frame for 1080p to 1440p on an RTX 5070) to
Streaming Ultra (11 ms); it comes with the same download. Each of the game's real
frames goes through it, and frame generation then works on the enlarged frames. The
level chosen is the one used: where it is too slow for the rate, frames are left out
and the stats overlay says so. In `config.toml` it is `ai_upscale` under `[params]`.

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
| Frame generation | `luma.wgsl`, `flow.wgsl`, `interp.wgsl` | optional; motion search on a brightness pyramid, then a warp. The AI method runs before the pipeline instead (`ai/`) |
| Noise reduction | `denoise.wgsl` | optional; a bilateral filter at capture size, for cards that send MJPEG |
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
r.toggle_pass("capture", True)    # show the picture window; it is hidden until capture is on
r.push_frame(array, fmt="nv12", width=1920, height=1080)               # newest frame wins
r.set_params(sharpness=0.4, exposure=0.0, saturation=1.1,
             lut_path="film.cube", lut_strength=0.6, upscaler="easu")
r.toggle_pass("rcas", False)      # denoise, upscale, rcas, grade, lut, split, overlay, panel, capture, fullscreen
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
src/                  Rust: one folder per concern, each with its types in types.rs
  lib.rs              module list and the Python module
  render/             window and render thread, pass chain, shader hot reload,
                      GPU timing, state shared between threads, the Renderer class
  params/             settings, uniform block, colour maths, frame plan
  source/             the captured frame as GPU textures, frame upload
  lut/                .cube parser and 3D texture
  ui/                 stats overlay, HUD zones, the settings window and native dialogs
python/capscale/     Python package
  app.py              command line and main loop
  capture/            where frames come from
    devices.py          list devices and modes
    stream.py           open the card, yield frames
    pattern.py          colour bar generator
    repeats.py          spot repeated frames (a 30 fps game in a 60 Hz signal)
    types.py            Mode, Frame
  audio.py            the card's sound to the speakers
  ai/                 optional AI frame generation and AI upscaling
    engines.py          how a model runs on the card: TensorRT, DirectML
    pack.py             the AI download and the models in it
    model.py            one model for one frame size
    framegen.py         the AI thread
    check.py            time the real model and score its output
    types.py            Planes, FrameModel, Engine
  config.py           config.toml load, save, live reload
  shaders/            one WGSL file per pass
tests/rust/           cargo test
tests/python/         pytest
docs/                 full documentation
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
