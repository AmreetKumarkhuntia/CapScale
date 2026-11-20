# Third-party licences

## AMD FidelityFX Super Resolution 1.0

`python/capscale/shaders/easu.wgsl` and `python/capscale/shaders/rcas.wgsl` are
WGSL ports of `FsrEasuF` / `FsrEasuCon` and `FsrRcasF` / `FsrRcasCon` from
`ffx_fsr1.h`, with the approximation helpers they use from `ffx_a.h`
(<https://github.com/GPUOpen-Effects/FidelityFX-FSR>). What was changed in the
port is listed at the top of each shader file.

```
Copyright (c) 2021 Advanced Micro Devices, Inc. All rights reserved.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## Libraries

Linked or imported, not copied into this repository; each keeps its own licence.

| Library | Used for | Licence |
|---|---|---|
| [wgpu](https://github.com/gfx-rs/wgpu) | GPU device, shaders, presentation | MIT or Apache-2.0 |
| [winit](https://github.com/rust-windowing/winit) | Window and input | Apache-2.0 |
| [egui](https://github.com/emilk/egui), egui-wgpu, egui-winit | Stats overlay and options panel (bundles the Ubuntu and Hack fonts) | MIT or Apache-2.0 |
| [PyO3](https://github.com/PyO3/pyo3), [rust-numpy](https://github.com/PyO3/rust-numpy) | Python bindings, borrowing NumPy arrays | MIT or Apache-2.0, BSD-2-Clause |
| [bytemuck](https://github.com/Lokathor/bytemuck), [half](https://github.com/VoidStarKat/half-rs), [pollster](https://github.com/zesterer/pollster) | Byte casts, half floats, blocking on futures | Zlib / MIT / Apache-2.0 |
| [PyAV](https://github.com/PyAV-Org/PyAV) (FFmpeg) | Capture and MJPEG decoding | BSD-3-Clause (FFmpeg: LGPL/GPL) |
| [NumPy](https://numpy.org) | Frame buffers | BSD-3-Clause |
| [sounddevice](https://github.com/spatialaudio/python-sounddevice) (PortAudio) | Game audio capture and playback | MIT |

## Optional: AI frame generation

Not in this repository and not in the installer. `scripts/install-ai.ps1` downloads
these to the user's own machine, each under its own licence.

| Component | Used for | Licence |
|---|---|---|
| [RIFE](https://github.com/hzwer/Practical-RIFE) v4.15 lite, as converted to ONNX in the [vs-mlrt](https://github.com/AmusementClub/vs-mlrt) model releases | The frame interpolation model | MIT |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime) | Running the model | MIT |
| NVIDIA CUDA runtime, cuDNN and TensorRT (from PyPI) | Running it on the graphics card | NVIDIA's own licence terms: free to use, not open source |
