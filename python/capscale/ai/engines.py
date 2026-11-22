"""The engines: the ways ONNX Runtime can run a model on the graphics card.

Which one is used follows from the runtime that install-ai.ps1 put in the pack. To
add one: an entry in ENGINES here, and its packages in that script's $runtimes.
"""
from __future__ import annotations

import functools
import os
import sys

from .pack import PACK, SITE
from .types import Engine


def _tensorrt() -> dict:
    return {"trt_fp16_enable": True, "trt_engine_cache_enable": True, "trt_engine_cache_path": str(PACK / "engines")}


def _no_mem_pattern(session_options) -> None:
    session_options.enable_mem_pattern = False  # DirectML does not start with it on


# In order of preference: the first one the installed runtime offers is used.
ENGINES = (
    Engine("TensorrtExecutionProvider", _tensorrt, also=("CUDAExecutionProvider",)),  # NVIDIA RTX
    # any DirectX 12 card; high_performance: the fast card of a laptop that has two
    Engine("DmlExecutionProvider", lambda: {"performance_preference": "high_performance"}, prepare=_no_mem_pattern),
)


@functools.cache
def runtime():
    """ONNX Runtime from the pack."""
    sys.path.append(str(SITE))
    # NVIDIA's libraries sit in folders of their own; the DirectML runtime has none
    for folder in {p.parent for p in SITE.glob("nvidia/**/*.dll")} | set(SITE.glob("tensorrt_libs")):
        os.add_dll_directory(str(folder))
        os.environ["PATH"] = f"{folder}{os.pathsep}{os.environ.get('PATH', '')}"
    import onnxruntime

    return onnxruntime


def pick(providers) -> Engine:
    """The engine to use, given the providers the installed runtime offers."""
    for engine in ENGINES:
        if engine.provider in providers:
            return engine
    raise RuntimeError("the AI runtime has no engine for the graphics card")


def session(path, sizes: dict[str, int]):
    """The model file at `path`, loaded on the graphics card with its named dimensions set to `sizes`."""
    ort = runtime()
    engine = pick(ort.get_available_providers())
    options = ort.SessionOptions()
    options.log_severity_level = 3
    for name, value in sizes.items():
        options.add_free_dimension_override_by_name(name, value)
    engine.prepare(options)
    made = ort.InferenceSession(str(path), options, providers=[(engine.provider, engine.options()), *engine.also])
    if made.get_providers()[0] != engine.provider:
        raise RuntimeError("the AI runtime could not start on the graphics card")
    return made
