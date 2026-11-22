"""Command line entry point: capture -> Renderer, plus hotkeys, the settings window's requests and live config."""
from __future__ import annotations

import argparse
import json
import os
import sys
import threading
import time
import tomllib
from pathlib import Path

from . import ai, config
from .capture import devices
from ._core import frame_plan, show_error
from .capture.repeats import Cadence, luma_of

FROZEN = getattr(sys, "frozen", False)

HINTS = {"toggle:panel": "settings", "toggle:split": "split", "stop": "close", "quit": "quit"}


def _parse(argv: list[str] | None) -> argparse.Namespace:
    p = argparse.ArgumentParser(prog="capscale", description="Real-time capture-card upscaler")
    p.add_argument("--list-devices", action="store_true", help="print capture devices and their modes")
    p.add_argument("--config", help="path to config.toml (default: see README)")
    p.add_argument("--self-test", action="store_true", help=argparse.SUPPRESS)
    p.add_argument("--device", help="capture device name (starts capturing right away)")
    p.add_argument("--size", help="capture size, e.g. 1920x1080")
    p.add_argument("--fps", type=int, help="capture frame rate")
    p.add_argument("--format", help="auto, mjpeg, nv12 or yuyv422")
    p.add_argument("--out", help="output size, e.g. 2560x1440 (default: fit the display)")
    p.add_argument("--windowed", action="store_true", help="window instead of borderless fullscreen")
    p.add_argument("--vsync", choices=("mailbox", "fifo", "immediate"))
    p.add_argument(
        "--test-pattern",
        metavar="FMT",
        help="colour bars instead of a device: nv12, yuy2 or i420; add :30 for a 30 fps picture, e.g. i420:30",
    )
    return p.parse_args(argv)


def default_config_path() -> Path:
    """config.toml in the current folder when run from source. The installed program
    sits in a read-only folder, so there it lives in the user's profile."""
    if not FROZEN:
        return Path("config.toml")
    base = os.environ.get("APPDATA") or os.path.expanduser("~/.config")
    return config.app_dir(Path(base)) / "config.toml"


def _self_test(log: Path) -> int:
    """Load every part the app needs, without opening a window. Checks a packaged
    build, where a module or data file left out only shows up at run time."""
    try:
        import av  # noqa: F401
        import sounddevice  # noqa: F401

        from . import Renderer, audio  # noqa: F401
        from .capture import pattern, stream  # noqa: F401

        if ai.available():
            ai.Model(1920, 1080)

        shaders = Path(__file__).with_name("shaders")
        if not (shaders / "common.wgsl").is_file():
            raise FileNotFoundError(f"shaders missing from {shaders}")
        devices.list_devices()
        sounddevice.query_devices()
        return 0
    except Exception:
        import traceback

        log.parent.mkdir(parents=True, exist_ok=True)
        log.write_text(traceback.format_exc())
        return 1


def _size(text: str) -> tuple[int, int]:
    w, h = text.lower().split("x")
    return int(w), int(h)


def _print_devices() -> None:
    for name, label in devices.list_devices():
        print(label)
        for m in devices.list_modes(name):
            print(f"    {m.width}x{m.height} @ {m.max_fps:g} fps  {m.fmt}")


def _capture_from(cfg: dict, args: argparse.Namespace) -> dict:
    """[capture] with command line flags on top."""
    src = dict(cfg["capture"])
    if args.device:
        src["device"] = args.device
    if args.size:
        src["width"], src["height"] = _size(args.size)
    if args.fps:
        src["fps"] = args.fps
    if args.format:
        src["format"] = args.format
    return src


def _output_from(cfg: dict, args: argparse.Namespace) -> dict:
    """[output] with command line flags on top, as Renderer.set_params keywords."""
    out = cfg["output"]
    return {
        "out_size": _size(args.out) if args.out else (out["width"], out["height"]),
        "fullscreen": out["fullscreen"] and not args.windowed,
        "vsync": args.vsync or out["vsync"],
        "backend": out["backend"],
    }


def _read_json(path: Path) -> dict | None:
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def _open(src: dict, pattern_fmt: str | None):
    if pattern_fmt:
        from .capture.pattern import Pattern

        fmt, _, content = pattern_fmt.partition(":")
        return Pattern(fmt, src["width"], src["height"], src["fps"], float(content or src["fps"]))
    from .capture.stream import Capture

    fmt = src["format"]
    if fmt == "auto":
        mode = devices.best_mode(devices.list_modes(src["device"]), src["width"], src["height"], src["fps"])
        if mode is None:
            raise RuntimeError(
                f"{src['device']!r} has no {src['width']}x{src['height']} @ {src['fps']} fps mode"
            )
        fmt = mode.fmt
    return Capture(src["device"], src["width"], src["height"], src["fps"], fmt)


