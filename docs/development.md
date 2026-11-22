# Development

## Build from source

Needs Rust, Python 3.11 or newer, and [uv](https://docs.astral.sh/uv/).

```
uv venv
uv pip install maturin numpy av sounddevice pytest
uv run maturin develop --release
```

`maturin develop` compiles the Rust crate and installs it into the environment as
`capscale._core`, next to the Python sources in `python/capscale/` (see
`[tool.maturin]` in `pyproject.toml`). Run it again after changing Rust code.
Python and shader changes need no rebuild.

## Run

```
uv run capscale                                    # settings window; pick the card, Start capture
uv run capscale --list-devices                     # devices and their modes
uv run capscale --windowed --device "Display capture-UVC03" --size 1920x1080 --fps 60
uv run capscale --test-pattern i420 --windowed     # colour bars, no card needed
uv run capscale --test-pattern i420:30 --windowed  # a 30 fps picture in a 60 Hz signal
```

From source, `config.toml` and `last_source.json` are read from the current
folder. All flags are in [Configuration](configuration.md#command-line).

`scripts\install-shortcut.ps1` puts a **CapScale** shortcut on the Desktop and in
the Start menu that runs this checkout with no console window. Run it again after
moving the folder; delete the two `.lnk` files to remove it.

## Layout

```
src/                  Rust: one folder per concern
  lib.rs                module list and the _core Python module
  render/               window and render thread, pass chain, shader hot reload,
                        GPU timing, shared state, the Renderer class
  params/               settings, uniform block, colour maths, frame plan
  source/               the captured frame as GPU textures
  lut/                  .cube parser and 3D texture
  ui/                   stats overlay, HUD zones, settings window, native dialogs
python/capscale/      Python package
  app.py                command line and main loop
  capture/              devices, stream, test pattern, repeat detection
  audio.py              the card's sound to the speakers
  ai/                   optional AI frame generation: engines, models, the AI thread
  config.py             config.toml load, save, live reload
  shaders/              one WGSL file per pass
tests/rust/           cargo test
tests/python/         pytest
scripts/              installer build, AI setup, shortcut
packaging/            PyInstaller entry, Inno Setup script, icon
docs/                 this documentation
```

Conventions in the Rust crate:

- one folder per concern, with `mod.rs` holding only the module list and re-exports;
- a folder's types live in `types.rs`;
- what a folder exposes to Python lives in its `python.rs` and is registered in
  `lib.rs`.

A file-by-file map is in [Architecture](architecture.md#source-map).

## Tests

```
cargo test
uv run pytest
```

Neither opens a window or needs a capture card.

### Rust (`tests/rust/`, one test binary)

| File | Covers |
|---|---|
| `params.rs` | YUV round trips for every matrix and range, limited-range code values, neutral settings are identity, `fit`, saved config and reset, toggles, the frame generation plan, HUD zones reaching the uniform block |
| `lut.rs` | the `.cube` parser: ordering, domain, unknown keywords, bad files |
| `source.rs` | packed buffers split into planes |
| `shaders.rs` | `common.wgsl` is put in front; saves are noticed |
| `pipeline.rs` | every shipped shader builds against the pass layout (on a stub GPU device); a broken shader is reported, not a panic |
| `gpu.rs` | the pipeline on the real GPU with no window: frames pass through unchanged, generated frames follow known motion, overlays stay steady in a noisy scene, a HUD zone shows the real frame. Skipped where there is no GPU |

### Python (`tests/python/`)

| File | Covers |
|---|---|
| `test_app.py` | the shortcut entry shows a start-up failure, the installed program's config location, command line over config |
| `test_config.py` | save replaces only the given sections, load merges defaults, unknown sections, the watcher |
| `test_devices.py` | DirectShow and V4L2 mode parsing, `best_mode` |
| `test_repeats.py` | repeat detection: 30 and 60 fps games, noise, still pictures, rhythm slips, forced rates, every frame layout |
| `test_audio.py` | matching the card's sound input by name, the delay line |
| `test_ai.py` | the AI thread's ordering, spacing and fallbacks with a stand-in model; HUD zones; which engine and which model are picked; the real model where it is installed |

## Working on shaders

Shaders reload while the app runs: edit a file in `python/capscale/shaders/`,
save, and look. A compile error shows in the stats overlay and the previous
version keeps running. Details, the uniform block and the list of places a new
pass touches are in [Render pipeline](pipeline.md#editing-a-shader).

## AI frame generation from source

```
scripts\install-ai.ps1 [-Runtime nvidia|directml] [-Model <name>]
```

Installs into `%LOCALAPPDATA%\CapScale\ai`, the same place the installed program
uses. Steps:

1. Fetches uv if it is missing, and a Python 3.12 for the build step.
2. Picks the runtime from the graphics card's name, `nvidia` for an NVIDIA RTX card
   and `directml` for anything else, unless `-Runtime` says. Installs its pinned
   packages into `ai\site`: `onnxruntime-gpu[cuda,cudnn]` and `tensorrt-cu13-libs`
   (the large download), or `onnxruntime-directml`. Going from one to the other
   replaces `ai\site`.
3. Downloads the model and wraps it with `scripts\build_ai_model.py`, so it
   takes and returns 8-bit YUV planes, as `ai\<model>.onnx`.
4. If the repo has a `.venv`, runs `python -m capscale.ai 1920x1080 <model>` in it,
   which loads the model on this graphics card (TensorRT prepares it for 1080p
   first) and checks its output.

The two NVIDIA versions are pinned because they were tested together; change
them as a pair. After rebuilding a model by hand, delete `ai\engines`, or what
TensorRT prepared from the old model is used again.

The script and the app both find the folder through `LOCALAPPDATA`. Setting that
variable to another folder for one shell tries the other runtime without touching
the installed one.

### Adding an engine or a model

- **An engine**, another way to run models on a card: its packages as a row of
  `$runtimes` in `install-ai.ps1`, and an `Engine` in `ENGINES` in
  `python/capscale/ai/engines.py` with ONNX Runtime's name for it and its options.
  The first entry the installed runtime offers is the one used.
- **A model**: a row of `$models` in `install-ai.ps1` (the archive, and the file in
  it), then `scripts\install-ai.ps1 -Model <name>`; it then appears in the Method row
  of the Frame rate tab. `build_ai_model.py` wraps a model that has one input (two
  RGB pictures and the moment, seven channels) and one output, at any frame size:
  the `rife_v2` files of the vs-mlrt releases. A model of another shape needs its
  own wrapper giving the same inputs and outputs, or its own class in place of
  `ai.Model`: `FrameGen` takes anything that follows `FrameModel` in `ai/types.py`.
  `ai/nvidia.py` is such a class, for NVIDIA's models; `ai/choice.py` is where a
  name is turned into the class that runs it, and where the settings window's lists
  come from.

## Building the installer

```
scripts\build-installer.ps1
```

Needs [Inno Setup 6](https://jrsoftware.org/isinfo.php) as well. It works in its
own environment (`.venv-build`), so it does not disturb the development one and
can run while the app is open.

| Step | Does |
|---|---|
| wheel | `maturin build --release` into `dist\wheels` |
| build environment | creates `.venv-build` with Python 3.12, once |
| install | the wheel and PyInstaller into it |
| freeze | PyInstaller builds `dist\CapScale\` from `packaging\main.py`, windowed, with the shaders added as data |
| self-test | runs `CapScale.exe --self-test`: a module or data file left out only fails at run time. Where the AI download is installed it loads every frame model and an upscaler too. The error, if any, is in `%APPDATA%\CapScale\self-test.log` |
| installer | Inno Setup compiles `packaging\installer.iss` |

Results:

- `dist\CapScale\CapScale.exe`: the app as a standalone program, no Python needed.
- `dist\CapScale-Setup-<version>.exe`: the per-user installer.

What the installer does (`packaging\installer.iss`): installs for the current
user, adds the Start menu entry and optional desktop shortcut, copies
`install-ai.ps1` and `build_ai_model.py` to `ai-setup\` for the **CapScale AI
setup** entry, shows `packaging\getting-started.txt` on its last page, and on
uninstall removes the program folder and the AI download.

ONNX Runtime, CuPy and NVIDIA's models are not part of the frozen app (the AI download
brings them), so the standard library modules they need and the app does not are named
as hidden imports in the freeze step (`$hidden` in the script). The self-test loads
every installed model, so a missing one shows up there.

Having its own `.exe` means graphics driver settings can target the program: in
the NVIDIA app, add `CapScale.exe` under Graphics > Program settings.

## Version

The version is in two places that must match: `version` in `pyproject.toml` and
in `Cargo.toml`. The installer build reads it from `pyproject.toml`.

## The icon

`packaging\icon.ico` is committed. `packaging\make_icon.py` draws it (it needs
only numpy) and is run only to change the design.
