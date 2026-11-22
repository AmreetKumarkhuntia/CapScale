"""NVIDIA's own video models (the nvidia-vfx package): frame generation and super resolution.

Optional and for NVIDIA RTX cards only: install-ai.ps1 puts nvidia-vfx and CuPy in the
pack, next to ONNX Runtime. A frame goes to the card as its 8-bit planes, is turned into
RGB there, and comes back as planes, so the app only copies bytes in and out.
"""
from __future__ import annotations

import functools
import os
import time

import numpy as np

from . import engines
from .pack import SITE
from .types import Planes

# Name -> what the settings window shows. The end of a name is NVIDIA's own name for the level.
FRAMEGEN = {"nvidia-low": "NVIDIA Low", "nvidia-medium": "NVIDIA Medium", "nvidia-high": "NVIDIA High"}
UPSCALE = {
    "nvidia-low": "Low",
    "nvidia-medium": "Medium",
    "nvidia-high": "High",
    "nvidia-ultra": "Ultra",
    "nvidia-highbitrate-high": "High-bitrate High",
    "nvidia-highbitrate-ultra": "High-bitrate Ultra",
    "nvidia-streaming-ultra": "Streaming Ultra",
}
REST, MOVING = 3.0, 6.0  # brightness change at which a region counts as at rest / moving, as in build_ai_model.py


def installed() -> bool:
    return (SITE / "nvvfx").is_dir() and (SITE / "cupy").is_dir()


def _level(name: str) -> str:
    return name.removeprefix("nvidia-").replace("-", "_").upper()


@functools.cache
def _runtime():
    """CuPy and nvidia-vfx from the pack."""
    engines.runtime()  # puts the pack on the path and NVIDIA's libraries where Windows finds them
    # CuPy compiles its kernels against CUDA's headers, and only looks for the pack's own when told
    os.environ["CUDA_PATH"] = str(next(SITE.glob("nvidia/cu*"), ""))
    import cupy
    import nvvfx

    return cupy, nvvfx


@functools.cache
def _kernels():
    """Planes to RGB and back, each one pass on the card."""
    cp, _ = _runtime()
    rgb = cp.ElementwiseKernel(
        "raw uint8 y, raw uint8 u, raw uint8 v, raw float32 m, int32 w",
        "raw float32 rgb",
        """
        int at = (i / w / 2) * (w / 2) + i % w / 2;
        float p[3] = {y[i] / 255.0f, u[at] / 255.0f, v[at] / 255.0f};
        for (int k = 0; k < 3; k++) {
            float c = m[4 * k] * p[0] + m[4 * k + 1] * p[1] + m[4 * k + 2] * p[2] + m[4 * k + 3];
            rgb[k * _ind.size() + i] = min(max(c, 0.0f), 1.0f);
        }
        """,
        "capscale_rgb",
    )
    luma = cp.ElementwiseKernel(
        "raw float32 rgb, raw float32 m",
        "raw uint8 y",
        """
        int n = _ind.size();
        float c = m[0] * rgb[i] + m[1] * rgb[n + i] + m[2] * rgb[2 * n + i] + m[3];
        y[i] = (unsigned char)(min(max(c, 0.0f), 1.0f) * 255.0f + 0.5f);
        """,
        "capscale_luma",
    )
    chroma = cp.ElementwiseKernel(
        "raw float32 rgb, raw float32 m, int32 w",
        "raw uint8 u, raw uint8 v",
        """
        int n = _ind.size() * 4, at = (i / (w / 2)) * 2 * w + i % (w / 2) * 2;
        float c[3];
        for (int k = 0; k < 3; k++) {
            const int o = k * n + at;
            c[k] = (rgb[o] + rgb[o + 1] + rgb[o + w] + rgb[o + w + 1]) * 0.25f;
        }
        float cb = m[4] * c[0] + m[5] * c[1] + m[6] * c[2] + m[7];
        float cr = m[8] * c[0] + m[9] * c[1] + m[10] * c[2] + m[11];
        u[i] = (unsigned char)(min(max(cb, 0.0f), 1.0f) * 255.0f + 0.5f);
        v[i] = (unsigned char)(min(max(cr, 0.0f), 1.0f) * 255.0f + 0.5f);
        """,
        "capscale_chroma",
    )
    return rgb, luma, chroma