class _Pump(threading.Thread):
    """Capture thread: reads frames and hands each one to the renderer."""

    def __init__(self, renderer, src: dict, pattern_fmt: str | None, gen: ai.FrameGen | None = None):
        super().__init__(daemon=True, name="capture")
        self.renderer, self.src, self.pattern_fmt, self.gen = renderer, src, pattern_fmt, gen
        self.stopping = threading.Event()
        self.error: Exception | None = None
        self.framegen = (False, 0.0, 0.0, False)
        self.start()

    def run(self) -> None:
        try:
            source = _open(self.src, self.pattern_fmt)
            cadence, told, shown = Cadence(), None, (False, "")
            capture_fps = float(self.src["fps"])
            try:
                for data, fmt, w, h, full, matrix, ms in source.frames():
                    if self.stopping.is_set():
                        break
                    on, content, target, want_ai = self.framegen
                    by_ai = False
                    if on:
                        forced = None if content <= 0 else content < capture_fps * 0.75
                        keep = cadence.step(luma_of(data, fmt, w, h), forced)
                        game_fps = capture_fps / 2 if cadence.halved else capture_fps
                        if game_fps != told:
                            self.renderer.set_params(detected_fps=game_fps)
                            told = game_fps
                        if not keep:
                            continue
                        if want_ai and self.gen:
                            multiple, period = frame_plan(content, game_fps, target)
                            by_ai = multiple > 1 and self.gen.submit(
                                data, fmt, w, h, full, matrix, ms, multiple, period
                            )
                    state = (by_ai, self.gen.status if on and want_ai and self.gen else "")
                    if state != shown:
                        self.renderer.set_params(ai_active=by_ai, note=state[1])
                        shown = state
                    if not by_ai:
                        self.renderer.push_frame(
                            data, fmt=fmt, width=w, height=h, full_range=full, matrix=matrix, decode_ms=ms
                        )
            finally:
                source.close()
        except Exception as e:
            self.error = e

    def stop(self) -> None:
        self.stopping.set()
        self.join(timeout=3)


def _start_audio(src: dict, pattern_fmt: str | None):
    """The card's sound, or None when there is none to play (test pattern, no matching input)."""
    if pattern_fmt:
        return None
    try:
        from .audio import Audio

        return Audio(src["device"], src.get("audio_device", ""))
    except Exception as e:
        print(f"audio: {e}", file=sys.stderr)
        return None


def _hint(cfg: dict, audio=None) -> str:
    """The line under the stats: which keys do what, and the sound's delay."""
    hint = "   ".join(f"{key} {HINTS[action]}" for key, action in cfg["hotkeys"].items() if action in HINTS)
    if audio:
        hint += f"   audio {audio.latency_ms:.0f} ms"
    return hint


def _list_sources(renderer, src: dict, pattern_fmt: str | None) -> dict:
    """Send the capture devices to the settings window's Source tab, with `src` picked.
    Returns each device's modes by name."""
    if pattern_fmt:
        found = [(src["device"], f"test pattern {pattern_fmt}")]
        modes = {src["device"]: [devices.Mode(pattern_fmt, src["width"], src["height"], float(src["fps"]))]}
    else:
        found = devices.list_devices()
        modes = {name: devices.list_modes(name) for name, _ in found}
    renderer.set_sources(
        [(name, label, [(m.width, m.height, m.max_fps) for m in modes[name]]) for name, label in found],
        src["device"],
        src["width"],
        src["height"],
        src["fps"],
    )
    return modes


