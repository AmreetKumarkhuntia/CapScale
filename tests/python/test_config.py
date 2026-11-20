import tomllib

from capscale import config

FILE = """# my settings
[capture]
device = "Card"   # keep me

[params]
sharpness = 0.4
lift = [0, 0, 0]

[hotkeys]
F1 = "toggle:panel"
"""
SAVED = '[output]\nwidth = 0\n\n[params]\nsharpness = 1.5\nlut_path = "C:\\\\luts\\\\film.cube"\n'


def test_save_replaces_only_the_given_sections(tmp_path):
    path = tmp_path / "config.toml"
    path.write_text(FILE)
    config.save_sections(path, SAVED)
    text = path.read_text()
    data = tomllib.loads(text)
    assert data["params"] == {"sharpness": 1.5, "lut_path": r"C:\luts\film.cube"}
    assert data["output"] == {"width": 0}
    assert data["capture"] == {"device": "Card"} and data["hotkeys"] == {"F1": "toggle:panel"}
    assert "# keep me" in text and text.count("[params]") == 1

    config.save_sections(path, SAVED)
    assert tomllib.loads(path.read_text()) == data


def test_save_creates_a_missing_file_and_load_merges_defaults(tmp_path):
    path = tmp_path / "config.toml"
    config.save_sections(path, SAVED)
    cfg = config.load(path)
    assert cfg["params"]["sharpness"] == 1.5
    assert cfg["output"]["fullscreen"] is True and cfg["hotkeys"]["Escape"] == "stop"


def test_unknown_section_is_an_error_and_watcher_sees_a_save(tmp_path):
    path = tmp_path / "config.toml"
    path.write_text("[oops]\nx = 1\n")
    try:
        config.load(path)
        raise AssertionError("expected ValueError")
    except ValueError:
        pass
    watcher = config.Watcher(path)
    assert not watcher.changed()
    import os

    os.utime(path, ns=(path.stat().st_atime_ns, path.stat().st_mtime_ns + 5_000_000_000))
    assert watcher.changed() and not watcher.changed()
