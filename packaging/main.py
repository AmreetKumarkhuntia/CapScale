"""What PS5Scaler.exe runs. Kept outside the package so PyInstaller has a script to freeze."""
import sys

from ps5scaler.app import gui

sys.exit(gui())
