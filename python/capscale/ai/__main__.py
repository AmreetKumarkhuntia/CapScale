import sys

from .check import _check

args = sys.argv[1:]
w, h = (int(n) for n in (args[0] if args else "1920x1080").lower().split("x"))
sys.exit(_check(w, h, args[1] if len(args) > 1 else ""))
