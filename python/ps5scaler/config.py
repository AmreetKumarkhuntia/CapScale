"""config.toml: defaults, loading, and noticing edits."""
from __future__ import annotations

import copy
import re
import sys
import tomllib
from pathlib import Path

DEFAULTS: dict = {
    # audio_device: empty finds the sound input named after the capture device
    "capture": {"device": "", "width": 1920, "height": 1080, "fps": 60, "format": "auto", "audio_device": ""},
    # backend: graphics API; DirectX 12 on Windows is what driver features such as
    # NVIDIA Smooth Motion attach to
    "output": {
        "width": 0, "height": 0, "fullscreen": True, "vsync": "mailbox",
        "backend": "dx12" if sys.platform == "win32" else "auto",
    },
    "params": {},  # anything Renderer.set_params accepts
    "passes": {},  # name = true/false, see Renderer.toggle_pass
    "audio": {"enabled": True, "volume": 1.0, "delay_ms": 0.0},
    "hotkeys": {
        "Escape": "quit",
        "F1": "toggle:panel",
        "F2": "toggle:overlay",
        "F3": "toggle:split",
        "F4": "cycle:upscaler",
        "F5": "toggle:rcas",
        "F6": "toggle:grade",
        "F7": "toggle:lut",
        "F8": "toggle:audio",
        "F9": "pick_source",
        "F11": "toggle:fullscreen",
    },
}


def load(path: Path) -> dict:
    """Defaults overlaid with the file's sections. Raises on bad TOML or an unknown section."""
    cfg = copy.deepcopy(DEFAULTS)
    if path.exists():
        with path.open("rb") as f:
            user = tomllib.load(f)
        for section, values in user.items():
            if section not in cfg or not isinstance(values, dict):
                raise ValueError(f"unknown section [{section}] in {path}")
            cfg[section].update(values)
    return cfg


def save_sections(path: Path, text: str) -> None:
    """Replace the sections that `text` contains; every other section of the file is kept."""
    old = path.read_text(encoding="utf-8") if path.exists() else ""
    for name in re.findall(r"^\[(\w+)\]", text, re.M):
        # a section runs from its header to the next line starting with "["
        # shortcut: a hand-written multi-line array whose continuation line starts with "["
        # would end the section early; none of the settings here are written that way
        old = re.sub(rf"^\[{name}\][^\n]*\n(?:(?!\[)[^\n]*\n?)*", "", old, flags=re.M)
    old = old.strip("\n")
    path.write_text(f"{old}\n\n{text}" if old else text, encoding="utf-8")


class Watcher:
    """changed() is True once after each save of the file."""

    def __init__(self, path: Path):
        self.path = path
        self.mtime = self._mtime()

    def _mtime(self) -> int:
        try:
            return self.path.stat().st_mtime_ns
        except OSError:
            return 0

    def changed(self) -> bool:
        now = self._mtime()
        changed, self.mtime = now != self.mtime, now
        return changed
