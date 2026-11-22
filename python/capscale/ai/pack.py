"""The AI download (scripts/install-ai.ps1): where it is and which models it holds."""
from __future__ import annotations

import os
from pathlib import Path

from ..config import app_dir

PACK = app_dir(Path(os.environ.get("LOCALAPPDATA") or Path.home() / ".local" / "share")) / "ai"
SITE = PACK / "site"


def models() -> dict[str, Path]:
    """The installed models by name: every .onnx file in the pack."""
    return {p.stem: p for p in sorted(PACK.glob("*.onnx"))}


def model_path(name: str = "") -> Path:
    """The model of that name; "" is the first one installed."""
    found = models()
    if name in found:
        return found[name]
    if not name and found:
        return next(iter(found.values()))
    raise FileNotFoundError(f"AI model {name!r} is not installed (installed: {', '.join(found) or 'none'})")


def available() -> bool:
    return bool(models()) and (SITE / "onnxruntime").is_dir()
