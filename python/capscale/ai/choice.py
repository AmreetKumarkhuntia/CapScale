"""What the settings window can offer: the installed frame models and upscalers, by name."""
from __future__ import annotations

from . import nvidia, pack
from .model import Model
from .types import FrameModel


def frame_models() -> list[tuple[str, str]]:
    """(name, label) of every AI frame model that is installed: the .onnx files, then NVIDIA's modes."""
    found = [(name, name.replace("_", " ")) for name in pack.models()] if pack.available() else []
    return found + (list(nvidia.FRAMEGEN.items()) if nvidia.installed() else [])


def upscalers() -> list[tuple[str, str]]:
    """(name, label) of every AI upscaler that is installed."""
    return list(nvidia.UPSCALE.items()) if nvidia.installed() else []


def frame_model(width: int, height: int, name: str = "") -> FrameModel:
    """The model of that name, set up for one frame size. "", or a name that is not
    installed here (a config from another PC), is the first one installed."""
    names = [n for n, _ in frame_models()]
    if name not in names:
        name = names[0] if names else ""
    return nvidia.FrameGenerator(width, height, name) if name in nvidia.FRAMEGEN else Model(width, height, name)
