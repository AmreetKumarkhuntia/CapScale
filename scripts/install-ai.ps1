# Installs the optional AI frame generation: the open-source RIFE model (MIT) and
# what runs it on an NVIDIA card (ONNX Runtime, CUDA, TensorRT).
#
# About 4 GB, in %LOCALAPPDATA%\CapScale\ai. The app, installed or run from
# source, finds it there at its next start. To remove it, delete that folder.
# Run from the repo, or by the CapScale installer (its "AI frame generation" box),
# which copies this script and build_ai_model.py next to the app.
# Needs: an NVIDIA RTX card with a current driver. Fetches uv itself if it is missing.
param([switch]$Pause)
$ErrorActionPreference = "Stop"
trap {
    Write-Host "AI setup failed: $_" -ForegroundColor Red
    if ($Pause) { Read-Host "Press Enter to close" }
    break
}
$repo = Split-Path -Parent $PSScriptRoot
$pack = Join-Path $env:LOCALAPPDATA "CapScale\ai"
$work = Join-Path $env:TEMP "capscale-ai"
$model = "rife_v4.15_lite"
New-Item -ItemType Directory -Force $pack, $work | Out-Null
# downloads and the Python that runs build_ai_model.py live in $work, so deleting it leaves nothing behind
$env:UV_CACHE_DIR = "$work\uv-cache"
$env:UV_PYTHON_INSTALL_DIR = "$work\uv-python"

function Step($name, [scriptblock]$run) {
    Write-Host "== $name"
    # uv and friends write progress to stderr, which Windows PowerShell would treat as
    # a failure under "Stop"; the exit code below is what decides
    $ErrorActionPreference = "Continue"
    & $run
    if ($LASTEXITCODE -ne 0) { throw "$name failed (exit $LASTEXITCODE)" }
}

$uv = (Get-Command uv -ErrorAction SilentlyContinue).Source
if (-not $uv) {
    Step "uv" {
        curl.exe -sSL --fail -o "$work\uv.zip" `
            "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-pc-windows-msvc.zip"
        if ($LASTEXITCODE -eq 0) { & "$env:SystemRoot\System32\tar.exe" -xf "$work\uv.zip" -C $work }
    }
    $uv = "$work\uv.exe"
}
Step "python" { & $uv python install 3.12 }

# Versions that were tested together. onnxruntime-gpu names the CUDA libraries it
# was built for; TensorRT has to be the 10.x line for CUDA 13 to match it.
Step "runtime (the large download)" {
    & $uv pip install --python 3.12 --target "$pack\site" `
        "onnxruntime-gpu[cuda,cudnn]==1.32.0" "tensorrt-cu13-libs==10.16.1.11"
}

Step "model" {
    curl.exe -sSL --fail -o "$work\$model.7z" `
        "https://github.com/AmusementClub/vs-mlrt/releases/download/external-models/$model.7z"
}
# Windows' own tar reads 7z archives
Step "unpack" { & "$env:SystemRoot\System32\tar.exe" -xf "$work\$model.7z" -C $work }

# rife_v2 is the layout that takes any frame size; the wrapper makes it take the card's YUV frames
Step "prepare the model" {
    # what TensorRT prepared from an earlier model must not be used for the new one
    if (Test-Path "$pack\engines") { Remove-Item -Recurse -Force "$pack\engines" }
    & $uv run --no-project --python 3.12 --with onnx python "$PSScriptRoot\build_ai_model.py" `
        "$work\rife_v2\$model.onnx" "$pack\rife_yuv420.onnx"
}

# Loads it, times it and checks its output. The first load prepares the model for
# this graphics card at 1080p, which takes a minute or two and is then kept.
if (Test-Path "$repo\.venv\Scripts\python.exe") {
    Step "check" { & "$repo\.venv\Scripts\python.exe" -m capscale.ai 1920x1080 }
}
Remove-Item -Recurse -Force $work
"Installed in $pack. Restart CapScale and tick AI frames (RIFE) on the settings window's Frame rate tab."
if ($Pause) { Read-Host "Press Enter to close" }
