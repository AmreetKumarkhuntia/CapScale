"""Native popups: the OBS-style capture source picker, and the LUT file dialog."""
from __future__ import annotations

import sys
import tkinter as tk
from tkinter import filedialog, messagebox, ttk

from . import devices

FPS_CHOICES = (120, 60, 50, 30, 25)


def _root(title: str) -> tk.Tk:
    if sys.platform == "win32":  # otherwise Windows bitmap-scales the popup and it looks blurry
        import ctypes

        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(2)
        except OSError:  # already set, e.g. by the render window
            pass
    root = tk.Tk()
    root.title(title)
    root.attributes("-topmost", True)  # stay above the borderless fullscreen window
    return root


def pick(current: dict | None = None) -> dict | None:
    """Ask which capture source to use. Returns {device, width, height, fps, format}, or None if cancelled."""
    current = current or {}
    root = _root("ps5scaler - capture source")
    root.resizable(False, False)
    frame = ttk.Frame(root, padding=16)
    frame.grid()

    device_var, size_var, fps_var, status = tk.StringVar(), tk.StringVar(), tk.StringVar(), tk.StringVar()
    state: dict = {"names": {}, "modes": [], "result": None}

    def row(i: int, text: str, var: tk.StringVar) -> ttk.Combobox:
        ttk.Label(frame, text=text).grid(row=i, column=0, sticky="w", pady=4)
        box = ttk.Combobox(frame, textvariable=var, state="readonly", width=38)
        box.grid(row=i, column=1, columnspan=2, sticky="ew", padx=(12, 0), pady=4)
        return box

    device_box = row(0, "Device", device_var)
    size_box = row(1, "Resolution", size_var)
    fps_box = row(2, "Frame rate", fps_var)
    ttk.Label(frame, textvariable=status, foreground="#a33").grid(row=3, column=0, columnspan=3, sticky="w")

    def size_of(text: str) -> tuple[int, int]:
        w, h = text.split(" x ")
        return int(w), int(h)

    def on_size(*_: object) -> None:
        if not size_var.get():
            fps_box["values"] = []
            fps_var.set("")
            return
        top = max(m.max_fps for m in state["modes"] if (m.width, m.height) == size_of(size_var.get()))
        rates = [str(f) for f in FPS_CHOICES if f <= top + 0.5]
        fps_box["values"] = rates
        want = str(current.get("fps", 60))
        fps_var.set(want if want in rates else rates[0] if rates else "")

    def on_device(*_: object) -> None:
        name = state["names"].get(device_var.get())
        state["modes"] = devices.list_modes(name) if name else []
        sizes = list(dict.fromkeys(f"{m.width} x {m.height}" for m in state["modes"]))
        size_box["values"] = sizes
        want = f"{current.get('width', 1920)} x {current.get('height', 1080)}"
        size_var.set(want if want in sizes else sizes[0] if sizes else "")
        status.set("" if sizes or not name else "This device offers no MJPEG, NV12 or YUY2 mode.")
        on_size()

    def refresh() -> None:
        found = devices.list_devices()
        state["names"] = {label: name for name, label in found}
        labels = list(state["names"])
        device_box["values"] = labels
        want = next((label for name, label in found if name == current.get("device")), None)
        device_var.set(want or (labels[0] if labels else ""))
        on_device()
        if not labels:
            status.set("No capture devices found.")

    def start() -> None:
        name = state["names"].get(device_var.get())
        if not (name and size_var.get() and fps_var.get()):
            return
        w, h = size_of(size_var.get())
        fps = int(fps_var.get())
        mode = devices.best_mode(state["modes"], w, h, fps)
        state["result"] = {"device": name, "width": w, "height": h, "fps": fps, "format": mode.fmt}
        root.destroy()

    device_box.bind("<<ComboboxSelected>>", on_device)
    size_box.bind("<<ComboboxSelected>>", on_size)
    ttk.Button(frame, text="Refresh", command=refresh).grid(row=4, column=0, sticky="w", pady=(12, 0))
    ttk.Button(frame, text="Cancel", command=root.destroy).grid(row=4, column=1, sticky="e", pady=(12, 0))
    ttk.Button(frame, text="Start", command=start, default="active").grid(row=4, column=2, sticky="e", pady=(12, 0))
    root.bind("<Return>", lambda _e: start())
    root.bind("<Escape>", lambda _e: root.destroy())

    refresh()
    root.eval("tk::PlaceWindow . center")
    root.mainloop()
    return state["result"]


def pick_lut() -> str | None:
    """Ask for a .cube file. Returns its path, or None if cancelled."""
    root = _root("ps5scaler")
    root.withdraw()
    path = filedialog.askopenfilename(
        parent=root, title="Load 3D LUT", filetypes=[("Cube LUT", "*.cube"), ("All files", "*.*")]
    )
    root.destroy()
    return path or None


def show_error(message: str) -> None:
    root = _root("ps5scaler")
    root.withdraw()
    messagebox.showerror("ps5scaler", message, parent=root)
    root.destroy()
