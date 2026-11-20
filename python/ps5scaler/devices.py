"""Capture devices and the modes they offer (DirectShow on Windows, V4L2 on Linux)."""
from __future__ import annotations

import re
import sys
from typing import NamedTuple

import av
import av.logging

BACKEND = "dshow" if sys.platform == "win32" else "v4l2"
PREFERRED = ("nv12", "yuyv422", "mjpeg")


class Mode(NamedTuple):
    fmt: str
    width: int
    height: int
    max_fps: float


def list_devices() -> list[tuple[str, str]]:
    """(name, label) per video device: name opens it, label is what a person reads."""
    devs = av.enumerate_input_devices(BACKEND)
    if BACKEND == "dshow":
        return [(d.description, d.description) for d in devs if "video" in d.media_types]
    return [(d.name, f"{d.description} ({d.name})") for d in devs]


def open_url(device: str) -> str:
    return f"video={device}" if BACKEND == "dshow" else device


def _log_of(url: str, options: dict[str, str]) -> str:
    """ffmpeg prints device capabilities to its log instead of returning them; capture that text."""
    old = av.logging.get_level()
    av.logging.set_level(av.logging.INFO)
    try:
        with av.logging.Capture() as logs:
            try:
                av.open(url, format=BACKEND, options=options).close()
            except av.FFmpegError:
                pass
        return "".join(str(entry[-1]) for entry in logs)
    finally:
        av.logging.set_level(old)


_DSHOW = re.compile(
    r"(?:vcodec|pixel_format)=(\S+)\s+min s=\d+x\d+ fps=[\d.]+ max s=(\d+)x(\d+) fps=([\d.]+)"
)
_V4L2 = re.compile(r"(?:Raw|Compressed)\s*:\s*(\S+)\s*:.*:((?:\s*\d+x\d+)+)\s*$", re.M)


def parse_dshow_modes(text: str) -> list[Mode]:
    fastest: dict[tuple[str, int, int], float] = {}
    for fmt, w, h, fps in _DSHOW.findall(text):
        key = (fmt, int(w), int(h))
        fastest[key] = max(fastest.get(key, 0.0), float(fps))
    modes = [Mode(*key, fps) for key, fps in fastest.items()]
    return sorted(modes, key=lambda m: (-m.width * m.height, m.fmt))


def parse_v4l2_modes(text: str) -> list[Mode]:
    modes = {
        Mode(fmt, int(w), int(h), 60.0)
        for fmt, sizes in _V4L2.findall(text)
        for w, h in re.findall(r"(\d+)x(\d+)", sizes)
    }
    return sorted(modes, key=lambda m: (-m.width * m.height, m.fmt))


def list_modes(device: str) -> list[Mode]:
    """Every mode the device offers in a format the renderer can take."""
    if BACKEND == "dshow":
        modes = parse_dshow_modes(_log_of(open_url(device), {"list_options": "true"}))
    else:
        modes = parse_v4l2_modes(_log_of(open_url(device), {"list_formats": "all"}))
    return [m for m in modes if m.fmt in PREFERRED]


def best_mode(modes: list[Mode], width: int, height: int, fps: float, fmt: str = "auto") -> Mode | None:
    """The mode to request: raw if the card can do it at this size and rate, else MJPEG."""
    fits = [
        m
        for m in modes
        if (m.width, m.height) == (width, height)
        and m.max_fps >= fps - 0.5
        and fmt in ("auto", m.fmt)
    ]
    return min(fits, key=lambda m: PREFERRED.index(m.fmt), default=None)
