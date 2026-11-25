# User guide

## What you need

- 64-bit Windows.
- A capture card that offers MJPEG, NV12 or YUY2 video. Raw NV12 or YUY2 gives less
  delay and a cleaner picture than MJPEG.
- A graphics card with DirectX 12 or Vulkan.
- For the optional AI frame generation: an NVIDIA RTX card and about 4.6 GB of disk for
  the fastest runtime. Any other DirectX 12 card (AMD, Intel) gets a slower one of
  about 150 MB.

## Install and remove

Download `CapScale-Setup-<version>.exe` from the
[Releases](https://github.com/AmreetKumarkhuntia/CapScale/releases) page and run it.
It installs for the current user only, so it never asks for administrator rights.

Options on the installer's task page:

| Option | Default | Does |
|---|---|---|
| Create a desktop shortcut | on | adds CapScale to the Desktop (it is always in the Start menu) |
| Download AI frame generation | off | fetches the AI runtime for your graphics card (about 4.6 GB for NVIDIA RTX, about 150 MB otherwise); a console window shows the download, press Enter when it says it is done |

The AI download can be added later from **CapScale AI setup** in the Start menu.

To remove CapScale use Windows' *Installed apps*. That removes the program and the
AI download. Your settings stay in `%APPDATA%\CapScale`; delete that folder for a
clean slate.

## Getting a picture

1. Connect the console's HDMI to the capture card and the card to the PC by USB.
2. Close OBS, Discord streaming, or anything else using the card. A capture device
   has one owner at a time.
3. Start CapScale. On the **Source** tab pick the card, 1920 x 1080 and 60, then
   press **Start capture**.
4. The picture opens in its own window. F11 toggles fullscreen, Escape stops.

The choice is remembered: the next start has the same card and mode picked.

## The two windows

- **Settings window.** Opens with the app and stays open. Closing it quits CapScale.
- **Picture window.** Appears when capture starts and hides when it stops. Closing
  it (or pressing Escape) stops capture and returns you to the settings window.

Changes in the settings window apply at once. They are kept for the next start only
after **Save to config.toml**. **Reset look** puts noise reduction, sharpening, colour and LUT
strength back to their defaults.

## Settings window, tab by tab

### Getting started

The steps above. Shown first on a machine that has not captured before.

### Source

| Control | Meaning |
|---|---|
| Device, Resolution, Frame rate | the card and the mode to ask it for. Only modes the card offers are listed |
| Start capture / Restart capture | open the card with that mode |
| Stop | stop capture and hide the picture window |
| Refresh devices | look for cards again, after plugging one in |
| Range: auto / limited / full | how the card's brightness values are read. Switch it if the picture is washed out or too dark |
| Matrix: auto / bt601 / bt709 | which colour standard the card uses. Switch it if colours are slightly tinted |

The line above Range shows what is arriving, for example
`1920x1080 i420 limited bt709`.

### Output

| Control | Meaning |
|---|---|
| render size | the size the picture is upscaled to: fit the window, 1080p, 1440p or 4K |
| Borderless fullscreen | fullscreen without a mode switch |
| Present: mailbox / fifo / immediate | how finished pictures are handed to the display. *mailbox* is the default: no tearing, lowest delay. *fifo* is classic vsync. *immediate* can tear |
| Graphics API (next start) | auto, dx12 or vulkan. Takes effect at the next start; the line below shows the one in use |
| Stats overlay on the picture | the numbers in the top-left corner (F2) |
| Before / after split | the untouched source on the left, the processed picture on the right; drag the divider (F3) |

### Frame rate

See [Frame rate](#frame-rate-1) below.

### Picture

| Control | Meaning |
|---|---|
| Noise reduction, strength | for a card that sends MJPEG: removes the compression speckle before the picture is enlarged and sharpened, which would otherwise make it stand out. Higher is smoother; too high and fine texture goes flat |
| Upscaler: AMD FSR 1 / Lanczos-3 / Bilinear | how the picture is enlarged. With the box unticked it is plain bilinear |
| AI upscale first (NVIDIA RTX): off / Low ... Streaming Ultra | NVIDIA's video upscaler enlarges each frame to the size shown, and cleans up compression noise, before the other steps. Higher levels take longer per frame. Needs the AI download on an NVIDIA RTX card. With it on, try the noise reduction off: the two do the same job |
| AMD FSR 1 sharpening (RCAS), sharpness | **0 is the sharpest**; each +1 halves the effect, up to 2 |
| Colour grading | exposure (stops), contrast, saturation, vibrance, temperature, tint, gamma, lift, lgg gamma, gain |
| Apply 3D LUT, Load .cube..., Clear, strength | a look-up table in `.cube` format (3D only), blended by strength |

### Audio

| Control | Meaning |
|---|---|
| Game audio | play the card's sound (F8) |
| volume | 0 to 1.5 |
| delay ms | holds the sound back, for when it arrives before the picture |

## Keys

Keys work while the picture window has focus.

| Key | Does |
|---|---|
| F1 | settings window on or off |
| F2 | stats overlay |
| F3 | before / after split |
| F4 | next upscaler |
| F5 / F6 / F7 | sharpening / colour grading / LUT on or off |
| F8 | mute or unmute game audio |
| F9 | show the settings window |
| F11 | fullscreen on or off |
| Esc | stop capture, hide the picture window |

They can be changed in the `[hotkeys]` section of `config.toml`; see
[Configuration](configuration.md#hotkeys).

## Frame rate

A console sends 60 pictures a second even when the game runs at 30: every frame
arrives twice. This tab turns those repeats into generated in-between frames.

| Control | Meaning |
|---|---|
| Current (game): auto / 30 / 60 | the game's own rate. *auto* measures it from the repeated frames |
| Target: off / 60 / 90 / 120 / 180 | the rate to show. *off* shows frames as they arrive |
| Method: Built-in / an AI model | what makes the in-between frames. The AI models appear once the AI download is installed: RIFE on any card, and NVIDIA Low, Medium and High on an NVIDIA RTX card |
| AI: leave still areas untouched | where nothing moves, a generated frame is the real picture |
| HUD zones: + box, + circle, remove last | areas kept out of frame generation |
| Show and move zones | draws the zones as red outlines on the picture so they can be dragged and resized |

Only whole multiples are generated: a target of 100 on a 30 fps game gives 90.

**What it costs.** A real frame waits while the generated frames before it are
shown: about 17 ms for 30 to 60, about 25 ms for 30 to 120. An AI model adds the
time one generated frame takes (at 1080p on an RTX 5070: RIFE 10 to 12 ms, NVIDIA Low
3 ms, Medium 5 ms, High 18 ms). The line at
the bottom of the tab and the stats overlay show the real figures.

**What to expect.** Camera pans and a still HUD are handled well. A small object
moving against the background, and a thin strip at the screen edges during a pan,
can smear or flicker in generated frames.

**HUD zones.** A minimap turning over a moving road cannot be interpolated cleanly.
Add a zone, tick *Show and move zones* and drag the outline over it. Inside a zone,
generated frames show the real picture. Up to eight zones; they are saved with
*Save to config.toml*.

**AI method.** A neural network trained on real video, where the built-in method
is a block search. Try both on your game. Things to know:

- On an NVIDIA RTX card, the first use of a capture size prepares the model for it, which
  takes a minute or two. The built-in method runs meanwhile.
- A generated frame should be ready within its time slot. With RIFE, 30 to 60 and 30 to 90
  fit on an RTX 5070. The model you choose is the one used either way: where it does not
  fit (30 to 120, or a 60 fps game to 120) some generated frames are left out and the
  stats overlay says so. NVIDIA Low and Medium are quick enough for those too; NVIDIA
  High needs a faster card than an RTX 5070.
- With **AI upscale first** on, frames are generated from the enlarged picture, which
  takes more than twice as long per frame at 1440p (NVIDIA Low 7 ms, Medium 12 ms on an
  RTX 5070). Both still fit 30 to 60 there; for 30 to 120 use NVIDIA Low.
- On other cards (AMD, Intel) the model runs through DirectML, which is slower: at the
  speed measured so far, 30 to 60 fits a 720p capture and not a 1080p one. No AMD or
  Intel card has been timed; the stats overlay shows what yours does.
- Small still things on top of motion (a crosshair, a minimap) are drawn by the
  model and can look a little soft. Put a HUD zone over them.

How both methods work is in [Frame generation](frame-generation.md).

## Sound

The card's sound input (the one named after the capture device) is played on the
default output device. On Windows it arrives about 50 ms after the picture, which
is the floor for shared audio and below what people notice. If the wrong input is
picked, set `audio_device` under `[capture]` in `config.toml` to part of the right
input's name.

## Reading the stats overlay

```
capture  60.0 fps   render  60.0 fps
added latency  decode 3.1 + present 4.2 = 7.3 ms
steadiness     1% low 59.4 fps
1920x1080 i420 limited bt709  ->  2560x1440 easu  [mailbox]
gpu ms  yuv 0.11  upscale 0.42  rcas 0.20  grade 0.09  composite 0.08  = 0.90
Escape close   F1 settings   F3 split   audio 54 ms
```

| Line | Meaning |
|---|---|
| capture / render | frames arriving per second, pictures drawn per second. With built-in frame generation it reads `game 30.0 fps x2 = 60 fps shown` |
| added latency | time to decode a frame, plus time from handing it to the renderer until it is presented. Display scan-out is not included |
| steadiness | the rate of the slowest 1% of pictures over the last ten seconds. Close to the render rate means even pacing; well below it means stutter |
| source -> output | what arrives, the render size, the upscaler and the present mode |
| gpu ms | GPU time per pass. `n/a` if the graphics card has no timestamp queries |
| red text | a shader that failed to compile (only when editing shaders) |
| AI frames: ... | what the AI method is doing |
| AI upscale: ... | what the AI upscaler is doing, and its time per frame |
| last line | the main keys and the sound's delay |

## Troubleshooting

| What you see | Likely cause | What to do |
|---|---|---|
| "No capture devices found" | another program owns the card, or it is unplugged | close OBS / Discord, then **Refresh devices** |
| "This device offers no MJPEG, NV12 or YUY2 mode" | the card only offers formats CapScale cannot take | use another card or driver |
| Black picture | HDCP | PS5: Settings > System > HDMI, turn HDCP off |
| `capture: ... - retrying` in the overlay | the card was unplugged or taken by another program | CapScale retries every second; free the card |
| Washed out or too dark | wrong range | switch **Range** on the Source tab, then save |
| Slight colour tint | wrong matrix | switch **Matrix** on the Source tab, then save |
| No sound, or the wrong sound | the sound input was not matched by name | set `audio_device` under `[capture]` |
| No dialogue | surround output; the centre channel is dropped | PS5 audio: Linear PCM, stereo |
| Sound before the picture | frame generation delays the picture | raise **delay ms** on the Audio tab |
| Sound well after the picture | Bluetooth headphones | use wired or 2.4 GHz headphones |
| Frame generation looks wrong on a 30 fps game | 120 Hz or VRR output: each frame arrives four times, not two | PS5: turn 120 Hz output and VRR off |
| Stutter | monitor refresh is not a multiple of the shown rate | 60 on a 120, 180 or 240 Hz display paces evenly |
| Tearing | *immediate* present mode | use *mailbox* |
| Method offers only Built-in, AI upscale is greyed out | the AI download is not installed, or for AI upscale the card is not an NVIDIA RTX | run **CapScale AI setup**, then restart |
| No NVIDIA choices after an update | the AI download is from before they existed | run **CapScale AI setup** again; it adds them |
| `AI frames: preparing the model` | first use of this capture size | wait a minute or two |
| `AI frames: N ms each, too slow for this rate: some are left out` | the model cannot finish a frame in its slot. It stays the one used; frames it has no time for are skipped | lower the target, or pick a faster model. With AI upscale on, frames are generated at the enlarged size: at 1440p on an RTX 5070 RIFE takes 18 ms, just over the slot of 30 to 60, and NVIDIA Low or Medium fit |
| `AI frames: <format> video is not handled - built-in used` | the card delivers YUY2 or 4:2:2 | the AI method takes 4:2:0 only (MJPEG, NV12) |
| `AI upscale: N ms a frame, not keeping up: frames are left out` | the level cannot finish a frame before the next is due (or capture has just caught up after a stall, which passes in a second or two). It stays the one used | pick a lower level, or a lighter frame generation model |
| `AI upscale: the picture is not being enlarged at this size` | the window is no larger than the capture | nothing to do: there is nothing to upscale |
| Card falls back to slow, compressed modes | weak USB port or cable | a rear motherboard port and the cable that came with the card |
| The app does not start from the shortcut | a start-up failure | the error is shown in a message box; from source, run `uv run capscale` in a console to see the full output |
