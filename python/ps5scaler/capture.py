"""Open a capture device and yield frames in the shape Renderer.push_frame takes."""
from __future__ import annotations

import time
from typing import Iterator

import av
import numpy as np

from .devices import BACKEND, open_url

# raw packets from the card: handed to the GPU without decoding
RAW = {"nv12": "nv12", "yuyv422": "yuy2"}
# what the MJPEG decoder produces
PLANAR = {
    "yuv420p": "i420", "yuvj420p": "i420",
    "yuv422p": "i422", "yuvj422p": "i422",
    "yuv444p": "i444", "yuvj444p": "i444",
}
_BT709, _BT601 = (1,), (4, 5, 6)  # AVColorSpace values
_FULL_RANGE = 2  # AVCOL_RANGE_JPEG

Frame = tuple[object, str, int, int, "bool | None", "str | None", float]


class Capture:
    def __init__(self, device: str, width: int, height: int, fps: float, fmt: str = "mjpeg"):
        dshow = BACKEND == "dshow"
        options = {
            "video_size": f"{width}x{height}",
            "framerate": f"{fps:g}",
            # start fast and never let ffmpeg hold frames back
            "fflags": "nobuffer",
            "probesize": "32",
            "analyzeduration": "0",
        }
        if dshow:
            options["rtbufsize"] = "64M"
            options["vcodec" if fmt == "mjpeg" else "pixel_format"] = fmt
        else:
            options["input_format"] = fmt
        self.container = av.open(open_url(device), format=BACKEND, options=options)
        self.stream = self.container.streams.video[0]
        # frame threading would hold back one frame per thread
        self.stream.codec_context.thread_type = "SLICE"
        self.dropped = 0

    def close(self) -> None:
        self.container.close()

    def frames(self) -> Iterator[Frame]:
        """Yield (data, fmt, width, height, full_range, matrix, decode_ms).

        `data` is one packed array (nv12, yuy2) or a tuple of per-plane arrays. It
        views the decoder's memory, so use it before asking for the next frame.
        """
        ctx = self.stream.codec_context
        raw_fmt = RAW.get(ctx.pix_fmt) if ctx.name == "rawvideo" else None
        backlog = 0
        demux = self.container.demux(self.stream)
        while True:
            asked = time.perf_counter()
            packet = next(demux, None)
            if packet is None:
                return
            got = time.perf_counter()
            if packet.size == 0:
                continue
            # An instant answer means frames piled up behind us. Every packet here is a
            # keyframe, so skip ahead to the newest instead of showing a late one.
            if got - asked < 0.001 and backlog < 4:
                backlog += 1
                self.dropped += 1
                continue
            backlog = 0

            if raw_fmt:
                data = np.frombuffer(packet, np.uint8)
                yield data, raw_fmt, ctx.width, ctx.height, None, None, 0.0
                continue

            for frame in packet.decode():
                fmt = PLANAR.get(frame.format.name)
                if fmt is None:
                    raise RuntimeError(f"unsupported pixel format from decoder: {frame.format.name}")
                planes = tuple(
                    np.frombuffer(p, np.uint8).reshape(p.height, p.line_size) for p in frame.planes
                )
                space, rng = int(frame.colorspace), int(frame.color_range)
                matrix = "bt709" if space in _BT709 else "bt601" if space in _BT601 else None
                full = True if rng == _FULL_RANGE or frame.format.name.startswith("yuvj") else None
                decode_ms = (time.perf_counter() - got) * 1e3
                yield planes, fmt, frame.width, frame.height, full, matrix, decode_ms
