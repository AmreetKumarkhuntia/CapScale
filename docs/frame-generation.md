# Frame generation

Showing a 30 fps game at 60, 90 or 120 by generating the pictures in between.
There are two methods, a built-in one in shaders and an optional AI model, and
they share the first two steps.

## The problem

A console's HDMI signal is 60 pictures a second whatever the game does. A 30 fps
game arrives with every frame twice:

```
signal:   A  A  B  B  C  C  ...      60 per second
game:     A     B     C     ...      30 per second
```

Generating frames needs each of the game's frames exactly once.

## Step 1: find the repeats

`capture/repeats.py`, class `Cadence`. It runs on the capture thread, only while
frame generation is on.

Repeats are not byte-identical: the signal carries noise, and how strong it is
depends on the scene, so no fixed "is this a repeat" threshold holds up. What does
hold is the rhythm. On a 30 fps game the frames at one parity (every second frame)
change and the frames at the other do not. So the two parities are compared with
each other, and the noise, which is the same on both, cancels out.

For each captured frame:

1. Count the pixels (on a half-resolution brightness grid) that differ from the
   previous frame by more than 14 levels.
2. Keep a running mean and variance of that count per parity.
3. If the gap between the two means is large against their wander, the signal is
   **halved** (a 30 fps game) and the parity with the larger changes is the one to
   keep. If the gap is small while both parities are changing, it is not halved.
4. A change of mind has to be seen on 12 frames in a row before it is acted on.
   When the rhythm slips (the game drops a frame), the switch to the other parity
   happens on a frame of the new parity, so no picture is lost.

A still picture (a menu) has no rhythm to read, so it changes nothing that was
learned.

With **Current (game)** set to 30 or 60 instead of *auto* the answer is forced and
obeyed at once; `Cadence` then only finds the parity.

The capture thread reports the result as `detected_fps` and drops the repeats, so
the renderer receives 30 frames a second.

## Step 2: the plan

`params::frame_plan(content_fps, measured_fps, target_fps)` in `src/params/plan.rs`
returns how many pictures to show per game frame and how long a game frame lasts.

- The game rate is `content_fps` if set, else the measured rate pulled to the
  standard rate it is near (24, 25, 30, 50, 60, 100, 120, within 6%).
- `multiple = round(target / game rate)`, between 1 and 8. Only whole multiples
  are generated: a target of 100 on a 30 fps game gives 3x = 90.
- A multiple of 1 means nothing to generate.

Both methods follow this one plan; Python reaches it as `_core.frame_plan`.

## The schedule and its cost

A generated frame sits between two real frames, so the newer one must have
arrived before anything between them can be shown. With a multiple of *m* and a
game frame lasting *T*, when frame B arrives:

```
m = 2 (30 -> 60)          m = 4 (30 -> 120)
t = 0      A+1/2          t = 0       A+1/4
t = T/2    B              t = T/4     A+2/4
                          t = 2T/4    A+3/4
                          t = 3T/4    B
```

So a real frame is shown `(m - 1) / m` of a game frame late: 16.7 ms for 30 to 60,
25 ms for 30 to 120. The AI method adds the time it takes to make the first
generated frame.

## The built-in method

Motion interpolation in three shaders, run by `Pipeline::encode` when the draw is
for a generated picture. GPU cost is under a millisecond.

### `luma.wgsl`: a brightness pyramid

Each source frame is reduced to a half-size brightness image, that one halved
again, and so on. The depth (2 to 6 levels) is chosen so the smallest level is
between 40 and 80 pixels wide: 5 levels for 1080p. Smaller than that and there is
too little picture to match. A frame's pyramid is built once and reused for every
generated picture between the same two frames.

### `flow.wgsl`: where did each pixel move?

Run from the smallest level to the largest. For each pixel it compares 3 x 3
patches of the two frames under candidate motion vectors and keeps the best match.

- The smallest level searches +/-4 of its own pixels from nothing. At 1080p that
  covers +/-128 pixels of movement between two game frames.
