"""AI frame generation: an open-source model (RIFE) on the GPU, through ONNX Runtime.

Optional. The runtime is a large download, so it is not part of the app:
scripts/install-ai.ps1 puts it and the model in PACK. Without it the app keeps to its
built-in generation (the flow and interp shaders).

A game frame comes in; the frames between it and the one before are generated and
shown first, each in its time slot, then the frame itself. So a real frame is late by
the time one generated frame takes, plus the slots before it.

Two things plug in: an engine (engines.py: TensorRT on NVIDIA RTX, DirectML on any
other card) and a model (pack.py lists the installed ones, model.py runs one).

    python -m capscale.ai [WIDTHxHEIGHT] [MODEL]     load the model, time it, check its output
"""
from .check import _check
from .framegen import FrameGen, keep_zones, planes_of
from .model import Model
from .pack import PACK, available

__all__ = ["PACK", "FrameGen", "Model", "_check", "available", "keep_zones", "planes_of"]
