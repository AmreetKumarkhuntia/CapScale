"""The AI upscale stage: a frame is enlarged before anything else is done with it."""
from __future__ import annotations

import queue
import threading
import time

from . import nvidia
from .framegen import planes_of

BEHIND = 2  # frames waiting behind the one being enlarged at which the next one is left out: it would be shown that late


class Upscale:
    """Takes each frame the capture thread means to show. With an AI upscaler chosen (the
    renderer's `ai_upscale`), the frame is enlarged to the size the renderer draws at, on
    a thread of its own, and handed on in the order it came; frame generation and the
    renderer then work on the larger frame. A frame it cannot take is handed on as it is.
    The chosen upscaler is used whatever it takes: where it has fallen behind (it is too
    slow for the rate, or capture is catching up after a stall), a frame that would be
    shown late is left out, and the note in the stats says so."""

    def __init__(self, renderer, scaler=nvidia.Upscaler):
        self.renderer, self._new_scaler = renderer, scaler
        self.jobs: queue.Queue = queue.Queue()
        self.key = self.seen = self.scaler = None
        self.status = ""
        self.skipped = -9.0
        self.thread = threading.Thread(target=self._run, daemon=True, name="upscale")
        self.thread.start()

    def submit(self, show, frame, fmt, width, height, full_range=None, matrix=None, decode_ms=0.0) -> None:
        """Take a frame; `show` is called with it, enlarged or not, and the same things after it."""
        now = time.perf_counter()
        want, (out_w, out_h) = self.renderer.ai_upscale, self.renderer.output_size
        key = (want, (width, height), (out_w & ~1, out_h & ~1))
        # a size is taken up once it is seen twice running: a window being dragged is not followed step by step
        if key != self.key and key == self.seen:
            self.key, self.scaler, self.status = key, None, ""
            if want and out_w > width and out_h > height:
                self.jobs.put(("load", key))
            elif want:
                self.status = "AI upscale: the picture is not being enlarged at this size"
        self.seen = key
        scaler = self.scaler if key == self.key and self.thread.is_alive() else None
        if scaler and self.jobs.qsize() >= BEHIND:  # it has fallen behind: left out, not shown late
            self.skipped = now
            return
        planes = planes_of(frame, fmt, width, height) if scaler else None
        if scaler and planes is None:
            self.status = f"AI upscale: {fmt} video is not handled"
        if planes is None:
            self.jobs.join()  # the frames still being enlarged go first
            show(frame, fmt, width, height, full_range, matrix, decode_ms)
            return
        late = ", not keeping up: frames are left out" if now - self.skipped < 2.0 else ""
        self.status = f"AI upscale: {scaler.took * 1e3:.0f} ms a frame{late}"
        self.jobs.put((show, scaler, planes, height, full_range, matrix, decode_ms, now))

    def close(self) -> None:
        self.jobs.put(None)
        self.thread.join(timeout=2)

    def _run(self) -> None:
        while (job := self.jobs.get()) is not None:
            if job[0] == "load":
                self.jobs.task_done()  # submit waits for frames, not for a model that is loading
                self._load(job[1])
                continue
            try:
                self._enlarge(*job)
            except Exception as e:  # from `show`: the window has closed
                self.status = f"AI upscale stopped: {str(e).splitlines()[0][:120]}"
            finally:
                self.jobs.task_done()

    def _load(self, key) -> None:
        self.status = "AI upscale: preparing the model"
        try:
            scaler = self._new_scaler(key[1], key[2], key[0])
        except Exception as e:
            self.status = f"AI upscale not available: {str(e).splitlines()[0][:120]}"
            return
        if key == self.key:
            self.scaler, self.status = scaler, "AI upscale: ready"

    def _enlarge(self, show, scaler, planes, height, full_range, matrix, decode_ms, arrived) -> None:
        # said outright: left to the renderer, the larger frame's size would decide it
        matrix = matrix or ("bt709" if height >= 720 else "bt601")
        began = time.perf_counter()
        try:
            planes = scaler.enlarge(planes, matrix)
            scaler.took += 0.1 * (time.perf_counter() - began - scaler.took)
        except Exception as e:
            if scaler is self.scaler:
                self.scaler = None
            self.status = f"AI upscale stopped: {str(e).splitlines()[0][:120]}"
        h, w = planes[0].shape
        show(planes, "i420", w, h, bool(full_range), matrix, decode_ms + (time.perf_counter() - arrived) * 1e3)
