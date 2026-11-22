"""AI frame generation and AI upscaling on the GPU.

Optional. The runtime is a large download, so it is not part of the app:
scripts/install-ai.ps1 puts it and the model in PACK. Without it the app keeps to its
built-in generation (the flow and interp shaders) and its own upscalers.

A game frame comes in; the frames between it and the one before are generated and
shown first, each in its time slot, then the frame itself. So a real frame is late by
the time one generated frame takes, plus the slots before it.

Two things plug in: an engine (engines.py: TensorRT on NVIDIA RTX, DirectML on any
other card) and a model (pack.py lists the installed ones, model.py runs one). On an
NVIDIA RTX card there are also NVIDIA's own models (nvidia.py): three more frame models,
and an upscaler that enlarges each frame before anything else is done with it (upscale.py).
choice.py lists what is installed, for the settings window.

    python -m capscale.ai [WIDTHxHEIGHT] [MODEL]     load the model, time it, check its output
"""
from .check import _check
from .choice import frame_model, frame_models, upscalers
from .framegen import FrameGen, keep_zones, planes_of
from .model import Model
from .pack import PACK, available
from .upscale import Upscale

__all__ = [
    "PACK", "FrameGen", "Model", "Upscale", "_check", "available", "frame_model", "frame_models", "keep_zones",
    "planes_of", "upscalers",
]  # fmt: skip