- Every finer level starts from the vector found one level up and only nudges it
  by one pixel. It also tries what its four neighbours found one level up: a patch
  that started from a wrong vector cannot climb out one pixel at a time, but it
  can adopt a good vector from next door.
- The search is done *at the moment being generated*: a pixel of the new picture
  at `p` came from `p - t*v` in the older frame and ends at `p + (1-t)*v` in the
  newer. That gives every output pixel a vector, so the warp leaves no holes.
- "Nothing moved" is always a candidate and gets a head start: a moving vector
  must match at least twice as well. Otherwise, with noise in the picture, a still
  HUD or crosshair over a moving background flickers between the two answers.
- A tiny cost per pixel of vector length settles ties in flat areas.

### `interp.wgsl`: build the picture

Both frames are moved along the vectors to the moment `t` and blended. Three
safeguards:

- At the picture's edge, content sliding in exists in only one frame; that frame
  is used alone.
- Where the two moved frames disagree, the motion is wrong or something was
  covered up. There the nearer real frame is shown, unmoved. A repeated frame in
  one spot is far less visible than a patch of wrongly moved picture.
- Inside a HUD zone the nearer real frame is shown, untouched.

### Timing on the render thread

`Gfx::render` in `src/render/window.rs` works out, from the time since the newest
frame was pushed, which slot of the plan the draw falls in. That gives the phase
(`Step::phase`, in the uniform block as `g.lift.w`) and the time of the next slot
(`next_tick`), for which the event loop sets its wake-up. The last slot shows the
newest frame as it is.

## HUD zones

Up to eight rectangles or ellipses, as fractions of the picture, stored in
`Params::zones` and saved as `hud_zones`. Inside a zone a generated frame shows a
real frame, so nothing there can smear or flicker.

- Built-in method: `in_hud_zone` in `common.wgsl`, used by `interp.wgsl`.
- AI method: `ai.keep_zones` copies the real pixels over the model's output.

Both show the older frame for the first half of the interval and the newer for
the second.

## The AI method

