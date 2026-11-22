# Render pipeline

Each displayed picture is produced by a chain of GPU passes. Every pass is one
fullscreen triangle and one fragment shader, one file in
`python/capscale/shaders/`. The chain is built and run by
`src/render/pipeline.rs`.

## The passes

| # | Pass | Shader | Reads | Writes | Size |
|---|---|---|---|---|---|
| 1 | YUV to RGB | `yuv2rgb.wgsl` | the source planes | `rgb[newest]` | capture |
| 2 | Frame generation (optional) | `luma.wgsl`, `flow.wgsl`, `interp.wgsl` | `rgb[older]`, `rgb[newest]` | `mid` | capture |
| 3 | Noise reduction (optional) | `denoise.wgsl` | `rgb[newest]` or `mid` | `den` | capture |
| 4 | Upscale | `easu.wgsl`, `lanczos3.wgsl` or `bilinear.wgsl` | `rgb[newest]`, `mid` or `den` | `a` | output |
| 5 | Sharpen (optional) | `rcas.wgsl` | `a` or `b` | the other one | output |
| 6 | Grade (optional) | `grade.wgsl` | `a` or `b` | the other one | output |
| 7 | LUT (optional) | `lut3d.wgsl` | `a` or `b` | the other one | output |
| 8 | Composite | `composite.wgsl` | `rgb[newest]`, the last of `a` / `b` | the window | window |

Notes on the chain:

- **Two source frames are kept.** `rgb` is a pair of targets; `Pipeline::newest`
  says which holds the newest converted frame. Frame generation needs both, and
  the composite pass uses the newest as the "before" side of the split.
- **Noise reduction runs at capture size, before the upscale.** Compression noise
  is one capture pixel in size there, and the upscaler and the sharpener then both
  work on a clean picture. The split's "before" side still shows the untouched frame.
- **`a` and `b` ping-pong.** Passes 5 to 7 each read one and write the other, so a
  pass that is switched off costs nothing: it is simply not run.
- **Upscale always runs**, so `a` always holds a picture at output size. With the
  upscaler switched off it is the bilinear shader.
- **The stats overlay and HUD zone frames** are drawn by egui into the same render
  pass as the composite, on top of it.
- Intermediate targets are `Rgba16Float`. The window surface uses a non-sRGB
  format where the display offers one, because the picture is already
  display-encoded.

Colour stays gamma-encoded (as the console sent it) through the chain. Only the
grade pass converts to linear light, does its work, and converts back.

### AI upscaling ahead of the passes

Optional, on an NVIDIA RTX card with the AI download: `ai_upscale` names a level of
NVIDIA's Video Super Resolution (`ai/nvidia.py`, the `nvidia-vfx` package). It is
not a pass. `ai.Upscale` (`ai/upscale.py`) takes each frame from the capture thread
before anything else is done with it: on a thread of its own it enlarges the frame to
`Renderer.output_size` and hands the larger one on, to AI frame generation if that is
on and otherwise to the renderer. So only the game's real frames are enlarged, and
frames are generated between enlarged ones; doing it the other way round would
enlarge every generated frame as well. The chain above then starts from a frame that
is already at output size: pass 4 maps it one to one, and noise reduction and the
built-in frame generation, if they are on, work on the enlarged picture.

| Level | 1080p to 1440p on an RTX 5070, planes in to planes out |
|---|---|
| `nvidia-low`, `nvidia-medium` | 4 ms |
| `nvidia-high`, `nvidia-highbitrate-high` | 7 ms |
| `nvidia-ultra` | 8 ms |
| `nvidia-highbitrate-ultra`, `nvidia-streaming-ultra` | 11 ms |

A frame is handed on as it is, and the pass chain does the enlarging, when:

