"""Game audio: the capture card's sound input played on the default output, in step with the picture."""
from __future__ import annotations

import sys

import numpy as np
import sounddevice as sd

RATE = 48000
CHANNELS = 2
MAX_DELAY_MS = 500


def match_input(names: list[str], needle: str) -> int | None:
    """Index of the first name containing `needle`, ignoring case.

    A capture card's sound input is named after the card, e.g. video
    "Display capture-UVC03" and audio "Microphone (Display capture-UVC03)".
    """
    needle = needle.strip().lower()
    return next((i for i, name in enumerate(names) if needle and needle in name.lower()), None)


def find_devices(video_device: str, wanted: str = "") -> tuple[int, int] | None:
    """(input, output) device numbers: the card's sound input and the default output."""
    apis = sd.query_hostapis()
    api = next((i for i, a in enumerate(apis) if "WASAPI" in a["name"]), sd.default.hostapi)
    inputs = [(i, d["name"]) for i, d in enumerate(sd.query_devices()) if d["hostapi"] == api and d["max_input_channels"]]
    hit = match_input([name for _, name in inputs], wanted or video_device)
    output = apis[api]["default_output_device"]
    return None if hit is None or output < 0 else (inputs[hit][0], output)


class DelayLine:
    """Holds audio back by a set number of samples, for when the picture is the slower of the two."""

    def __init__(self, channels: int, max_samples: int):
        self.buf = np.zeros((max_samples, channels), np.float32)
        self.pos = 0

    def process(self, block: np.ndarray, delay: int) -> np.ndarray:
        size = len(self.buf)
        at = (self.pos + np.arange(len(block))) % size
        self.buf[at] = block
        self.pos = (self.pos + len(block)) % size
        return self.buf[(at - delay) % size]


class Audio:
    """One duplex stream: what the card hears goes straight to the speakers."""

    def __init__(self, video_device: str, device: str = ""):
        found = find_devices(video_device, device)
        if found is None:
            raise RuntimeError(
                f"no sound input matching {device or video_device!r}; set device in [audio] of config.toml"
            )
        self.volume, self.delay_ms, self.muted = 1.0, 0.0, False
        self._delay = DelayLine(CHANNELS, RATE * MAX_DELAY_MS // 1000 + RATE // 4)
        self.stream = sd.Stream(
            device=found,
            samplerate=RATE,
            channels=CHANNELS,
            dtype="float32",
            latency="low",
            extra_settings=sd.WasapiSettings(auto_convert=True) if sys.platform == "win32" else None,
            callback=self._callback,
        )
        self.stream.start()

    def _callback(self, indata, outdata, frames, time, status) -> None:
        delay = int(min(max(self.delay_ms, 0.0), MAX_DELAY_MS) * RATE / 1000)
        outdata[:] = self._delay.process(indata, delay) * (0.0 if self.muted else self.volume)

    @property
    def latency_ms(self) -> float:
        """Time from the card's input to the speakers, including the chosen delay."""
        return sum(self.stream.latency) * 1e3 + self.delay_ms

    def close(self) -> None:
        self.stream.close()
