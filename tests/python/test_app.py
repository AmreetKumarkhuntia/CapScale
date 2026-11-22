from capscale import app


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
