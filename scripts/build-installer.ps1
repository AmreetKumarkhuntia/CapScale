# Builds dist\CapScale\CapScale.exe and dist\CapScale-Setup-<version>.exe.
#
# Needs: Rust, uv, and Inno Setup 6 (https://jrsoftware.org/isinfo.php).
# Works in a throwaway environment (.venv-build), so it neither needs nor disturbs
# the development one, and can run while the app is open.
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

$version = (Select-String -Path pyproject.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$iscc = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 not found. Install it from https://jrsoftware.org/isinfo.php" }

function Step($name, [scriptblock]$run) {
    Write-Host "== $name"
    # uv and friends write progress to stderr, which Windows PowerShell would treat as
    # a failure under "Stop"; the exit code below is what decides
    $ErrorActionPreference = "Continue"
    & $run
    if ($LASTEXITCODE -ne 0) { throw "$name failed (exit $LASTEXITCODE)" }
}

Step "wheel" { uv tool run maturin build --release --out dist\wheels }
$wheel = Get-ChildItem dist\wheels\capscale-$version-*.whl | Sort-Object LastWriteTime | Select-Object -Last 1

# An existing environment is kept: an editor that has picked it up runs its python.exe,
# and Windows will not let that file be replaced.
if (-not (Test-Path .venv-build\Scripts\python.exe)) {
    Step "build environment" { uv venv .venv-build --python 3.12 }
}
Step "install" { uv pip install --python .venv-build --reinstall $wheel.FullName pyinstaller }

# --add-data brings the shaders, which Python imports alone would not find.
# Absolute paths: PyInstaller resolves relative ones against --specpath.
# The hidden imports are what ONNX Runtime needs from the standard library: it is not
# part of this build (scripts\install-ai.ps1 installs it apart), so PyInstaller cannot see that.
Step "freeze" {
    & .venv-build\Scripts\pyinstaller.exe --noconfirm --clean --windowed `
        --name CapScale --icon "$repo\packaging\icon.ico" `
        --add-data "$repo\python\capscale\shaders;capscale\shaders" `
        --hidden-import socket --hidden-import selectors `
        --distpath dist --workpath build\pyinstaller --specpath build `
        "$repo\packaging\main.py"
}

# a module or data file left out only fails at run time, so run it once
Step "self-test" {
    $test = Start-Process dist\CapScale\CapScale.exe -ArgumentList "--self-test" -PassThru
    if (-not $test.WaitForExit(120000)) {
        $test.Kill()
        throw "self-test hung: a start-up error dialog is probably open, run the exe by hand to see it"
    }
    $global:LASTEXITCODE = $test.ExitCode
    if ($test.ExitCode -ne 0) { Get-Content "$env:APPDATA\CapScale\self-test.log" -ErrorAction SilentlyContinue }
}

Step "installer" { & $iscc /Qp "/DAppVersion=$version" packaging\installer.iss }

Get-ChildItem dist\CapScale-Setup-$version.exe, dist\CapScale\CapScale.exe |
    ForEach-Object { "{0}  ({1:N0} MB)" -f $_.FullName, ($_.Length / 1MB) }
