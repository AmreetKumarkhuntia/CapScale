"""Follow the rhythm of new and repeated frames in a capture signal.

A console showing a 30 fps game still sends 60 pictures a second: every frame twice.
Frame generation needs each of the game's frames once, so every other captured frame
is dropped before it reaches the renderer.

The repeats are not byte-identical (the signal carries noise) and how strong that
noise is depends on the scene, so no fixed "is this a repeat" threshold holds up.
What does hold is the rhythm: on a 30 fps game the frames at one parity change and the
frames at the other do not. So the two parities are compared with each other, and the
noise, which is the same on both, cancels out.

In that rhythm it barely matters which parity is kept: every picture is in the signal
twice, so keeping one frame of each pair passes every picture on exactly once.
Picking the "new" one only saves a sixtieth of a second.
"""
from __future__ import annotations

import math
from collections import deque

import numpy as np

LEVEL = 14
MEMORY = 0.9
FLOOR = 60.0
PATIENCE = 12


def luma_of(data, fmt: str, width: int, height: int) -> np.ndarray:
    """The brightness plane of a frame, as capture or the test pattern hands it over."""
    if isinstance(data, (tuple, list)):
        return data[0]
    if fmt == "yuy2":
        return data.reshape(height, width, 2)[..., 0]
    return data[: width * height].reshape(height, width)


class Cadence:
    def __init__(self) -> None:
        self.prev: np.ndarray | None = None
        self.index = 0
        self.recent: deque[int] = deque(maxlen=32)
        self.mean = [0.0, 0.0]
        self.var = [0.0, 0.0]
        self.seen = [0, 0]
        self.halved = False
        self.keep = 0
        self._want: tuple[bool, int] | None = None
        self._streak = 0

    def step(self, luma: np.ndarray, forced: bool | None = None) -> bool:
        """Take the next captured frame. True if it should be passed on."""
        grid = np.ascontiguousarray(luma[::2, ::2])
        change = 0
        if self.prev is not None and self.prev.shape == grid.shape:
            change = int(np.count_nonzero(np.maximum(grid, self.prev) - np.minimum(grid, self.prev) > LEVEL))
        self.prev = grid
        return self.feed(change, forced)

    def feed(self, change: int, forced: bool | None = None) -> bool:
        """`change`: how many pixels differ from the frame before. `forced`: True if the
        content is known to be at half the capture rate, False if known not to be."""
        parity = self.index & 1
        self.index += 1
        self.recent.append(change)
        if self.seen[parity] == 0:
            self.mean[parity] = float(change)
        off = change - self.mean[parity]
        self.mean[parity] += (1 - MEMORY) * off
        self.var[parity] = MEMORY * (self.var[parity] + (1 - MEMORY) * off * off)
        self.seen[parity] += 1

        gap = self.mean[0] - self.mean[1]
        wander = math.sqrt((self.var[0] + self.var[1]) * (1 - MEMORY) / (1 + MEMORY)) + 1.0
        want: tuple[bool, int] | None = None
        if min(self.seen) >= 4:
            if abs(gap) > 4 * wander and abs(gap) > FLOOR:
                want = (True, 0 if gap > 0 else 1)
            elif abs(gap) < 1.5 * wander and min(self.mean) - min(self.recent) > FLOOR:
                want = (False, 0)
        if forced is False:
            want = (False, 0)
        elif forced:
            want = (True, want[1] if want and want[0] else self.keep)

        current = (self.halved, self.keep if self.halved else 0)
        if want is None or want == current:
            self._streak = 0
        else:
            self._streak = self._streak + 1 if want == self._want else 1
            self._want = want
            told = forced is not None and want[0] != self.halved
            safe = not (self.halved and want[0]) or parity == want[1]
            if (told or self._streak >= PATIENCE) and safe:
                self.halved, self.keep = want
                self._streak = 0
        return not self.halved or parity == self.keep
