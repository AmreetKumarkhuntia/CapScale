; Inno Setup script for PS5 Scaler. Built by scripts\build-installer.ps1, which
; passes /DAppVersion and expects the frozen app in dist\PS5Scaler.
; Installs for the current user only, so it never asks for administrator rights.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6C1B5E0A-3F0E-4E7B-9C41-7A1D2F5B8E90}
AppName=PS5 Scaler
AppVersion={#AppVersion}
AppPublisher=Amreet Khuntia
DefaultDirName={autopf}\PS5 Scaler
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=PS5Scaler-Setup-{#AppVersion}
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\PS5Scaler.exe
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
Source: "..\dist\PS5Scaler\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\THIRD_PARTY_LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\PS5 Scaler"; Filename: "{app}\PS5Scaler.exe"
Name: "{autodesktop}\PS5 Scaler"; Filename: "{app}\PS5Scaler.exe"; Tasks: desktopicon

[UninstallDelete]
; also remove anything the app wrote next to itself, so no empty folder is left
Type: filesandordirs; Name: "{app}"
; the optional AI frame generation runtime (scripts\install-ai.ps1), about 4 GB
Type: filesandordirs; Name: "{localappdata}\PS5 Scaler\ai"
Type: dirifempty; Name: "{localappdata}\PS5 Scaler"

[Run]
Filename: "{app}\PS5Scaler.exe"; Description: "Start PS5 Scaler"; Flags: nowait postinstall skipifsilent
