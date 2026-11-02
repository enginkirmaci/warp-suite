; Inno Setup 6 script - optional per-user installer for Warp Station, compiled
; manually with ISCC after build.ps1 has staged out\. The version fallback
; keeps plain `ISCC warp-station.iss` working too.

#ifndef MyAppVersion
#define MyAppVersion "0.0.0-dev"
#endif

#define MyAppName "Warp Station"
#define MyAppExeName "warp-station.exe"

[Setup]
AppId={{8E4C2A70-9B31-4E2B-9C7A-RUSTDEVTOOLS01}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=Engin Kirmaci
DefaultDirName={localappdata}\Programs\warp-station
DefaultGroupName={#MyAppName}
PrivilegesRequired=lowest
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=out
OutputBaseFilename=warp-station-{#MyAppVersion}-setup-x64
SetupIconFile=warp-station.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"

[Files]
Source: "out\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; The SnapIt companion lives in its own repo (warp-snapit) and ships
; separately; point the SnapIt.Executable setting at it.

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch Warp Station"; \
    Flags: nowait postinstall skipifsilent
