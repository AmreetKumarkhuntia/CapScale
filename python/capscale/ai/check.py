"""Load the real model, time it and score its output."""
from __future__ import annotations

import time

import numpy as np

from .model import Model
from .pack import PACK, available
from .types import Planes


def _check(width: int, height: int, name: str = "") -> int:
    """Load the model for this size, time it and score it on a picture that moves by a known amount."""
    if not available():
        print(f"not installed: nothing in {PACK}. Run scripts\\install-ai.ps1")
        return 1
    print(f"preparing the model for {width}x{height} (a minute or two the first time) ...")
    began = time.perf_counter()
    model = Model(width, height, name)
    print(f"ready in {time.perf_counter() - began:.0f} s, running on {model.session.get_providers()[0]}")

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
