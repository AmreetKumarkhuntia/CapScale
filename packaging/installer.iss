; Inno Setup script for CapScale. Built by scripts\build-installer.ps1, which
; passes /DAppVersion and expects the frozen app in dist\CapScale.
; Installs for the current user only, so it never asks for administrator rights.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
; -Pause keeps the window open at the end, so the result does not flash past
#define AiSetup '-NoProfile -ExecutionPolicy Bypass -File ""{app}\ai-setup\install-ai.ps1"" -Pause'

[Setup]
AppId={{6C1B5E0A-3F0E-4E7B-9C41-7A1D2F5B8E90}
AppName=CapScale
AppVersion={#AppVersion}
AppPublisher=Amreet Khuntia
DefaultDirName={autopf}\CapScale
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=CapScale-Setup-{#AppVersion}
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\CapScale.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; the app holds its files open while it runs
CloseApplications=yes
; shown on the page after the files are installed
InfoAfterFile=getting-started.txt

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"
Name: "aiframes"; Description: "Download AI frame generation (about 4 GB, NVIDIA RTX only)"; Flags: unchecked

[Files]
Source: "..\dist\CapScale\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\THIRD_PARTY_LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\scripts\install-ai.ps1"; DestDir: "{app}\ai-setup"; Flags: ignoreversion
Source: "..\scripts\build_ai_model.py"; DestDir: "{app}\ai-setup"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\CapScale"; Filename: "{app}\CapScale.exe"
Name: "{autodesktop}\CapScale"; Filename: "{app}\CapScale.exe"; Tasks: desktopicon
Name: "{autoprograms}\CapScale AI setup"; Filename: "powershell.exe"; Parameters: "{#AiSetup}"

[UninstallDelete]
; also remove anything the app wrote next to itself, so no empty folder is left
Type: filesandordirs; Name: "{app}"
; the optional AI frame generation runtime (scripts\install-ai.ps1), about 4 GB
Type: filesandordirs; Name: "{localappdata}\CapScale\ai"
Type: dirifempty; Name: "{localappdata}\CapScale"

[Run]
Filename: "powershell.exe"; Parameters: "{#AiSetup}"; StatusMsg: "Downloading AI frame generation (about 4 GB)..."; Tasks: aiframes; Flags: waituntilterminated
Filename: "{app}\CapScale.exe"; Description: "Start CapScale"; Flags: nowait postinstall skipifsilent
