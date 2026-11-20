"""AI frame generation: the open-source RIFE model on the GPU, through ONNX Runtime
and NVIDIA TensorRT.

Optional. The runtime is about 4 GB of NVIDIA libraries, so it is not part of the
app: scripts/install-ai.ps1 puts it and the model in PACK. Without it the app keeps to
its built-in generation (the flow and interp shaders).

A game frame comes in; the frames between it and the one before are generated and
shown first, each in its time slot, then the frame itself. So a real frame is late by
the time one generated frame takes, plus the slots before it.

    python -m ps5scaler.ai [WIDTHxHEIGHT]     load the model, time it, check its output
"""
from __future__ import annotations

import functools
import os
import queue
import sys
import threading
import time
from pathlib import Path

import numpy as np

PACK = Path(os.environ.get("LOCALAPPDATA") or Path.home() / ".local" / "share") / "PS5 Scaler" / "ai"
SITE = PACK / "site"
MODEL = PACK / "rife_yuv420.onnx"

Planes = tuple[np.ndarray, np.ndarray, np.ndarray]


def available() -> bool:
    return MODEL.is_file() and (SITE / "onnxruntime").is_dir()


@functools.cache
def runtime():
    """ONNX Runtime from PACK."""
    sys.path.append(str(SITE))
    for folder in {p.parent for p in SITE.glob("nvidia/**/*.dll")} | {SITE / "tensorrt_libs"}:
        os.add_dll_directory(str(folder))
        os.environ["PATH"] = f"{folder}{os.pathsep}{os.environ.get('PATH', '')}"
    import onnxruntime

    return onnxruntime


@functools.cache
def _colour(matrix: str, full_range: bool) -> dict[str, np.ndarray]:
    """YUV to RGB and back, in the form the model takes: pixel * matrix + offset."""
    from ._core import yuv_matrix

    rows = np.array(yuv_matrix(matrix, full_range), np.float32)
    to_rgb, offset = rows[:, :3], rows[:, 3]
    to_yuv = np.linalg.inv(to_rgb)
    return {
        "to_rgb": np.ascontiguousarray(to_rgb.T),
        "rgb_offset": np.ascontiguousarray(offset),
        "to_yuv": np.ascontiguousarray(to_yuv.T),
        "yuv_offset": -to_yuv @ offset,
    }


