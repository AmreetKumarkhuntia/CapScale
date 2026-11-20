from capscale.devices import Mode, best_mode, parse_dshow_modes, parse_v4l2_modes

DSHOW = (
    'DirectShow video device options (from video devices)\n Pin "Capture" (alternative pin name "0")\n'
    "  vcodec=mjpeg  min s=1920x1080 fps=15 max s=1920x1080 fps=60.0002\n"
    "  vcodec=mjpeg  min s=1920x1080 fps=15 max s=1920x1080 fps=60.0002 (pc, bt470bg/bt709/unknown, center)\n"
    "  vcodec=mjpeg  min s=3840x2160 fps=15 max s=3840x2160 fps=30\n"
    "  pixel_format=yuyv422  min s=1920x1080 fps=5 max s=1920x1080 fps=5\n"
    "  pixel_format=nv12  min s=1280x720 fps=30 max s=1280x720 fps=60\n"
    "  pixel_format=bgr24  min s=640x480 fps=30 max s=640x480 fps=30\n"
)
V4L2 = (
    "[video4linux2,v4l2 @ 0x55d0] Raw       :     yuyv422 :           YUYV 4:2:2 : 640x480 1920x1080\n"
    "[video4linux2,v4l2 @ 0x55d0] Compressed:       mjpeg :          Motion-JPEG : 1920x1080 1280x720\n"
)


def test_dshow_modes_are_deduplicated_and_sorted_largest_first():
    modes = parse_dshow_modes(DSHOW)
    assert modes[0] == Mode("mjpeg", 3840, 2160, 30.0)
    assert modes.count(Mode("mjpeg", 1920, 1080, 60.0002)) == 1
    assert Mode("yuyv422", 1920, 1080, 5.0) in modes and Mode("nv12", 1280, 720, 60.0) in modes


def test_v4l2_modes():
    modes = parse_v4l2_modes(V4L2)
    assert Mode("yuyv422", 1920, 1080, 60.0) in modes and Mode("mjpeg", 1280, 720, 60.0) in modes
    assert len(modes) == 4


def test_best_mode_prefers_raw_only_when_it_is_fast_enough():
    modes = parse_dshow_modes(DSHOW)
    assert best_mode(modes, 1920, 1080, 60).fmt == "mjpeg"
    assert best_mode(modes, 1920, 1080, 5).fmt == "yuyv422"
    assert best_mode(modes, 1280, 720, 60).fmt == "nv12"
    assert best_mode(modes, 3840, 2160, 60) is None
    assert best_mode(modes, 1920, 1080, 60, fmt="yuyv422") is None