@functools.cache
def _matrices(matrix: str):
    """The renderer's YUV to RGB rows and their reverse, on the card. Always the full-range
    ones: whatever range the planes are in, nothing is clipped on the way to RGB, and the
    reverse puts every value back where it was."""
    from .._core import yuv_matrix

    cp, _ = _runtime()
    rows = np.array(yuv_matrix(matrix, True), np.float64)
    back = np.linalg.inv(rows[:, :3])
    back = np.hstack([back, -back @ rows[:, 3:]])
    return cp.asarray(rows.ravel(), cp.float32), cp.asarray(back.ravel(), cp.float32)


def _rgb(planes, matrix: str):
    """Planes on the card -> the (3, h, w) picture of floats the models take."""
    cp, _ = _runtime()
    h, w = planes[0].shape
    out = cp.empty((3, h, w), cp.float32)
    _kernels()[0](*planes, _matrices(matrix)[0], w, out, size=h * w)
    return out


def _planes(result, matrix: str):
    """A model's result -> planes on the card."""
    cp, _ = _runtime()
    rgb = cp.ascontiguousarray(cp.from_dlpack(result.image))
    _, h, w = rgb.shape
    y, u, v = cp.empty((h, w), cp.uint8), cp.empty((h // 2, w // 2), cp.uint8), cp.empty((h // 2, w // 2), cp.uint8)
    back = _matrices(matrix)[1]
    _kernels()[1](rgb, back, y, size=h * w)
    _kernels()[2](rgb, back, w, u, v, size=h * w // 4)
    return y, u, v


def _settle(made, older, newer, when: float):
    """Where the picture is at rest the real frame is kept, as build_ai_model.py does for RIFE."""
    cp, _ = _runtime()
    from cupyx.scipy import ndimage

    h, w = older[0].shape
    change = cp.abs(older[0].astype(cp.float32) - newer[0])
    cells = change[: h // 8 * 8, : w // 8 * 8].reshape(h // 8, 8, w // 8, 8).mean((1, 3))
    moving = cp.clip((ndimage.maximum_filter(cells, 11) - REST) / (MOVING - REST), 0, 1)
    settled = []
    for plane, real in zip(made, older if when < 0.5 else newer):
        gate = ndimage.zoom(moving, [a / b for a, b in zip(plane.shape, moving.shape)], order=1, mode="nearest")
        settled.append(cp.rint(real + (plane.astype(cp.float32) - real) * gate).astype(cp.uint8))
    return settled


def _blank(width: int, height: int) -> Planes:
    return np.zeros((height, width), np.uint8), *[np.full((height // 2, width // 2), 128, np.uint8)] * 2


def _timed(run) -> float:
    """Seconds one call takes once it is warm. The best of a few: another model loading at
    the same moment must not make this one look slow, and use corrects the figure anyway."""
    took = []
    for _ in range(8):
        began = time.perf_counter()
        run()
        took.append(time.perf_counter() - began)
    return min(took[3:])


class FrameGenerator:
    """One of NVIDIA's frame generation modes, set up for one frame size. A FrameModel."""

    def __init__(self, width: int, height: int, name: str):
        kind = _runtime()[1].VideoFrameGeneration
        self.effect = kind(kind.Mode[_level(name)])
        self.effect.input_width, self.effect.input_height = width, height
        self.effect.load()
        blank = _blank(width, height)
        self.took = _timed(lambda: self.generate(blank, blank, 0.5))

    def generate(
        self, older: Planes, newer: Planes, when: float, matrix: str = "bt709", full_range: bool = False, keep_still: bool = True
    ) -> Planes:  # fmt: skip
        """The picture `when` (0..1) of the way from `older` to `newer`. `keep_still`:
        where the two frames do not differ, it is the real picture, untouched."""
        cp, _ = _runtime()
        older, newer = (tuple(cp.asarray(p) for p in frame) for frame in (older, newer))
        made = _planes(self.effect.run_at_timestep(_rgb(older, matrix), _rgb(newer, matrix), float(when)), matrix)
        if keep_still:
            made = _settle(made, older, newer, when)
        return tuple(cp.asnumpy(p) for p in made)


class Upscaler:
    """One level of NVIDIA's Video Super Resolution, from one frame size to another."""

    def __init__(self, size: tuple[int, int], out: tuple[int, int], name: str):
        kind = _runtime()[1].VideoSuperRes
        self.effect = kind(quality=kind.QualityLevel[_level(name)])
        self.effect.output_width, self.effect.output_height = out
        self.effect.load()
        blank = _blank(*size)
        self.took = _timed(lambda: self.enlarge(blank))

    def enlarge(self, planes: Planes, matrix: str = "bt709") -> Planes:
        cp, _ = _runtime()
        made = _planes(self.effect.run(_rgb(tuple(cp.asarray(p) for p in planes), matrix)), matrix)
        return tuple(cp.asnumpy(p) for p in made)
