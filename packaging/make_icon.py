"""Draws packaging/icon.ico: a rounded tile with an arrow pointing up and right ("scale up").

Run with the project's Python; needs only numpy. The .ico is committed, so this is
only for changing the design.
"""
import struct
import zlib
from pathlib import Path

import numpy as np

SIZES = (256, 64, 48, 32, 16)


def segment(px, py, ax, ay, bx, by):
    """Distance from each pixel to the line segment a-b."""
    dx, dy = bx - ax, by - ay
    t = np.clip(((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy), 0, 1)
    return np.hypot(px - (ax + t * dx), py - (ay + t * dy))


def draw(size: int) -> np.ndarray:
    """RGBA image, drawn 4x oversize and averaged down for smooth edges."""
    n = size * 4
    y, x = np.mgrid[0:n, 0:n] / n  # 0..1
    # rounded square
    r = 0.22
    qx, qy = np.abs(x - 0.5) - (0.5 - r), np.abs(y - 0.5) - (0.5 - r)
    tile = np.hypot(np.maximum(qx, 0), np.maximum(qy, 0)) + np.minimum(np.maximum(qx, qy), 0) - r <= -0.02
    # arrow: shaft from lower left to upper right, plus the two head strokes
    arrow = np.minimum.reduce([
        segment(x, y, 0.30, 0.70, 0.70, 0.30),
        segment(x, y, 0.70, 0.30, 0.42, 0.30),
        segment(x, y, 0.70, 0.30, 0.70, 0.58),
    ]) <= 0.055
    blend = ((x + (1 - y)) / 2)[..., None]  # 0 lower left .. 1 upper right
    colour = (1 - blend) * np.array([36, 99, 235]) + blend * np.array([147, 51, 234])
    rgba = np.zeros((n, n, 4))
    rgba[..., :3] = np.where(arrow[..., None], 255, colour)
    rgba[..., 3] = tile * 255
    small = rgba.reshape(size, 4, size, 4, 4).mean(axis=(1, 3))
    return np.round(small).astype(np.uint8)


def png(rgba: np.ndarray) -> bytes:
    h, w, _ = rgba.shape
    raw = b"".join(b"\x00" + rgba[row].tobytes() for row in range(h))

    def chunk(tag: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))

    header = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def dib(rgba: np.ndarray) -> bytes:
    """Classic icon bitmap: header with doubled height, bottom-up BGRA rows, then an
    (unused, all zero) 1-bit mask. Older tools only read PNG at 256 px."""
    h, w, _ = rgba.shape
    header = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    mask = bytes(((w + 31) // 32) * 4 * h)
    return header + rgba[::-1, :, [2, 1, 0, 3]].tobytes() + mask


def main() -> None:
    images = [png(draw(s)) if s == 256 else dib(draw(s)) for s in SIZES]
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data in zip(SIZES, images):
        # width/height bytes: 0 means 256
        out += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    path = Path(__file__).with_name("icon.ico")
    path.write_bytes(out + b"".join(images))
    print(f"wrote {path} ({path.stat().st_size} bytes, sizes {SIZES})")


if __name__ == "__main__":
    main()
