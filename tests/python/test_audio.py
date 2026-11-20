import numpy as np

from ps5scaler.audio import DelayLine, match_input


def test_card_sound_input_is_found_by_the_video_device_name():
    names = ["Microphone (Realtek Audio)", "Microphone (Display capture-UVC03)", "Line (USB AUDIO  CODEC)"]
    assert match_input(names, "Display capture-UVC03") == 1
    assert match_input(names, "display CAPTURE") == 1
    assert match_input(names, "/dev/video0") is None
    assert match_input(names, "") is None


def test_delay_line_holds_samples_back_across_blocks_and_wraps():
    line = DelayLine(channels=2, max_samples=64)
    ramp = np.repeat(np.arange(1, 201, dtype=np.float32)[:, None], 2, axis=1)  # sample n has value n
    out = np.concatenate([line.process(ramp[i : i + 20], delay=30) for i in range(0, 200, 20)])
    assert np.all(out[:30] == 0)  # nothing to play yet
    assert np.array_equal(out[30:], ramp[:170])  # then the input, 30 samples late


def test_zero_delay_passes_audio_straight_through():
    line = DelayLine(channels=2, max_samples=64)
    block = np.random.default_rng(0).random((48, 2), dtype=np.float32)
    assert np.array_equal(line.process(block, 0), block)
