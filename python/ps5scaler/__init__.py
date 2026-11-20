"""Real-time capture-card upscaler. The GPU side lives in the Rust extension `_core`."""
from ._core import Renderer

__all__ = ["Renderer"]
