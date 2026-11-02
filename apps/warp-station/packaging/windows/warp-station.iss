; Inno Setup 6 script - optional per-user installer for Warp Station, compiled
; manually with ISCC after build.ps1 has staged out\. The version fallback
; keeps plain `ISCC warp-station.iss` working too.

#ifndef MyAppVersion
#define MyAppVersion "0.0.0-dev"
#endif

#define MyAppName "Warp Station"
#define MyAppExeName "warp-station.exe"

; build.ps1 passes the staging dir and the dist output dir; the fallbacks keep
; a manual `iscc warp-station.iss` working against packaging\windows\out.
#ifndef MySourceDir
#define MySourceDir "out"
#endif
#ifndef MyOutputDir
#define MyOutputDir "out"
#endif

[Setup]
AppId={{3F7A1C64-9D2E-4B58-8A17-6C05E9B2D4F1}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=Engin Kirmaci
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
PrivilegesRequired=lowest
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#MyOutputDir}
OutputBaseFilename=warp-station-{#MyAppVersion}-setup-x64
SetupIconFile=warp-station.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"

[Files]
Source: "{#MySourceDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; The SnapIt companion lives in its own repo (warp-snapit) and ships
; separately; point the SnapIt.Executable setting at it.

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch Warp Station"; \
    Flags: nowait postinstall skipifsilent
