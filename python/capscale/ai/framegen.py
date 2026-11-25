"""The AI thread: takes the game's frames and shows the generated ones in between."""
from __future__ import annotations

import queue
import threading
import time

import numpy as np

from .choice import frame_model
from .types import Planes


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
    """Takes the game's frames; shows each one after the generated frames that lead up to it.
    The model is the one chosen in the settings window (the renderer's `ai_model`), and
    it is used whatever it takes: one too slow for the rate leaves out the generated
    frames it has no time for (see `_show`), and the note in the stats says so. It starts
    afresh when the choice or the frame size changes."""

    def __init__(self, renderer, model=frame_model):
        super().__init__(daemon=True, name="ai")
        self.renderer, self._new_model = renderer, model
        self.jobs: queue.Queue = queue.Queue()
        self.key: tuple | None = None
        self.model = None
        self.failed = False
        self.status = ""
        self.older: tuple[Planes, float] | None = None
        self.start()

    def submit(self, data, fmt, width, height, full_range, matrix, decode_ms, multiple: int, period: float) -> bool:
        """Hand over a game frame, to be shown `multiple` pictures to a frame of `period`
        seconds. False: not taken, and the caller shows it."""
        if not self.is_alive():
            return False
        key = (width, height, self.renderer.ai_model)
        if self.key != key:
            self.key, self.model, self.failed = key, None, False
            self.jobs.put(("load", key))
        model = self.model
        if model is None or self.failed:
            return False
        planes = planes_of(data, fmt, width, height)
        if planes is None:
            self.status = f"AI frames: {fmt} video is not handled - built-in used"
            return False
        slow = ", too slow for this rate: some are left out" if model.took > period / multiple else ""
        self.status = f"AI frames: {model.took * 1e3:.0f} ms each{slow}"
        self.jobs.put((model, planes, width, height, full_range, matrix, decode_ms, multiple, period, time.perf_counter()))
        return True

    def close(self) -> None:
        self.jobs.put(None)
        self.join(timeout=2)

    def run(self) -> None:
        while (job := self.jobs.get()) is not None:
            if job[0] == "load":
                self._load(job[1])
            else:
                self._show(*job)

    def _load(self, key) -> None:
        width, height, name = key
        self.status = "AI frames: preparing the model (a minute or two the first time)"
        try:
            model = self._new_model(width, height, name)
        except Exception as e:
            if self.key == key:
                self.failed = True
                self.status = f"AI frames not available: {str(e).splitlines()[0][:120]}"
            return
        if self.key == key:
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
