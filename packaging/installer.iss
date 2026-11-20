; Inno Setup script for CapScale. Built by scripts\build-installer.ps1, which
; passes /DAppVersion and expects the frozen app in dist\CapScale.
; Installs for the current user only, so it never asks for administrator rights.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

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

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"

[Files]
Source: "..\dist\CapScale\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\THIRD_PARTY_LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\CapScale"; Filename: "{app}\CapScale.exe"
Name: "{autodesktop}\CapScale"; Filename: "{app}\CapScale.exe"; Tasks: desktopicon

[UninstallDelete]
; also remove anything the app wrote next to itself, so no empty folder is left
Type: filesandordirs; Name: "{app}"
; the optional AI frame generation runtime (scripts\install-ai.ps1), about 4 GB
Type: filesandordirs; Name: "{localappdata}\CapScale\ai"
Type: dirifempty; Name: "{localappdata}\CapScale"

[Run]
Filename: "{app}\CapScale.exe"; Description: "Start CapScale"; Flags: nowait postinstall skipifsilent
