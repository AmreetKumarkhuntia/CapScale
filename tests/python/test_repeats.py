import random

import numpy as np

from ps5scaler.pattern import Pattern
from ps5scaler.repeats import Cadence, luma_of


def run(cadence: Cadence, changes, forced=None) -> list[bool]:
    return [cadence.feed(c, forced) for c in changes]


def game_30_in_60(frames: int, motion=600, noise=0, seed=0):
    """Change counts for a 30 fps game on a 60 Hz signal: new, repeat, new, repeat...

    `noise` is how many pixels count as changed on every frame, repeats included,
    wobbling by 10% from frame to frame.
    """
    rng = random.Random(seed)
    return [(motion if i % 2 == 0 else 0) + int(noise * rng.uniform(0.9, 1.1)) for i in range(frames)]


def test_a_30_fps_game_is_recognised_and_only_its_new_frames_pass():
    c = Cadence()
    kept = run(c, game_30_in_60(120))
    assert c.halved and c.keep == 0
    assert kept[60:] == [True, False] * 30  # settled: exactly the new frames


def test_noise_that_would_fool_a_fixed_threshold_does_not_change_the_answer():
    # every frame, repeats included, has thousands of noisy pixels; motion adds few.
    # A fixed "more than N changed pixels means new" would call every frame new.
    c = Cadence()
    kept = run(c, game_30_in_60(240, motion=400, noise=3000))
    assert c.halved
    assert sum(kept[120:]) == 60  # half the frames, steadily
    assert not any(a == b for a, b in zip(kept[120:], kept[121:]))  # strictly alternating


def test_a_60_fps_game_keeps_every_frame():
    rng = random.Random(1)
    c = Cadence()
    kept = run(c, [rng.randint(400, 800) for _ in range(120)])
    assert not c.halved and all(kept)


def test_a_still_picture_does_not_change_what_was_learned():
    c = Cadence()
    run(c, game_30_in_60(120))
    kept = run(c, [0] * 300)  # paused: nothing changes at all
    assert c.halved and sum(kept) == 150
    c = Cadence()
    run(c, [500] * 120)
    run(c, [3] * 300)
    assert not c.halved


def test_when_the_rhythm_slips_no_picture_is_lost_and_the_right_parity_is_found_again():
    # the game hitches once: one picture is shown three times, shifting new frames to the other parity
    pictures = []
    for n in range(100):
        pictures += [n] * (3 if n == 50 else 2)
    changes = [600 if i == 0 or pictures[i] != pictures[i - 1] else 0 for i in range(len(pictures))]
    c = Cadence()
    kept = run(c, changes)
    shown = [p for p, k in zip(pictures, kept) if k]
    settled = range(15, 100)  # the first quarter second is spent learning the rhythm
    assert set(settled) <= set(shown)  # every picture still reaches the screen
    assert max(shown.count(p) for p in settled) <= 2
    assert c.keep == 1  # and the tracker has moved to the new parity


def test_a_known_rate_is_obeyed_at_once():
    c = Cadence()
    assert run(c, [0] * 6, forced=True) == [True, False] * 3  # told 30: no waiting for evidence
    assert all(run(c, game_30_in_60(60), forced=False))  # told 60: everything passes


def test_luma_is_found_in_every_frame_layout():
    y = np.random.default_rng(0).integers(40, 200, (360, 640), dtype=np.uint8)
    h, w = y.shape
    chroma = np.zeros((h // 2, w // 2), np.uint8)
    nv12 = np.concatenate([y.ravel(), np.zeros(w * h // 2, np.uint8)])
    yuy2 = np.zeros((h, w, 2), np.uint8)
    yuy2[..., 0] = y
    for data, fmt in (((y, chroma, chroma), "i420"), (nv12, "nv12"), (yuy2.ravel(), "yuy2")):
        assert np.array_equal(luma_of(data, fmt, w, h), y), fmt


def test_real_pixels_from_the_30_in_60_test_pattern():
    frames = Pattern("i420", 640, 360, fps=2000, content_fps=1000).frames()  # fast clock for the test
    c = Cadence()
    kept = [c.step(luma_of(*next(frames)[:4])) for _ in range(80)]
    assert c.halved and sum(kept[40:]) == 20
