# Python API

The package is `capscale`. Its GPU side is the Rust extension `capscale._core`;
the rest is plain Python that can be used on its own.

```python
import capscale
from capscale.capture.pattern import Pattern

r = capscale.Renderer(0, 0, fullscreen=False)
r.toggle_pass("capture", True)                 # show the picture window
for data, fmt, w, h, full, matrix, ms in Pattern("i420").frames():
    r.push_frame(data, fmt=fmt, width=w, height=h,
                 full_range=full, matrix=matrix, decode_ms=ms)
    if any(kind == "closed" for kind, _ in r.poll_events()):
        break
r.close()
```

## `capscale.Renderer`

The two windows and the GPU pipeline. One per process.

### `Renderer(out_w=0, out_h=0, fullscreen=True, vsync="mailbox", shader_dir=None, backend=None)`

| Argument | Meaning |
|---|---|
| `out_w`, `out_h` | render size. `0, 0` fits the window |
| `fullscreen` | borderless fullscreen for the picture window |
| `vsync` | `"mailbox"`, `"fifo"` or `"immediate"` |
| `shader_dir` | folder of `.wgsl` files. Default: `shaders` inside the installed package |
| `backend` | `"auto"`, `"dx12"` or `"vulkan"`. Default: DirectX 12 on Windows, else auto |

Starts the render thread and returns once its windows and pipeline are up. The
settings window is visible; the picture window stays hidden until
`toggle_pass("capture", True)`.

Raises `ValueError` for an unknown `vsync` or `backend`, and `RuntimeError` if a
`Renderer` already exists or start-up fails (no usable GPU, a shader that does not
compile).

### `push_frame(array, fmt, width, height, full_range=None, matrix=None, decode_ms=0.0)`

Show a frame. The newest frame wins: nothing queues.

| Argument | Meaning |
|---|---|
| `array` | one packed `uint8` array, or a sequence of 2-D `uint8` arrays, one per plane |
| `fmt` | `"nv12"`, `"yuy2"` (also `"yuyv422"`), `"i420"`, `"i422"` or `"i444"` |
| `width`, `height` | picture size in pixels |
| `full_range` | `True` full, `False` limited. `None`: limited |
| `matrix` | `"bt601"` or `"bt709"`. `None`: BT.709 from 720 lines up, BT.601 below |
| `decode_ms` | time spent getting this frame, for the latency figure |

Planes per format:

| `fmt` | Planes |
|---|---|
| `nv12` | Y; interleaved CbCr at half width and half height |
| `yuy2` | one: `Y0 Cb Y1 Cr` per pixel pair |
| `i420` | Y; Cb and Cr at half width and half height |
| `i422` | Y; Cb and Cr at half width |
| `i444` | Y; Cb; Cr |

A packed array holds the planes back to back with no padding, which is how a
capture card delivers `nv12` and `yuy2`. A sequence holds one 2-D array per plane,
whose rows may be longer than the plane (padding), which is how a decoder delivers
planar frames. Arrays must be C-contiguous. `nv12` and `yuy2` need an even width,
`nv12` an even height too.

The pixels are copied to the GPU before the call returns, and the Python lock is
released while that happens, so the array can be reused straight after.

`full_range` and `matrix` describe the frame; the `full_range` and `matrix`
*settings* override them when set.

Raises `ValueError` for a bad format, size or buffer, `TypeError` for something
that is not an array, and `RuntimeError` once the window is closed.

### `set_params(**settings)`

Change any tunable. Values are clamped to their ranges. If one keyword is unknown
(`TypeError`) or has a bad value (`ValueError`), none of the call is applied.

