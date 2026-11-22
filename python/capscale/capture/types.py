"""Shapes shared by the capture modules."""
from __future__ import annotations

from typing import NamedTuple


class Mode(NamedTuple):
    fmt: str
    width: int
    height: int
    max_fps: float


Frame = tuple[object, str, int, int, "bool | None", "str | None", float]
