# Configuration

Everything that can be set: `config.toml`, hotkeys, the command line, and the
files CapScale reads and writes.

## Where the settings come from

Later entries win:

1. Built-in defaults (`DEFAULTS` in `python/capscale/config.py`, `Params::default()`
   in `src/params/types.rs`).
2. `config.toml`.
3. Command line flags, for the capture source and the output.
4. Whatever is changed while running: the settings window, hotkeys, an edit of
   `config.toml`.

One extra rule for the capture source: if neither `config.toml` nor the command
line names a device, the one started last time is used (`last_source.json`).

Capture starts on its own only with `--device` or `--test-pattern`. Otherwise the
settings window opens and waits for **Start capture**.

### Live reload

Saving `config.toml` applies it while the app runs. The file's modification time is
checked every 20 ms.

- `[output]`, `[params]`, `[passes]`, `[audio]` and `[hotkeys]` take effect at once.
- A change to `[capture]` restarts capture, if it is running and `device` is set.
- `backend` is only read when the window is created, and `[ai]` when the app starts:
  they apply at the next start.
- A file that does not parse, an unknown section, or a wrong key or value is
  reported on stderr as `config: ...` and that reload is skipped; the app keeps
  running on the previous values.

### Save

**Save to config.toml** in the settings window (or a hotkey bound to `save`)
rewrites the `[output]`, `[params]`, `[passes]` and `[audio]` sections with the
current values. `[capture]`, `[hotkeys]` and anything else in the file are kept as
they are. Comments inside the four rewritten sections are replaced by the standard
ones.

## `config.toml`

### `[capture]`

| Key | Default | Meaning |
|---|---|---|
| `device` | `""` | capture device name as `--list-devices` prints it. Empty: the one started last time |
| `width`, `height` | `1920`, `1080` | capture size |
| `fps` | `60` | capture rate |
| `format` | `"auto"` | `auto`, `mjpeg`, `nv12` or `yuyv422`. *auto* takes raw (NV12, then YUY2) when the card offers it at this size and rate, else MJPEG |
| `audio_device` | `""` | part of the sound input's name. Empty: the input named after the capture device |

### `[output]`

| Key | Default | Meaning |
|---|---|---|
| `width`, `height` | `0`, `0` | render size. `0` x `0` fits the window, keeping the source's shape |
| `fullscreen` | `true` | borderless fullscreen |
| `vsync` | `"mailbox"` | `mailbox`, `fifo` or `immediate`. If the display does not offer the mode: mailbox falls back to fifo, immediate to mailbox then fifo |
| `backend` | `"dx12"` on Windows, else `"auto"` | `auto`, `dx12` or `vulkan`. Used at the next start |

### `[params]`

Values outside the range are clamped, not rejected.

| Key | Default | Range | Meaning |
|---|---|---|---|
| `upscaler` | `"easu"` | `easu`, `lanczos3`, `bilinear` | AMD FSR 1, Lanczos-3 or bilinear |
| `denoise_strength` | `0.5` | 0 to 1 | noise reduction: how different a neighbouring pixel may be and still be averaged in. Higher removes more noise and more fine detail |
| `sharpness` | `0.4` | 0 to 2 | RCAS, AMD's scale: 0 is sharpest, each +1 halves it |
| `exposure` | `0.0` | -5 to 5 | stops |
| `contrast` | `1.0` | 0.1 to 3 | pivots on mid grey |
| `saturation` | `1.0` | 0 to 3 | |
| `vibrance` | `0.0` | -1 to 1 | saturation that is weaker on already vivid colours |
| `temperature` | `0.0` | -1 to 1 | cool to warm |
| `tint` | `0.0` | -1 to 1 | green to magenta |
| `gamma` | `1.0` | 0.2 to 5 | display gamma trim |
| `lift` | `0.0` | -1 to 1 | raises the blacks. A number, or `[r, g, b]` |
| `lgg_gamma` | `1.0` | 0.1 to 10 | midtones. A number, or `[r, g, b]` |
| `gain` | `1.0` | 0 to 4 | scales the whites. A number, or `[r, g, b]` |
| `lut_path` | `""` | | path to a `.cube` file (3D, size 2 to 256) |
| `lut_strength` | `1.0` | 0 to 1 | blend between the ungraded and the LUT picture |
| `split_pos` | `0.5` | 0 to 1 | where the before / after divider sits |
| `content_fps` | `0.0` | 0 to 240 | the game's own rate. 0 measures it |
| `target_fps` | `0.0` | 0 to 480 | generate frames up to this rate. 0 is off |
| `hud_zones` | `[]` | up to 8 | areas kept out of frame generation: `[left, top, right, bottom]` as fractions of the picture, with an optional fifth number, non-zero for an ellipse |
| `ai_framegen` | `false` | | `true`: frames from the AI model; `false`: the built-in method |
| `ai_still` | `true` | | AI method: still areas are left as the real picture |
| `full_range` | left out | `true`, `false` | brightness range of the video. Left out: trust what the card reports |
| `matrix` | left out | `"bt601"`, `"bt709"`, `"auto"` | colour standard. Left out or `auto`: what the card reports, else BT.709 from 720 lines up and BT.601 below |

