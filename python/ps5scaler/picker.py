"""Native popups: the LUT file dialog and error messages."""
from __future__ import annotations

import sys
import tkinter as tk
from tkinter import filedialog, messagebox


def _root(title: str) -> tk.Tk:
    if sys.platform == "win32":
        import ctypes

        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(2)
        except OSError:
            pass
    root = tk.Tk()
    root.title(title)
    root.attributes("-topmost", True)
    return root


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