def _apply(renderer, cfg: dict, args: argparse.Namespace, audio=None) -> None:
    """Push [output], [params], [passes] and [audio] to the renderer; a typo is reported, not fatal."""
    sound = cfg["audio"]
    try:
        renderer.set_params(
            hint=_hint(cfg, audio),
            volume=sound["volume"],
            audio_delay_ms=sound["delay_ms"],
            **_output_from(cfg, args),
            **cfg["params"],
        )
        renderer.toggle_pass("audio", bool(sound["enabled"]))
        for name, on in cfg["passes"].items():
            renderer.toggle_pass(name, bool(on))
    except (TypeError, ValueError) as e:
        print(f"config: {e}", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    args = _parse(argv)
    if args.list_devices:
        _print_devices()
        return 0

    cfg_path = Path(args.config) if args.config else default_config_path()
    if args.self_test:
        return _self_test(cfg_path.with_name("self-test.log"))
    cfg_path.parent.mkdir(parents=True, exist_ok=True)
    cfg = config.load(cfg_path)
    last_path = cfg_path.with_name("last_source.json")
    src = _capture_from(cfg, args)
    saved = _read_json(last_path)
    if saved and not src["device"]:
        src.update(saved)

    from . import Renderer

    out = _output_from(cfg, args)
    r = Renderer(*out["out_size"], fullscreen=out["fullscreen"], vsync=out["vsync"], backend=out["backend"])
    gen = ai.FrameGen(r) if ai.available() else None
    r.set_params(ai_available=gen is not None)
    _apply(r, cfg, args)
    watcher = config.Watcher(cfg_path)
    last_error, retried_at = "", 0.0
    pump: _Pump | None = None
    audio = None

    def stop() -> None:
        nonlocal pump, audio
        if pump:
            pump.stop()
            pump = None
        if audio:
            audio.close()
            audio = None

    def start() -> None:
        nonlocal pump, audio
        stop()
        pump = _Pump(r, src, args.test_pattern, gen)
        audio = _start_audio(src, args.test_pattern)
        r.set_params(hint=_hint(cfg, audio))
        r.toggle_pass("capture", True)

    modes = _list_sources(r, src, args.test_pattern)
    if args.device or args.test_pattern:
        start()

    try:
        while True:
            time.sleep(0.02)
            if pump:
                pump.framegen = r.framegen
            if audio:
                on, audio.volume, audio.delay_ms = r.audio
                audio.muted = not on
            for kind, value in r.poll_events():
                action = cfg["hotkeys"].get(value, "") if kind == "key" else kind
                if action in ("quit", "closed"):
                    return 0
                if action == "start":
                    name, w, h, fps = value.split("\t")
                    w, h, fps = int(w), int(h), int(fps)
                    mode = None if args.test_pattern else devices.best_mode(modes.get(name, []), w, h, fps)
                    chosen = {"device": name, "width": w, "height": h, "fps": fps, "format": mode.fmt if mode else "auto"}
                    if not args.test_pattern:
                        last_path.write_text(json.dumps(chosen, indent=2))
                    src = {**src, **chosen}
                    start()
                elif action == "stop":
                    stop()
                    r.toggle_pass("capture", False)
                elif action == "refresh_sources":
                    modes = _list_sources(r, src, args.test_pattern)
                elif action == "save":
                    config.save_sections(cfg_path, value if kind == "save" else r.config_toml())
                    print(f"settings saved to {cfg_path}")
                elif action == "cycle:upscaler":
                    r.set_params(upscaler="next")
                elif action.startswith("toggle:"):
                    try:
                        r.toggle_pass(action[7:])
                    except ValueError as e:
                        print(f"hotkey {value}: {e}", file=sys.stderr)

            if watcher.changed():
                try:
                    new = config.load(cfg_path)
                except (tomllib.TOMLDecodeError, ValueError, OSError) as e:
                    print(f"config: {e}", file=sys.stderr)
                    continue
                _apply(r, new, args, audio)
                source_moved = new["capture"] != cfg["capture"]
                cfg = new
                if source_moved and cfg["capture"]["device"] and not args.test_pattern:
                    src = _capture_from(cfg, args)
                    if pump:
                        start()

            if pump and not pump.is_alive():
                message = str(pump.error or "capture ended")
                if message != last_error:
                    print(f"capture: {message} - retrying", file=sys.stderr)
                    r.set_params(hint=f"capture: {message} - retrying")
                    last_error = message
                time.sleep(1.0)
                pump = _Pump(r, src, args.test_pattern, gen)
                retried_at = time.monotonic()
            elif last_error and time.monotonic() - retried_at > 3:
                r.set_params(hint=_hint(cfg, audio))
                last_error = ""
    except KeyboardInterrupt:
        return 0
    finally:
        stop()
        if gen:
            gen.close()
        r.close()


def gui(argv: list[str] | None = None) -> int:
    """Entry point for the desktop shortcut, which has no console: a start-up failure
    is shown in a message box instead of vanishing."""
    if argv is None:
        argv = sys.argv[1:]
    try:
        return main(argv)
    except Exception as e:
        show_error(f"CapScale could not start.\n\n{type(e).__name__}: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(gui() if sys.stderr is None else main())
