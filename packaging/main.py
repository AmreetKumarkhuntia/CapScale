"""What CapScale.exe runs. Kept outside the package so PyInstaller has a script to freeze."""
import sys

from capscale.app import gui

sys.exit(gui())
