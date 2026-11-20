# Puts a "CapScale" shortcut on the Desktop and in the Start menu.
# It runs the app from this checkout with no console window, starting at the
# capture source popup. Run again after moving the folder; delete the two .lnk
# files to remove.
$repo = Split-Path -Parent $PSScriptRoot
$python = Join-Path $repo ".venv\Scripts\pythonw.exe"
if (-not (Test-Path $python)) { throw "Build first (see README): $python is missing" }

$shell = New-Object -ComObject WScript.Shell
foreach ($folder in [Environment]::GetFolderPath("Desktop"), [Environment]::GetFolderPath("Programs")) {
    $link = $shell.CreateShortcut((Join-Path $folder "CapScale.lnk"))
    $link.TargetPath = $python
    $link.Arguments = "-m capscale.app"
    $link.WorkingDirectory = $repo   # config.toml and last_source.json live here
    $link.Description = "Real-time capture-card upscaler"
    $link.Save()
    "created $($link.FullName)"
}
