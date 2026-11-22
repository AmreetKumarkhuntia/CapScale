# CapScale documentation

CapScale shows a console through a capture card: each captured frame is upscaled,
sharpened and colour graded on the GPU, optionally with generated in-between
frames, and the card's sound is played alongside.

| Document | Read it when you want to |
|---|---|
| [User guide](user-guide.md) | install it, get a picture, use the settings window, fix a problem |
| [Configuration](configuration.md) | look up a `config.toml` key, a hotkey action, a command line flag or a file location |
| [Architecture](architecture.md) | understand the threads, the Rust / Python split and how a frame travels |
| [Render pipeline](pipeline.md) | read or change a shader, or follow what each GPU pass does |
| [Frame generation](frame-generation.md) | understand repeat detection, the built-in method and the AI (RIFE) method |
| [Python API](python-api.md) | drive the renderer or reuse the capture, audio or config modules from Python |
| [Development](development.md) | build from source, run the tests, build the installer |

The [README](../README.md) at the top of the repository is the short version of
the user guide. Licences of the ported code are in
[THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md).