| Reason | Status line |
|---|---|
| the model for this size and level is still loading | `AI upscale: preparing the model` |
| 30 frames in a row took longer than the gap between frames | `AI upscale: N ms a frame, too slow for this rate - the app's own upscaler is used`. It is tried again after 10 seconds, then 20, doubling up to five minutes; changing the level, the size or the frame generation settings starts afresh |
| the output is no larger than the frame | `AI upscale: the picture is not being enlarged at this size` |
| the frame is not 4:2:0 | `AI upscale: <fmt> video is not handled` |
| the model failed to load or run | `AI upscale not available: ...` / `AI upscale stopped: ...` |

Frames are handed on in the order they came: one that skips the upscaler waits for
those still in it. The time a frame spends there is added to its `decode_ms`.
A new output size is taken up once two frames in a row have asked for it, so
dragging the window's edge does not reload the model at every step.

### What each shader does

| Shader | Summary |
|---|---|
| `yuv2rgb` | one branch per layout: NV12 (Y plane + interleaved CbCr), YUY2 (one texel per pixel pair), planar (three planes; bilinear chroma covers 4:2:0, 4:2:2 and 4:4:4). The matrix and range are already folded into three rows of the uniform block |
| `luma`, `flow`, `interp` | see [Frame generation](frame-generation.md#the-built-in-method) |
| `denoise` | a bilateral filter, 5 x 5: each pixel becomes an average of its neighbours, weighted by distance and by how close their colour is, so noise is averaged away and edges are not averaged across. For cards that send MJPEG, whose compression noise the sharpening and grading would otherwise multiply |
| `easu` | port of AMD FSR 1.0 EASU (edge adaptive spatial upsampling) |
| `lanczos3` | Lanczos-3, 6 x 6 taps in one pass |
| `bilinear` | one bilinear tap: the baseline to compare the others against |
| `rcas` | port of AMD FSR 1.0 RCAS with its denoise option on, because captured video carries compression noise that plain RCAS would sharpen |
| `grade` | in linear light, in this order: white balance and exposure, gain and lift, midtone gamma, contrast around mid grey (0.18), saturation, vibrance; then back to display encoding and the gamma trim. With every setting at its default it returns its input unchanged |
| `lut3d` | trilinear look-up in the `.cube` table on display-encoded colour, blended by strength |
| `composite` | fits the picture into the window (letterboxed, centred), and draws the before / after split and its divider |

## What every shader gets

`common.wgsl` is placed in front of every pass file when it is loaded
(`ShaderDir::source`), because WGSL has no include. It provides:

- the uniform block `g` (below),
- the texture bindings,
- `vs_main`, the fullscreen triangle, with `uv` running from the top-left,
- `in_hud_zone(uv)`, `srgb_to_linear`, `linear_to_srgb`.

A pass file only has to define `fs_main`.

### Bindings

| Group | Binding | Name | What |
|---|---|---|---|
| 0 | 0 | `g` | the uniform block |
| 1 | 0 | `tex_a` | 2D texture; meaning depends on the pass |
| 1 | 1 | `tex_b` | 2D texture |
| 1 | 2 | `tex_c` | 2D texture |
| 1 | 3 | `lut` | 3D texture: the loaded LUT |
| 1 | 4 | `samp` | linear filtering, clamp to edge |

Every pass uses the same layout, so one constructor builds them all. A slot a
pass does not use is bound to a 1 x 1 dummy texture.

### The uniform block

`Globals` in `src/params/types.rs` mirrors `struct Globals` in `common.wgsl`
field for field. Every field is a `vec4`, so there is no padding to get wrong. It
is filled by `Globals::new` (`src/params/globals.rs`) once per draw.

| Field | x, y, z, w |
|---|---|
| `src_size` | capture width, height, 1 / width, 1 / height |
| `out_size` | the same for the render size |
| `dst_size` | the same for the window |
| `yuv0`, `yuv1`, `yuv2` | rows of the YUV to RGB transform: `rgb.r = dot(yuv0, vec4(y, cb, cr, 1))` and so on |
| `grade` | exposure as a multiplier (`2^stops`), contrast, saturation, vibrance |
| `wb` | white balance gains for r, g, b; 1 / gamma |
| `lift` | lift r, g, b; **frame generation phase** (0 at the older frame, 1 at the newer) |
| `inv_lgg_gamma` | 1 / lgg_gamma for r, g, b |
| `gain` | gain r, g, b; **noise reduction strength**, 0 to 1 |
| `lut_min`, `lut_scale` | the LUT's input domain: `(colour - lut_min) * lut_scale` is 0..1 |
| `misc` | RCAS constant `2^-sharpness`, LUT strength, split position, LUT edge length |
| `flags` (u32) | source layout (0 NV12, 1 YUY2, 2 planar), split on, number of HUD zones, bit *i* set = zone *i* is an ellipse |
| `zones[8]` | HUD zones: left, top, right, bottom as fractions of the picture |

### Colour maths on the CPU

- `yuv_rows(matrix, full_range)` builds the three YUV rows from the BT.601 or
  BT.709 coefficients, with the limited-range scale (219 / 224 steps, offset 16)
  and the chroma offset folded in. The same rows are handed to the AI model
  (`_core.yuv_matrix`), which converts frames itself and must match exactly.
- `wb_gains(temperature, tint)` turns the two sliders into RGB gains, normalised
  so that grey keeps its brightness.

## Targets and when they are rebuilt

`Pipeline::rebuild` allocates the targets and their bind groups. It runs only when
the capture size, the render size or the LUT changes, never per frame.

| Target | Size | Count |
|---|---|---|
| `rgb` | capture | 2 |
| `den` (after noise reduction) | capture | 1 |
| `a`, `b` | render size | 1 each |
| `luma` pyramid | capture / 2, / 4, ... | 2 pyramids, one per source frame |
| `flow` | one per pyramid level | 1 set |
| `mid` (the generated frame) | capture | 1 |

The render size is `[output] width x height`, capped at the GPU's largest texture.
With `0 x 0` it is the largest size with the source's shape that fits the window
(`params::fit`).

## GPU timing

`render::Timing` measures each pass with timestamp queries: one start and one end
per slot, in the order `yuv, framegen, denoise, upscale, rcas, grade, lut, composite`
(`PASS_NAMES`). Frame generation is several passes timed as one slot: its first
pass records the start, its last the end.

Results are read back asynchronously, a frame or two later, so measuring never
stalls a draw. A slot that did not run reads 0 and is left out of the overlay. On
a graphics card without timestamp queries the overlay says `n/a`.

## Editing a shader

Shaders are loaded from the installed package's `shaders` folder (or the
`shader_dir` given to `Renderer`). The render thread checks their modification
times every 250 ms:

- saving a pass file recompiles that pass;
- saving `common.wgsl` recompiles every pass;
- if the new version does not compile, the previous one keeps running and the
  compiler's message appears in red in the stats overlay (`stats["error"]`).

So the loop is: run the app with a test pattern, edit, save, look.

```
uv run capscale --test-pattern i420 --windowed
```

`cargo test` compiles every shipped shader against the pass layout without a
window, and scores frame generation on the real GPU.

### Places that know the list of passes

If you add a pass, these need to agree:

| Where | What |
|---|---|
| `FILES` and the index constants in `src/render/types.rs` | the shader file names; every entry is compiled at start-up |
| `Pipeline::encode` in `src/render/pipeline.rs` | where in the chain it runs |
| `SLOTS`, the `SLOT_*` constants and `PASS_NAMES` | if it is timed on its own |
| `Params`, `Params::toggle`, `TOGGLES`, `Params::to_toml`, `set_params` | if it has a switch or a setting |
| `Globals` in `src/params/types.rs` **and** `common.wgsl` | if it needs a new uniform; the two must stay field for field |
| `src/ui/overlay.rs` | its controls in the settings window |
