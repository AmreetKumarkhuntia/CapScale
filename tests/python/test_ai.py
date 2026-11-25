"""AI frame generation: the bookkeeping around the model, with a stand-in for the
model itself, and the real model where it is installed."""
import time

import numpy as np
import pytest

from capscale import ai
from capscale.ai import engines, nvidia, pack

PERIOD = 0.04


class Renderer:
    hud_zones: list = []
    ai_still = True
    ai_model = ""
    ai_upscale = ""
    output_size = (8, 8)
    framegen = (False, 0.0, 0.0, False)

    def __init__(self):
        self.shown = []

    def push_frame(self, frame, **_):
        self.shown.append(int(frame[0][0, 0]))


class Model:
    took = 0.001

    def __init__(self, width, height, name=""):
        self.name = name

    def generate(self, older, newer, when, matrix, full_range, keep_still):
        return tuple(((1 - when) * a + when * b).astype(np.uint8) for a, b in zip(older, newer))


def frame(value):
    """A 4x4 frame as capture hands it over: rows of the Y plane are padded."""
    return np.full((4, 7), value, np.uint8), np.full((2, 2), value, np.uint8), np.full((2, 2), value, np.uint8)


def started(renderer, model=Model):
    gen = ai.FrameGen(renderer, model)
    assert not gen.submit(frame(0), "i420", 4, 4, None, None, 0.0, 2, PERIOD)
    deadline = time.monotonic() + 2
    while gen.model is None and not gen.failed and time.monotonic() < deadline:
        time.sleep(0.002)
    return gen


@pytest.mark.parametrize("multiple, expected", [(2, [10, 15, 20, 25, 30]), (4, [10, 12, 15, 17, 20, 22, 25, 27, 30])])
def test_generated_frames_are_shown_before_the_frame_they_lead_to(multiple, expected):
    renderer = Renderer()
    gen = started(renderer)
    for value in (10, 20, 30):
        assert gen.submit(frame(value), "i420", 4, 4, None, None, 0.0, multiple, PERIOD)
        time.sleep(PERIOD)
    gen.close()
    assert renderer.shown == expected


def test_frames_are_spread_evenly_over_the_game_frame():
    times = []

    class Timed(Renderer):
        def push_frame(self, frame, **_):
            times.append(time.perf_counter())

    gen = started(Timed())
    for value in (10, 20):
        gen.submit(frame(value), "i420", 4, 4, None, None, 0.0, 4, PERIOD)
        time.sleep(PERIOD)
    gen.close()
    gaps = np.diff(times[1:])
    assert len(gaps) == 3 and np.allclose(gaps, PERIOD / 4, atol=0.004)


def test_a_model_too_slow_for_the_rate_is_not_used():
    class Slow(Model):
        took = PERIOD

    renderer = Renderer()
    gen = started(renderer, Slow)
    assert not gen.submit(frame(10), "i420", 4, 4, None, None, 0.0, 2, PERIOD)
    assert "too slow" in gen.status
    gen.close()
    assert renderer.shown == []


def test_a_failing_model_still_lets_the_real_frame_through():
    class Broken(Model):
        def generate(self, *_):
            raise RuntimeError("out of memory")

    renderer = Renderer()
    gen = started(renderer, Broken)
    for value in (10, 20):
        gen.submit(frame(value), "i420", 4, 4, None, None, 0.0, 2, PERIOD)
        time.sleep(PERIOD)
    gen.close()
    assert renderer.shown == [10, 20]
    assert gen.failed and "out of memory" in gen.status
    assert not gen.submit(frame(30), "i420", 4, 4, None, None, 0.0, 2, PERIOD)


def test_a_model_that_does_not_load_is_reported():
    def missing(width, height, name):
        raise RuntimeError("the AI runtime could not start on the graphics card")

    gen = started(Renderer(), missing)
    assert gen.failed and "could not start" in gen.status


def test_choosing_another_model_loads_it():
    renderer = Renderer()
    gen = started(renderer)
    assert gen.model.name == ""
    renderer.ai_model = "nvidia-medium"
    assert not gen.submit(frame(0), "i420", 4, 4, None, None, 0.0, 2, PERIOD)
    deadline = time.monotonic() + 2
    while gen.model is None and time.monotonic() < deadline:
        time.sleep(0.002)
    gen.close()
    assert gen.model.name == "nvidia-medium"


def test_a_model_known_to_be_too_slow_for_a_larger_frame_is_not_prepared_for_it():
    made = []

    class Counted(Model):
        def __init__(self, width, height, name=""):
            made.append(width)

    def settled(gen):
        deadline = time.monotonic() + 2
        while gen.model is None and not gen.guess and time.monotonic() < deadline:
            time.sleep(0.002)

    gen = started(Renderer(), Counted)  # 1 ms for 4x4: 64 ms for 32x32, against a slot of 20
    assert not gen.submit(frame(0), "i420", 32, 32, None, None, 0.0, 2, PERIOD)
    settled(gen)
    assert not gen.submit(frame(0), "i420", 32, 32, None, None, 0.0, 2, PERIOD)
    assert made == [4] and gen.model is None and "about 64 ms each at this size, too slow" in gen.status
    assert not gen.submit(frame(0), "i420", 32, 32, None, None, 0.0, 2, 1.0)  # a slower game: it fits now
    settled(gen)
    gen.close()
    assert made == [4, 32] and gen.model is not None


