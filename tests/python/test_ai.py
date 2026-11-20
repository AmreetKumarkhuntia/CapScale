"""AI frame generation: the bookkeeping around the model, with a stand-in for the
model itself, and the real model where it is installed."""
import time

import numpy as np
import pytest

from ps5scaler import ai

PERIOD = 0.04


class Renderer:
    hud_zones: list = []
    ai_still = True

    def __init__(self):
        self.shown = []

    def push_frame(self, frame, **_):
        self.shown.append(int(frame[0][0, 0]))


class Model:
    took = 0.001

    def __init__(self, width, height):
        pass

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
    def missing(width, height):
        raise RuntimeError("the AI runtime could not start on the graphics card")

    gen = started(Renderer(), missing)
    assert gen.failed and "could not start" in gen.status


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


@pytest.mark.skipif(not ai.available(), reason="AI runtime not installed (scripts/install-ai.ps1)")
def test_the_real_model_on_this_graphics_card():
    assert ai._check(1920, 1080) == 0
