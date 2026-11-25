"""Shapes shared by the AI modules."""
from __future__ import annotations

from typing import Callable, NamedTuple, Protocol

import numpy as np

Planes = tuple[np.ndarray, np.ndarray, np.ndarray]


class FrameModel(Protocol):
    """What FrameGen needs of a model. FrameGen is given the class (or any function)
    that makes one from (width, height), so another kind of model plugs in there."""

    took: float  # seconds per generated frame; FrameGen keeps it current

    def generate(
        self, older: Planes, newer: Planes, when: float, matrix: str, full_range: bool, keep_still: bool
    ) -> Planes: ...  # fmt: skip


class Engine(NamedTuple):
    """One way of running a model on the graphics card: an ONNX Runtime provider."""

    provider: str
    options: Callable[[str], dict]  # given a name for the model at one frame size
    also: tuple[str, ...] = ()  # providers behind it, for the operations it does not take
    prepare: Callable[[object], None] = lambda session_options: None