The open-source [RIFE](https://github.com/hzwer/Practical-RIFE) model
(`rife_v4.15_lite`), run through ONNX Runtime. It is not part
of the app; `scripts/install-ai.ps1` puts it in `%LOCALAPPDATA%\CapScale\ai`.
`ai.available()` is true when a model file and ONNX Runtime are there, and only
then is the switch in the settings window enabled.

### Engines and models

Two things plug in, each from a table.

**The engine** is how ONNX Runtime reaches the graphics card. `install-ai.ps1`
picks a runtime from the card's name, and `ai/engines.py` uses the first entry of
`ENGINES` that the installed runtime offers:

| Engine | Cards | Runtime packages | 1080p frame on an RTX 5070 |
|---|---|---|---|
| TensorRT | NVIDIA RTX | `onnxruntime-gpu`, `tensorrt-cu13-libs` (about 4 GB) | 10 to 12 ms |
| DirectML | any DirectX 12 card: AMD, Intel, older NVIDIA | `onnxruntime-directml` (about 150 MB) | about 33 ms (13 ms at 720p) |

DirectML runs the model in full precision and TensorRT in half. No AMD or Intel
card has been timed. `-Runtime nvidia` or `-Runtime directml` on the script
overrides the choice.

**The model** is any `.onnx` file in the pack that has the inputs and outputs
below; its name is the file name. `[ai] model` in `config.toml` names the one to
use, and empty means the first. Adding either is covered in
[Development](development.md#adding-an-engine-or-a-model).

### The model takes the card's frames directly

RIFE itself wants two RGB pictures as floats. `scripts/build_ai_model.py` wraps it
so that the colour conversion runs on the GPU with the rest, and the app only
copies 8-bit planes in and out:

| | Name | Type |
|---|---|---|
| in | `y0`, `u0`, `v0`, `y1`, `u1`, `v1` | the two frames as 8-bit 4:2:0 planes |
| in | `t` | the moment, 0 to 1 |
| in | `still` | 1 = leave still areas untouched |
| in | `to_rgb`, `rgb_offset`, `to_yuv`, `yuv_offset` | the colour transform, from the renderer's own `yuv_matrix` |
| out | `y`, `uv` | the generated frame as 8-bit 4:2:0 planes |

**Still areas.** The wrapper measures how much the brightness changed between the
two frames, in 8 x 8 cells, widened to their neighbourhood. Where it is at rest
the output is the real frame (the older before the midpoint, the newer after);
where it moves it is the model's picture; in between they are blended. A menu, a
still scene or a large HUD panel therefore comes out untouched. `ai_still = false`
switches this off and the model draws everything.

### `ai.Model`

One ONNX Runtime session for one model and one frame size, opened by
`engines.session`. TensorRT builds an engine for the
graphics card the first time a size is used, which takes a minute or two; it is
kept in `ai\engines`. The model is run a few times on load to time it
(`Model.took`), and that figure is kept up to date while it runs.

### `ai.FrameGen`: the AI thread

The capture thread calls `submit` for each game frame. It returns `False`, and the
caller shows the frame itself so the built-in method takes over, when:

| Reason | Status line |
|---|---|
| the model for this size is still loading | `AI frames: preparing the model (a minute or two the first time)` |
| one generated frame takes longer than its slot | `AI frames: N ms each, too slow for this rate - built-in used` |
| the frame is not 4:2:0 (`i420` or `nv12`, even size) | `AI frames: <fmt> video is not handled - built-in used` |
| the model failed to load or run | `AI frames not available: ...` / `AI frames stopped: ...` |

Otherwise the frame is queued. For each one the thread:

1. generates the in-between frames from the previous game frame and this one;
2. copies the real pixels into the HUD zones;
3. pushes the first generated frame as soon as it is ready, the rest one slot
   apart, and the real frame one slot after the last.

If the previous frame is missing, too old (three game frames) or a different
size, or if the thread has fallen behind, the real frame is pushed alone.

While the AI thread is the one pushing, the capture thread sets `ai_active`, and
the renderer's own generation stands down: the frames arrive ready-made and go
through the rest of the pipeline like any other. The lateness of each pushed frame
is added to its `decode_ms`, so the overlay's latency figure stays true.

## Which to use

| | Built-in | AI (RIFE) |
|---|---|---|
| Needs | any supported graphics card | a download: 4 GB for NVIDIA RTX, 150 MB for other cards |
| Extra delay at 30 to 60 | about 17 ms | about 28 ms (RTX 5070 with TensorRT, 1080p) |
| Cost per generated frame | under 1 ms of GPU | 10 to 12 ms with TensorRT, about 33 ms with DirectML (RTX 5070, 1080p) |
| Rates it can reach | any multiple up to 8x | as far as a frame fits its slot. On an RTX 5070: 30 to 60 and 30 to 90 at 1080p with TensorRT; 30 to 60 at 720p with DirectML |
| Video it takes | everything the renderer takes | 4:2:0 only (MJPEG, NV12) |
| Weak spots | small objects against a moving background; a thin strip at the edges during a pan | small still things on top of motion look a little soft |

Try both on your game.

## Checking it

| Command | Checks |
|---|---|
| `uv run capscale --test-pattern i420:30 --windowed` | a 30 fps picture in a 60 Hz signal, to watch either method |
| `cargo test` | the built-in method on the real GPU against a scene with known motion: generated frames follow the motion, overlays stay steady under noise, a HUD zone shows the real frame |
| `uv run pytest tests/python/test_repeats.py` | repeat detection, including noise and rhythm slips |
| `uv run pytest tests/python/test_ai.py` | the AI thread's timing and fallbacks with a stand-in model, the choice of engine and model, and the real model where it is installed |
| `uv run python -m capscale.ai [WIDTHxHEIGHT] [MODEL]` | loads the real model, says which engine runs it, times it, and scores its output on a picture that moves by a known amount |
