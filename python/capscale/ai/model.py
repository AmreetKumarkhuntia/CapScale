"""A model on the graphics card, taking and returning the capture card's YUV planes."""
from __future__ import annotations

import functools
import time

import numpy as np

from . import engines
from .pack import model_path
from .types import Planes


@functools.cache
def _colour(matrix: str, full_range: bool) -> dict[str, np.ndarray]:
    """YUV to RGB and back, in the form the model takes: pixel * matrix + offset."""
    from .._core import yuv_matrix

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
    """An installed model, set up for one frame size. `name`: one of pack.models(), "" for
    the first. Any ONNX file works that has the inputs and outputs build_ai_model.py
    gives RIFE: y0 u0 v0 y1 u1 v1 (8-bit planes), t, still and the colour matrices in;
    y and uv out."""

    def __init__(self, width: int, height: int, name: str = ""):
        sizes = {"h": height, "w": width, "h2": height // 2, "w2": width // 2}
        self.session = engines.session(model_path(name), sizes)
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