class Scaler:
    took = 0.001

    def __init__(self, size, out, name):
        self.out = out

    def enlarge(self, planes, matrix):
        return tuple(np.full((self.out[1] // d, self.out[0] // d), p[0, 0], np.uint8) for p, d in zip(planes, (1, 2, 2)))


def upscaling(renderer, scaler=Scaler):
    """An Upscale whose model is loaded, a function that hands it a frame of one value, and the
    list of (first pixel, width, matrix) it hands on. Until the model is there frames go through as they are."""
    up, shown = ai.Upscale(renderer, scaler), []

    def take(value):
        up.submit(lambda frame, fmt, width, height, full_range, matrix, ms: shown.append((int(frame[0][0, 0]), width, matrix)), frame(value), "i420", 4, 4)  # fmt: skip
        time.sleep(0.005)

    deadline = time.monotonic() + 2
    while renderer.ai_upscale and up.scaler is None and "not available" not in up.status and time.monotonic() < deadline:
        take(0)
    up.jobs.join()
    shown.clear()
    return up, take, shown


def test_frames_are_enlarged_to_the_size_drawn_and_handed_on_in_order():
    renderer = Renderer()
    renderer.ai_upscale = "nvidia-high"
    up, take, shown = upscaling(renderer)
    for value in (10, 20, 30):
        take(value)
    up.close()
    assert shown == [(10, 8, "bt601"), (20, 8, "bt601"), (30, 8, "bt601")]
    assert up.status == "AI upscale: 1 ms a frame"


def test_without_an_upscaler_chosen_frames_go_straight_through():
    up, take, shown = upscaling(Renderer())
    for value in (10, 20, 30):
        take(value)
    up.close()
    assert shown == [(10, 4, None), (20, 4, None), (30, 4, None)] and up.status == ""


class Stalled(Scaler):
    def enlarge(self, planes, matrix):
        time.sleep(0.2)  # many times the gap between the frames below
        return super().enlarge(planes, matrix)


def test_frames_that_pile_up_behind_the_upscaler_are_left_out_not_shown_late():
    renderer = Renderer()
    renderer.ai_upscale = "nvidia-high"
    up, take, shown = upscaling(renderer, Stalled)
    for value in (10, 20, 30, 40, 50, 60):  # capture catching up after a stall: a burst
        take(value)
    up.jobs.join()
    assert shown == [(10, 8, "bt601"), (20, 8, "bt601"), (30, 8, "bt601")] and "too slow" not in up.status
    take(70)  # and it carries on
    up.close()
    assert shown[-1] == (70, 8, "bt601")


def test_an_upscaler_too_slow_for_the_rate_stands_down_and_is_tried_again(monkeypatch):
    monkeypatch.setattr(ai.upscale, "SPELL", 0.05)
    renderer = Renderer()
    renderer.ai_upscale = "nvidia-high"
    up, take, shown = upscaling(renderer, Stalled)
    for value in range(1, 31):  # frames kept piling up for longer than that
        take(value)
    assert shown[-1][:2] == (30, 4) and "too slow" in up.status
    assert [value for value, _, _ in shown] == sorted(value for value, _, _ in shown)
    up.until = 0.0  # the rest is over
    take(31)
    up.close()
    assert shown[-1] == (31, 8, "bt601")


def test_a_failing_upscaler_still_lets_the_frame_through():
    class Broken(Scaler):
        def enlarge(self, planes, matrix):
            raise RuntimeError("out of memory")

    renderer = Renderer()
    renderer.ai_upscale = "nvidia-high"
    up, take, shown = upscaling(renderer, Broken)
    for value in (10, 20):
        take(value)
    up.close()
    assert [handed[:2] for handed in shown] == [(10, 4), (20, 4)] and "out of memory" in up.status


def test_an_upscaler_that_does_not_load_is_reported():
    def missing(size, out, name):
        raise RuntimeError("the driver is too old")

    renderer = Renderer()
    renderer.ai_upscale = "nvidia-high"
    up, take, shown = upscaling(renderer, missing)
    take(10)
    up.close()
    assert shown == [(10, 4, None)] and "driver is too old" in up.status


def test_what_can_be_chosen_is_what_is_installed(tmp_path, monkeypatch):
    monkeypatch.setattr(pack, "PACK", tmp_path)
    monkeypatch.setattr(pack, "SITE", tmp_path / "site")
    monkeypatch.setattr(nvidia, "SITE", tmp_path / "site")
    assert ai.frame_models() == [] and ai.upscalers() == []
    (tmp_path / "site" / "onnxruntime").mkdir(parents=True)
    (tmp_path / "rife_v4.15_lite.onnx").touch()
    assert ai.frame_models() == [("rife_v4.15_lite", "rife v4.15 lite")] and ai.upscalers() == []
    for package in ("nvvfx", "cupy"):
        (tmp_path / "site" / package).mkdir()
    assert [name for name, _ in ai.frame_models()] == ["rife_v4.15_lite", "nvidia-low", "nvidia-medium", "nvidia-high"]
    assert ("nvidia-streaming-ultra", "Streaming Ultra") in ai.upscalers()
    assert nvidia._level("nvidia-highbitrate-ultra") == "HIGHBITRATE_ULTRA"
    monkeypatch.setattr(ai.choice, "Model", lambda width, height, name: ("onnx", name))
    monkeypatch.setattr(nvidia, "FrameGenerator", lambda width, height, name: ("nvidia", name))
    assert ai.frame_model(4, 4) == ("onnx", "rife_v4.15_lite")
    assert ai.frame_model(4, 4, "rife_yuv420") == ("onnx", "rife_v4.15_lite")  # a name from another PC's config
    assert ai.frame_model(4, 4, "nvidia-low") == ("nvidia", "nvidia-low")


def test_planes_of():
    y, u, v = ai.planes_of(frame(9), "i420", 4, 4)
    assert y.shape == (4, 4) and u.shape == v.shape == (2, 2) and y.flags.c_contiguous
    packed = np.concatenate([np.full(16, 1, np.uint8), np.tile(np.array([2, 3], np.uint8), 4)])
    y, u, v = ai.planes_of(packed, "nv12", 4, 4)
    assert (y == 1).all() and (u == 2).all() and (v == 3).all() and u.shape == (2, 2)
    assert ai.planes_of(np.zeros(32, np.uint8), "yuy2", 4, 4) is None


def test_hud_zones_keep_the_real_picture():
    made = tuple(np.zeros(s, np.uint8) for s in ((40, 40), (20, 20), (20, 20)))
    real = tuple(np.full(s, 200, np.uint8) for s in ((40, 40), (20, 20), (20, 20)))
    ai.keep_zones(made, real, [(0.0, 0.0, 0.5, 0.5, False), (0.5, 0.5, 1.0, 1.0, True)])
    y, u, _ = made
    assert (y[:20, :20] == 200).all() and (u[:10, :10] == 200).all()
    assert y[30, 30] == 200 and y[21, 21] == 0 and u[15, 15] == 200
    assert y[30, 5] == 0 and y[5, 30] == 0


def test_the_engine_is_the_first_one_the_runtime_offers():
    nvidia = ["TensorrtExecutionProvider", "CUDAExecutionProvider", "CPUExecutionProvider"]
    assert engines.pick(nvidia).provider == "TensorrtExecutionProvider"
    assert engines.pick(["DmlExecutionProvider", "CPUExecutionProvider"]).provider == "DmlExecutionProvider"
    with pytest.raises(RuntimeError, match="no engine"):
        engines.pick(["CPUExecutionProvider"])


def test_models_are_the_onnx_files_in_the_pack(tmp_path, monkeypatch):
    monkeypatch.setattr(pack, "PACK", tmp_path)
    with pytest.raises(FileNotFoundError, match="none"):
        pack.model_path()
    for name in ("rife_yuv420.onnx", "rife_v4.15_lite.onnx", "notes.txt"):
        (tmp_path / name).touch()
    assert list(pack.models()) == ["rife_v4.15_lite", "rife_yuv420"]
    assert pack.model_path() == tmp_path / "rife_v4.15_lite.onnx"
    assert pack.model_path("rife_yuv420") == tmp_path / "rife_yuv420.onnx"
    with pytest.raises(FileNotFoundError, match="rife_v4.15_lite, rife_yuv420"):
        pack.model_path("gmfss")


@pytest.mark.skipif(not ai.available(), reason="AI runtime not installed (scripts/install-ai.ps1)")
def test_the_real_model_on_this_graphics_card():
    assert ai._check(1920, 1080) == 0


@pytest.mark.skipif(not nvidia.installed(), reason="NVIDIA's models not installed (scripts/install-ai.ps1 on an RTX card)")
def test_nvidia_models_on_this_graphics_card():
    assert ai._check(1920, 1080, "nvidia-medium") == 0
    rng = np.random.default_rng(3)
    small = np.kron(rng.integers(40, 216, (45, 80), np.uint8), np.ones((16, 16), np.uint8))  # 1280x720 of 16-pixel blocks
    planes = small, np.full((360, 640), 100, np.uint8), np.full((360, 640), 160, np.uint8)
    y, u, v = nvidia.Upscaler((1280, 720), (1920, 1080), "nvidia-low").enlarge(planes)
    assert y.shape == (1080, 1920) and u.shape == v.shape == (540, 960)
    # the same picture, larger: block centres keep their brightness, the flat colour stays flat
    assert np.abs(y[12::24, 12::24].astype(int) - small[8::16, 8::16]).mean() < 6
    assert abs(u.mean() - 100) < 2 and abs(v.mean() - 160) < 2