class Model:
    """RIFE, set up for one frame size."""

    def __init__(self, width: int, height: int):
        ort = runtime()
        options = ort.SessionOptions()
        options.log_severity_level = 3
        for name, value in (("h", height), ("w", width), ("h2", height // 2), ("w2", width // 2)):
            options.add_free_dimension_override_by_name(name, value)
        tensorrt = {
            "trt_fp16_enable": True,
            "trt_engine_cache_enable": True,
            "trt_engine_cache_path": str(PACK / "engines"),
        }
        self.session = ort.InferenceSession(
            str(MODEL), options, providers=[("TensorrtExecutionProvider", tensorrt), "CUDAExecutionProvider"]
        )
        if self.session.get_providers()[0] != "TensorrtExecutionProvider":
            raise RuntimeError("the AI runtime could not start on the graphics card")
        blank = (np.zeros((height, width), np.uint8), *[np.full((height // 2, width // 2), 128, np.uint8)] * 2)
        self.generate(blank, blank, 0.5)
        began = time.perf_counter()
        for _ in range(5):
            self.generate(blank, blank, 0.5)
        self.took = (time.perf_counter() - began) / 5

    def generate(
        self, older: Planes, newer: Planes, when: float, matrix: str = "bt709", full_range: bool = False, keep_still: bool = True
    ) -> Planes:  # fmt: skip
        """The picture `when` (0..1) of the way from `older` to `newer`. `keep_still`:
        where the two frames do not differ, it is the real picture, untouched."""
        feed = dict(zip(("y0", "u0", "v0", "y1", "u1", "v1"), (*older, *newer)))
        feed["t"] = np.array([when], np.float32)
        feed["still"] = np.array([keep_still], np.float32)
        y, uv = self.session.run(None, {**feed, **_colour(matrix, full_range)})
        return y, uv[0], uv[1]


def planes_of(data, fmt: str, width: int, height: int) -> Planes | None:
    """A frame as capture hands it over, copied into (Y, U, V). None: a layout not handled."""
    if width % 2 or height % 2:
        return None
    half = (height // 2, width // 2)
    if fmt == "i420":
        return tuple(np.array(p[:rows, :cols]) for p, (rows, cols) in zip(data, ((height, width), half, half)))
    if fmt == "nv12":
        chroma = data[width * height :].reshape(*half, 2)
        return np.array(data[: width * height].reshape(height, width)), np.array(chroma[..., 0]), np.array(chroma[..., 1])
    return None


def keep_zones(made: Planes, real: Planes, zones) -> None:
    """Inside a HUD zone the generated frame shows the real one. `zones`: (left, top,
    right, bottom, round) as fractions of the picture."""
    for plane, source in zip(made, real):
        h, w = plane.shape
        for left, top, right, bottom, is_round in zones:
            x0, x1 = max(0, int(left * w)), min(w, int(np.ceil(right * w)))
            y0, y1 = max(0, int(top * h)), min(h, int(np.ceil(bottom * h)))
            if x1 <= x0 or y1 <= y0:
                continue
            if is_round:
                yy, xx = np.ogrid[y0:y1, x0:x1]
                dx = (xx + 0.5 - (left + right) / 2 * w) / ((right - left) / 2 * w)
                dy = (yy + 0.5 - (top + bottom) / 2 * h) / ((bottom - top) / 2 * h)
                inside = dx * dx + dy * dy <= 1
                plane[y0:y1, x0:x1][inside] = source[y0:y1, x0:x1][inside]
            else:
                plane[y0:y1, x0:x1] = source[y0:y1, x0:x1]


def _sleep_until(moment: float) -> None:
    wait = moment - time.perf_counter()
    if wait > 0:
        time.sleep(wait)


class FrameGen(threading.Thread):
    """Takes the game's frames; shows each one after the generated frames that lead up to it."""

    def __init__(self, renderer, model=Model):
        super().__init__(daemon=True, name="ai")
        self.renderer, self._new_model = renderer, model
        self.jobs: queue.Queue = queue.Queue()
        self.size: tuple[int, int] | None = None
        self.model = None
        self.failed = False
        self.status = ""
        self.older: tuple[Planes, float] | None = None
        self.start()

    def submit(self, data, fmt, width, height, full_range, matrix, decode_ms, multiple: int, period: float) -> bool:
        """Hand over a game frame, to be shown `multiple` pictures to a frame of `period`
        seconds. False: not taken, and the caller shows it."""
        if self.failed or not self.is_alive():
            return False
        if self.size != (width, height):
            self.size, self.model = (width, height), None
            self.jobs.put(("load", width, height))
        model = self.model
        if model is None:
            return False
        if model.took > period / multiple:
            self.status = f"AI frames: {model.took * 1e3:.0f} ms each, too slow for this rate - built-in used"
            return False
        planes = planes_of(data, fmt, width, height)
        if planes is None:
            self.status = f"AI frames: {fmt} video is not handled - built-in used"
            return False
        self.status = f"AI frames: {model.took * 1e3:.0f} ms each"
        self.jobs.put((model, planes, width, height, full_range, matrix, decode_ms, multiple, period, time.perf_counter()))
        return True

    def close(self) -> None:
        self.jobs.put(None)
        self.join(timeout=2)

    def run(self) -> None:
        while (job := self.jobs.get()) is not None:
            if job[0] == "load":
                self._load(*job[1:])
            else:
                self._show(*job)

    def _load(self, width: int, height: int) -> None:
        self.status = "AI frames: preparing the model (a minute or two the first time)"
        try:
            model = self._new_model(width, height)
        except Exception as e:
            self.failed = True
            self.status = f"AI frames not available: {str(e).splitlines()[0][:120]}"
            return
        if self.size == (width, height):
            self.model, self.status = model, "AI frames: ready"

    def _show(self, model, planes, width, height, full_range, matrix, decode_ms, multiple, period, arrived) -> None:
        def push(frame: Planes, behind: float = 0.0) -> None:
            late = (time.perf_counter() - arrived + behind) * 1e3
            self.renderer.push_frame(
                frame, fmt="i420", width=width, height=height, full_range=full_range, matrix=matrix,
                decode_ms=decode_ms + late,
            )  # fmt: skip

        older, self.older = self.older, (planes, arrived)
        follows = older is not None and arrived - older[1] < 3 * period and older[0][0].shape == planes[0].shape
        if follows and self.jobs.empty():
            step, first = period / multiple, 0.0
            try:
                for i in range(1, multiple):
                    began = time.perf_counter()
                    made = model.generate(
                        older[0], planes, i / multiple, matrix or "bt709", bool(full_range), self.renderer.ai_still
                    )
                    model.took += 0.1 * (time.perf_counter() - began - model.took)
                    keep_zones(made, older[0] if 2 * i < multiple else planes, self.renderer.hud_zones)
                    if i == 1:
                        first = time.perf_counter()
                    _sleep_until(first + (i - 1) * step)
                    push(made, (multiple - i) * step)
                _sleep_until(first + (multiple - 1) * step)
            except Exception as e:
                self.failed = True
                self.status = f"AI frames stopped: {str(e).splitlines()[0][:120]}"
        push(planes)


def _check(width: int, height: int) -> int:
    """Load the model for this size, time it and score it on a picture that moves by a known amount."""
    if not available():
        print(f"not installed: nothing in {PACK}. Run scripts\\install-ai.ps1")
        return 1
    print(f"preparing the model for {width}x{height} (a minute or two the first time) ...")
    began = time.perf_counter()
    model = Model(width, height)
    print(f"ready in {time.perf_counter() - began:.0f} s")

    rng = np.random.default_rng(7)
    blocks = np.kron(rng.integers(40, 216, (height // 16 + 8, width // 16 + 8), np.uint8), np.ones((16, 16), np.uint8))
    for _ in range(3):
        blocks = ((blocks.astype(np.uint16) + np.roll(blocks, 2, 0) + np.roll(blocks, 2, 1) + np.roll(blocks, (2, 2), (0, 1))) // 4).astype(np.uint8)

    def frame(shift: int) -> Planes:
        y = blocks[40 - shift // 2 : 40 - shift // 2 + height, 40 - shift : 40 - shift + width]
        chroma = y.reshape(height // 2, 2, width // 2, 2).mean((1, 3))
        return np.ascontiguousarray(y), (144 - chroma / 8).astype(np.uint8), (112 + chroma / 8).astype(np.uint8)

    def db(a: np.ndarray, b: np.ndarray) -> float:
        return float(-10 * np.log10(np.mean((a[32:-32, 32:-32].astype(np.float32) - b[32:-32, 32:-32]) ** 2) / 255**2 + 1e-12))

    older, truth, newer = frame(0), frame(8), frame(16)
    times = []
    for _ in range(60):
        began = time.perf_counter()
        made = model.generate(older, newer, 0.5, "bt601", True)
        times.append((time.perf_counter() - began) * 1e3)
    moving = db(made[0], truth[0])
    fade = db((older[0].astype(np.float32) + newer[0]) / 2, truth[0])
    still = model.generate(older, older, 0.5, "bt601", True)
    still_y, still_c = db(still[0], older[0]), min(db(still[1], older[1]), db(still[2], older[2]))
    drawn = db(model.generate(older, older, 0.5, "bt601", True, keep_still=False)[0], older[0])
    print(f"one generated frame: {np.median(times):.1f} ms typical, {np.percentile(times, 95):.1f} ms slow case")
    print(f"moving picture against the true in-between frame: {moving:.1f} dB (plain cross-fade: {fade:.1f} dB)")
    print(f"still picture kept as it is: brightness {still_y:.1f} dB, colour {still_c:.1f} dB (120 = identical)")
    print(f"still picture with that switched off, drawn by the model: {drawn:.1f} dB")
    good = moving > fade + 10 and still_y > 100 and still_c > 100 and 38 < drawn < 100
    print("OK" if good else "FAILED: the model's output is not what it should be")
    return 0 if good else 1


if __name__ == "__main__":
    w, h = (int(n) for n in (sys.argv[1] if len(sys.argv) > 1 else "1920x1080").lower().split("x"))
    sys.exit(_check(w, h))
