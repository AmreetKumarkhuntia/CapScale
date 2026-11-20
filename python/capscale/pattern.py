"""Synthetic colour bars, for checking the pipeline without a capture card."""
from __future__ import annotations

import time

import numpy as np

FORMATS = ("nv12", "yuy2", "i420")
BARS = [(1, 1, 1), (1, 1, 0), (0, 1, 1), (0, 1, 0), (1, 0, 1), (1, 0, 0), (0, 0, 1), (0, 0, 0)]


class Pattern:
    """Same interface as capture.Capture."""

    def __init__(
        self, fmt: str, width: int = 1920, height: int = 1080, fps: float = 60, content_fps: float | None = None
    ):
        if fmt not in FORMATS:
            raise ValueError(f"test pattern format must be one of {FORMATS}")
        self.fmt, self.width, self.height, self.fps = fmt, width, height, fps
        self.hold = max(1, round(fps / (content_fps or fps)))

    def close(self) -> None:
        pass

    def frames(self):
        w, h, fmt = self.width, self.height, self.fmt
        rgb = np.zeros((h, w, 3), np.float32)
        top = h * 2 // 3
        for i, colour in enumerate(BARS):
            rgb[:top, i * w // 8 : (i + 1) * w // 8] = colour
        rgb[top:] = np.linspace(0, 1, w, dtype=np.float32)[None, :, None]

        r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
        y = 0.2126 * r + 0.7152 * g + 0.0722 * b
        luma = np.round(16 + 219 * y).astype(np.uint8)
        cb = np.round(128 + 224 * (b - y) / 1.8556).astype(np.uint8)
        cr = np.round(128 + 224 * (r - y) / 1.5748).astype(np.uint8)
        cb420, cr420 = np.ascontiguousarray(cb[0::2, 0::2]), np.ascontiguousarray(cr[0::2, 0::2])
        chroma_nv12 = np.stack([cb420, cr420], axis=-1).ravel()
        yuy2 = np.empty((h, w, 2), np.uint8)
        yuy2[:, 0::2, 1] = cb[:, 0::2]
        yuy2[:, 1::2, 1] = cr[:, 0::2]

        frame, deadline = 0, time.perf_counter()
        while True:
            x = (frame // self.hold * self.hold * 8) % (w - 8)
            moving = luma.copy()
            moving[:, x : x + 8] = 235
            if fmt == "nv12":
                data = np.concatenate([moving.ravel(), chroma_nv12])
            elif fmt == "yuy2":
                yuy2[..., 0] = moving
                data = yuy2.ravel()
            else:
                data = (moving, cb420, cr420)
            yield data, fmt, w, h, False, "bt709", 0.0
            frame += 1
            deadline += 1 / self.fps
            time.sleep(max(0.0, deadline - time.perf_counter()))