Every key of `[params]` in [Configuration](configuration.md#params) is accepted,
plus:

| Keyword | Type | Meaning |
|---|---|---|
| `out_size` | `(w, h)` | render size; `(0, 0)` fits the window |
| `fullscreen` | bool | |
| `vsync` | str | `"mailbox"`, `"fifo"`, `"immediate"` |
| `backend` | str | stored for `config_toml()`; the running window keeps its backend |
| `upscaler` | str | also `"next"`, to step through them |
| `full_range` | bool or `None` | `None`: trust each frame |
| `matrix` | str or `None` | `None` or `"auto"`: trust each frame |
| `lift`, `lgg_gamma`, `gain` | float or `[r, g, b]` | |
| `lut_path` | str or `None` | the file is read and parsed in this call; `""` or `None` unloads |
| `volume`, `audio_delay_ms` | float | held for the settings window; playback reads them through `audio` |
| `hint` | str | a line under the stats |
| `note` | str | a line above the hint; the app uses it for the AI status |
| `detected_fps` | float | the game rate found by repeat detection |
| `ai_available` | bool | enables the AI switch in the settings window |
| `ai_active` | bool | the caller is pushing ready-made generated frames; the built-in generation stands down |

### `toggle_pass(name, on=None) -> bool`

Set a switch, or flip it when `on` is left out. Returns the new state.

| Name | Switches |
|---|---|
| `denoise`, `upscale`, `rcas`, `grade`, `lut` | the passes |
| `split` | the before / after view |
| `overlay` | the stats overlay |
| `panel` | the settings window |
| `capture` | the picture window |
| `fullscreen` | |
| `audio` | the sound switch that `audio` reports |
| `zones` | showing and dragging the HUD zones |

Raises `ValueError` for an unknown name.

### `poll_events() -> list[tuple[str, str]]`

Window events since the last call, as `(kind, value)`:

| `kind` | `value` |
|---|---|
| `"key"` | the key's name: `"F1"`, `"Escape"`, `"a"` |
| `"start"` | `"<device>\t<width>\t<height>\t<fps>"` from the Source tab |
| `"stop"` | `""`: Stop was pressed or the picture window closed |
| `"refresh_sources"` | `""` |
| `"save"` | the config text to write (the same as `config_toml()`) |
| `"closed"` | `""`: the settings window was closed |

### `set_sources(devices, device="", width=1920, height=1080, fps=60)`

Fill the Source tab. `devices` is a list of
`(name, label, [(width, height, max_fps), ...])`. The other arguments say which
one to show as picked.

### `config_toml() -> str`

The current settings as the `[output]`, `[params]`, `[passes]` and `[audio]`
sections of `config.toml`.

### `close()`

Close both windows and stop the render thread. Also happens when the object is
dropped.

### Properties

| Property | Value |
|---|---|
| `stats` | dict, see below |
| `framegen` | `(on, content_fps, target_fps, ai_chosen)` |
| `audio` | `(on, volume, delay_ms)` |
| `hud_zones` | list of `(left, top, right, bottom, round)` |
| `ai_still` | bool |

`stats` keys:

| Key | Meaning |
|---|---|
| `capture_fps`, `render_fps` | frames pushed and pictures drawn per second, smoothed |
| `decode_ms` | as given to `push_frame` |
| `present_ms` | from the `push_frame` call to the present call |
| `latency_ms` | `decode_ms + present_ms` |
| `pass_ms` | dict of GPU milliseconds per pass: `yuv`, `framegen`, `denoise`, `upscale`, `rcas`, `grade`, `lut`, `composite` |
| `gpu_timing` | whether `pass_ms` is measured on this graphics card |
| `frames`, `rendered` | frames pushed, pictures drawn |
| `framegen_multiple` | pictures per game frame; 1 when the built-in generation is off |
| `low_fps` | rate of the slowest 1% of pictures; 0 until measured |
| `source` | e.g. `"1920x1080 i420 limited bt709"` |
| `output` | `(w, h)` render size |
| `upscaler`, `present_mode`, `adapter` | what is in use |
| `error` | shader compile errors from hot reload; empty when all is well |

## `capscale._core` functions

| Function | Returns |
|---|---|
| `yuv_matrix(matrix, full_range)` | the renderer's YUV to RGB transform as three rows of four: `rgb[i] = row[i][0]*y + row[i][1]*cb + row[i][2]*cr + row[i][3]`, all 0..1 |
| `frame_plan(content_fps, measured_fps, target_fps)` | `(multiple, period_seconds)`: the plan the renderer follows for frame generation |
| `show_error(message)` | nothing; shows a native error box |

## `capscale.capture`

### `capture.devices`

| Name | Does |
|---|---|
| `list_devices()` | `[(name, label), ...]`: `name` opens the device, `label` is for people |
| `list_modes(device)` | `[Mode(fmt, width, height, max_fps), ...]` in formats the renderer takes (`nv12`, `yuyv422`, `mjpeg`), largest first |
| `best_mode(modes, width, height, fps, fmt="auto")` | the mode to ask for: raw if the card offers it at this size and rate, else MJPEG; `None` if nothing fits |
| `parse_dshow_modes(text)`, `parse_v4l2_modes(text)` | the parsers behind `list_modes`, which reads FFmpeg's log output |

DirectShow is used on Windows, V4L2 on Linux.

### `capture.stream.Capture(device, width, height, fps, fmt="mjpeg")`

Opens the device through PyAV with buffering switched off.

- `frames()` yields `(data, fmt, width, height, full_range, matrix, decode_ms)`,
  ready for `push_frame`. `data` views the decoder's memory: use it before asking
  for the next frame.
- `dropped` counts frames skipped because they were already queued.
- `close()`.

### `capture.pattern.Pattern(fmt, width=1920, height=1080, fps=60, content_fps=None)`

Colour bars over a grey ramp with a moving stripe, with the same `frames()` and
`close()` as `Capture`. `fmt` is `"nv12"`, `"yuy2"` or `"i420"`. With
`content_fps` below `fps`, each picture is held for several frames, like a 30 fps
game in a 60 Hz signal.

### `capture.repeats`

- `luma_of(data, fmt, width, height)`: the brightness plane of a frame in any
  layout `frames()` yields.
- `Cadence().step(luma, forced=None) -> bool`: take the next captured frame;
  `True` if it should be passed on. `forced` is `True` when the game is known to
  run at half the capture rate, `False` when known not to. `Cadence.halved` is the
  current verdict. See [Frame generation](frame-generation.md#step-1-find-the-repeats).

## `capscale.audio`

### `Audio(video_device, device="")`

A duplex stream at 48 kHz stereo: the card's sound input to the default output.
The input is the first one whose name contains `device`, or `video_device` when
that is empty (a card's sound input is named after the card). Raises
`RuntimeError` if none matches.

| Attribute | Meaning |
|---|---|
| `volume`, `delay_ms`, `muted` | set these at any time; delay is 0 to 500 ms |
| `latency_ms` | time from input to speakers, including the delay |
| `glitches` | count of under- or overruns reported by the sound system |
| `peak` | recent input level, for a meter |
| `close()` | stop |

`DelayLine(channels, max_samples).process(block, delay)` is the ring buffer
behind the delay; `find_devices` and `match_input` do the matching.

## `capscale.ai`

Optional; see [Frame generation](frame-generation.md#the-ai-method).

| Name | Does |
|---|---|
| `PACK` | where the AI download lives |
| `available()` | whether it is installed |
| `Model(width, height, name="")` | loads an installed model for one frame size (`""`: the first one); `took` is seconds per generated frame |
| `Model.generate(older, newer, when, matrix="bt709", full_range=False, keep_still=True)` | the picture `when` (0..1) of the way between two `(Y, U, V)` plane tuples |
| `planes_of(data, fmt, width, height)` | a captured frame copied into `(Y, U, V)`, or `None` for a layout the model does not take |
| `keep_zones(made, real, zones)` | copies the real pixels into the HUD zones, in place |
| `FrameGen(renderer, model=Model)` | the AI thread: `submit(...)` a game frame, read `status`, `close()`. `model` is called with `(width, height)` to make the model |
| `pack.models()` | the installed models: name to file |
| `engines.ENGINES`, `engines.pick(providers)` | the ways to run a model, in order of preference, and the one to use given what ONNX Runtime offers |

`python -m capscale.ai [WIDTHxHEIGHT] [MODEL]` loads the model, times it and checks its
output.

## `capscale.config`

| Name | Does |
|---|---|
| `DEFAULTS` | every section with its default values |
| `load(path)` | defaults overlaid with the file. Raises on bad TOML or an unknown section |
| `save_sections(path, text)` | replaces the sections `text` contains; every other section of the file is kept |
| `Watcher(path).changed()` | `True` once after each save of the file |
| `app_dir(base)` | the app's folder under `base` |

## `capscale.app`

| Name | Does |
|---|---|
| `main(argv=None) -> int` | the command line program (`capscale`) |
| `gui(argv=None) -> int` | the same for a shortcut with no console: a start-up failure is shown in a message box |
| `default_config_path()` | `config.toml` in the current folder from source; in the user's profile when installed |
