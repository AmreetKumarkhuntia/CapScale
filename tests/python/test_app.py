import time

import numpy as np

from capscale import app


class Screen:
    """A renderer that only notes the width of each frame it is shown."""

    ai_upscale, ai_model, output_size, framegen = "nvidia-high", "", (128, 72), (False, 0.0, 0.0, False)

    def __init__(self):
        self.widths = []

    def set_params(self, **_):
        pass

    def push_frame(self, frame, width, **_):
        self.widths.append(width)


class Doubler:
    took = 0.0001

    def __init__(self, size, out, name):
        self.out = out

    def enlarge(self, planes, matrix):
        return tuple(np.zeros((self.out[1] // d, self.out[0] // d), np.uint8) for d in (1, 2, 2))


def test_captured_frames_reach_the_renderer_enlarged_when_there_is_an_ai_upscaler():
    src = {"device": "", "width": 64, "height": 36, "fps": 200}
    for with_upscaler, width in ((False, 64), (True, 128)):
        screen = Screen()
        upscale = app.ai.Upscale(screen, Doubler) if with_upscaler else None
        pump = app._Pump(screen, src, "i420", None, upscale)
        deadline = time.monotonic() + 3
        while screen.widths[-1:] != [width] and time.monotonic() < deadline:
            time.sleep(0.005)
        pump.stop()
        if upscale:
            upscale.close()
        assert pump.error is None and screen.widths[0] == 64 and screen.widths[-1] == width


def test_shortcut_entry_shows_a_start_up_failure_instead_of_losing_it(monkeypatch, tmp_path):
    shown = []
    monkeypatch.setattr(app, "show_error", shown.append)
    broken = tmp_path / "config.toml"
    broken.write_text("[capture\n")

    assert app.gui(["--config", str(broken)]) == 1
    assert len(shown) == 1 and "could not start" in shown[0] and "TOMLDecodeError" in shown[0]


def test_installed_program_keeps_its_config_in_the_user_profile(monkeypatch, tmp_path):
    assert app.default_config_path() == app.Path("config.toml")
    monkeypatch.setattr(app, "FROZEN", True)
    monkeypatch.setenv("APPDATA", str(tmp_path))
    assert app.default_config_path() == tmp_path / "CapScale" / "config.toml"


def test_command_line_overrides_config(tmp_path):
    cfg = app.config.load(tmp_path / "missing.toml")
    args = app._parse(["--device", "Card", "--size", "1280x720", "--out", "3840x2160", "--windowed", "--vsync", "fifo"])
    src = app._capture_from(cfg, args)
    assert (src["device"], src["width"], src["height"], src["fps"]) == ("Card", 1280, 720, 60)
    out = app._output_from(cfg, args)
    assert (out["out_size"], out["fullscreen"], out["vsync"]) == ((3840, 2160), False, "fifo")
    assert out["backend"] in ("dx12", "auto")
