# Installs the optional AI frame generation: an open-source model (RIFE, MIT) and
# the runtime that runs it on the graphics card, chosen from the card:
#
#   NVIDIA RTX       ONNX Runtime with CUDA and TensorRT, and NVIDIA's own video models
#                    (frame generation and upscaling). About 4.6 GB, the fastest.
#   any other card   ONNX Runtime with DirectML (AMD, Intel, older NVIDIA). Small, slower.
#
# In %LOCALAPPDATA%\CapScale\ai. The app, installed or run from source, finds it
# there at its next start. To remove it, delete that folder.
# Run from the repo, or by the CapScale installer (its "AI frame generation" box),
# which copies this script and build_ai_model.py next to the app.
# Needs: a DirectX 12 graphics card with a current driver. Fetches uv itself if it is missing.
#
#   -Runtime nvidia|directml   instead of choosing from the graphics card
#   -Model <name>              another entry of $models; set [ai] model in config.toml to use it
param(
    [switch]$Pause,
    [ValidateSet("auto", "nvidia", "directml")][string]$Runtime = "auto",
    [string]$Model = "rife_v4.15_lite"
)
$ErrorActionPreference = "Stop"
trap {
    Write-Host "AI setup failed: $_" -ForegroundColor Red
    if ($Pause) { Read-Host "Press Enter to close" }
    break
}

# What runs a model, by name. Versions that were tested together: onnxruntime-gpu
# names the CUDA libraries it was built for, and TensorRT has to be the 10.x line for
# CUDA 13 to match it. A new one also needs an entry in python\capscale\ai\engines.py.
# nvidia-vfx is NVIDIA's video models (python\capscale\ai\nvidia.py), which CuPy hands the
# frames to; CuPy uses the CUDA 13 libraries ONNX Runtime brings. Needs driver 570.65 or newer.
$runtimes = @{
    nvidia   = "onnxruntime-gpu[cuda,cudnn]==1.32.0", "tensorrt-cu13-libs==10.16.1.11", "nvidia-vfx==0.2.0.0", "cupy-cuda13x==14.2.0"
    directml = "onnxruntime-directml==1.24.4"
}
# The models, by name: an archive, and the file in it that build_ai_model.py wraps.
# That has to be one that takes any frame size, with one input and one output
# (vs-mlrt's rife_v2 layout).
$models = @{
    "rife_v4.15_lite" = @{
        url  = "https://github.com/AmusementClub/vs-mlrt/releases/download/external-models/rife_v4.15_lite.7z"
        file = "rife_v2\rife_v4.15_lite.onnx"
    }
}
$source = $models[$Model]
if (-not $source) { throw "no model called '$Model'. There is: $($models.Keys -join ', ')" }

if ($Runtime -eq "auto") {
    $cards = @((Get-CimInstance Win32_VideoController).Name)
    $Runtime = if ($cards -match "NVIDIA.*RTX") { "nvidia" } else { "directml" }
    Write-Host "Graphics: $($cards -join ', '). Installing the $Runtime runtime."
}

$repo = Split-Path -Parent $PSScriptRoot
$pack = Join-Path $env:LOCALAPPDATA "CapScale\ai"
$work = Join-Path $env:TEMP "capscale-ai"
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

# Both runtimes are a package called onnxruntime, so one cannot be installed over the other
$had = if (Test-Path "$pack\site\tensorrt_libs") { "nvidia" } elseif (Test-Path "$pack\site\onnxruntime") { "directml" }
if ($had -and $had -ne $Runtime) {
    Remove-Item -Recurse -Force "$pack\site", "$pack\engines" -ErrorAction SilentlyContinue
}
$packages = $runtimes[$Runtime]
# NVIDIA's own index holds the files of its packages; PyPI only has pointers to them
Step "runtime ($Runtime)" {
    & $uv pip install --python 3.12 --target "$pack\site" --extra-index-url https://pypi.nvidia.com $packages
}

Step "model ($Model)" { curl.exe -sSL --fail -o "$work\model.7z" $source.url }
# Windows' own tar reads 7z archives
Step "unpack" { & "$env:SystemRoot\System32\tar.exe" -xf "$work\model.7z" -C $work }

# the wrapper makes the model take the card's YUV frames
Step "prepare the model" {
    # what TensorRT prepared from an earlier model must not be used for the new one
    if (Test-Path "$pack\engines") { Remove-Item -Recurse -Force "$pack\engines" }
    & $uv run --no-project --python 3.12 --with onnx python "$PSScriptRoot\build_ai_model.py" `
        "$work\$($source.file)" "$pack\$Model.onnx"
}
# this model's file name from before models had names
if ($Model -eq "rife_v4.15_lite") { Remove-Item "$pack\rife_yuv420.onnx" -ErrorAction SilentlyContinue }

# Loads it, times it and checks its output. With TensorRT the first load prepares the
# model for this graphics card at 1080p, which takes a minute or two and is then kept.
if (Test-Path "$repo\.venv\Scripts\python.exe") {
    Step "check" { & "$repo\.venv\Scripts\python.exe" -m capscale.ai 1920x1080 $Model }
}
Remove-Item -Recurse -Force $work
"Installed in $pack. Restart CapScale and tick AI frames (RIFE) on the settings window's Frame rate tab."
if ($Pause) { Read-Host "Press Enter to close" }