### `[passes]`

| Key | Default | Meaning |
|---|---|---|
| `denoise` | `false` | noise reduction, before the upscale. For a card that sends compressed video (MJPEG); on uncompressed video it only softens |
| `upscale` | `true` | `false`: plain bilinear, whatever `upscaler` says |
| `rcas` | `true` | sharpening |
| `grade` | `true` | colour grading |
| `lut` | `true` | LUT, when one is loaded |
| `split` | `false` | before / after view |
| `overlay` | `true` | stats overlay |

### `[audio]`

| Key | Default | Range | Meaning |
|---|---|---|---|
| `enabled` | `true` | | play the card's sound |
| `volume` | `1.0` | 0 to 2 | |
| `delay_ms` | `0.0` | 0 to 500 | hold the sound back |

### `[ai]`

| Key | Default | Meaning |
|---|---|---|
| `model` | `""` | which installed AI model to use: the name of a file in `%LOCALAPPDATA%\CapScale\ai`, without `.onnx`. Empty: the first one there |

Read at start. See [Frame generation](frame-generation.md#engines-and-models).

### `[hotkeys]`

Each line is `Key = "action"`. Keys are named as the window reports them: `F1` to
`F12`, `Escape`, `Enter`, `ArrowLeft` and so on; a character key is its
lower-case letter (`s = "save"`).

The file's entries are added to the defaults below. To free a default key, give it
an empty action: `F9 = ""`.

| Default key | Action |
|---|---|
| `Escape` | `stop` |
| `F1` | `toggle:panel` |
| `F2` | `toggle:overlay` |
| `F3` | `toggle:split` |
| `F4` | `cycle:upscaler` |
| `F5` | `toggle:rcas` |
| `F6` | `toggle:grade` |
| `F7` | `toggle:lut` |
| `F8` | `toggle:audio` |
| `F9` | `pick_source` |
| `F11` | `toggle:fullscreen` |

All actions:

| Action | Does |
|---|---|
| `toggle:<name>` | flips `denoise`, `upscale`, `rcas`, `grade`, `lut`, `split`, `overlay`, `panel` (settings window), `fullscreen`, `audio` or `zones` (show and move HUD zones) |
| `cycle:upscaler` | next upscaler |
| `stop` | stop capture and hide the picture window |
| `pick_source` | show the settings window |
| `save` | write the current settings to `config.toml` |
| `quit` | exit |

## Command line

```
capscale [--list-devices] [--config PATH]
         [--device NAME] [--size WxH] [--fps N] [--format FMT]
         [--out WxH] [--windowed] [--vsync MODE]
         [--test-pattern FMT[:FPS]]
```

| Flag | Meaning |
|---|---|
| `--list-devices` | print the capture devices and the modes they offer, then exit |
| `--config PATH` | use this `config.toml` |
| `--device NAME` | capture device; capture starts right away |
| `--size WxH` | capture size, e.g. `1920x1080` |
| `--fps N` | capture rate |
| `--format FMT` | `auto`, `mjpeg`, `nv12` or `yuyv422` |
| `--out WxH` | render size, e.g. `2560x1440`. Default: fit the display |
| `--windowed` | a window instead of borderless fullscreen |
| `--vsync MODE` | `mailbox`, `fifo` or `immediate` |
| `--test-pattern FMT[:FPS]` | colour bars instead of a card: `nv12`, `yuy2` or `i420`. `i420:30` is a 30 fps picture inside the capture rate, for trying frame generation |
| `--self-test` | load every part without opening a window; exit code 0 if all is there. Used by the installer build |

Flags stay on top of `config.toml` for the whole run, also after a live reload.

## Files and folders

| Path | What |
|---|---|
| `config.toml` in the current folder | settings, when run from source |
| `%APPDATA%\CapScale\config.toml` | settings of the installed program |
| `last_source.json`, next to `config.toml` | the device, size, rate and format started last |
| `self-test.log`, next to `config.toml` | the error from a failed `--self-test` |
| `%LOCALAPPDATA%\CapScale\ai\` | the optional AI download: `site\` (ONNX Runtime with TensorRT or DirectML), one `.onnx` file per model, `engines\` (what TensorRT prepared for this graphics card, per capture size) |

Deleting `%LOCALAPPDATA%\CapScale\ai` removes the AI method. Deleting only its
`engines` folder makes the model be prepared again on next use.

## Environment variables

| Variable | Effect |
|---|---|
| `WGPU_BACKEND` | with `backend = "auto"`, picks the graphics API (`dx12`, `vulkan`). `backend = "dx12"` or `"vulkan"` overrides it |
| `APPDATA` | where the installed program keeps `config.toml` |
| `LOCALAPPDATA` | where the AI download lives |
