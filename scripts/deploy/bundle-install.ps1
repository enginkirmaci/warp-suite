<#
.SYNOPSIS
    Installs the warp-suite Windows bundle for the current user.

.DESCRIPTION
    Copies the bundled apps under %LOCALAPPDATA%\Programs\warp-suite, creates
    a Start Menu folder with one shortcut per app, and records every path it
    wrote so bundle-uninstall.ps1 can remove exactly those files.

    No administrator rights are needed.

.PARAMETER Prefix
    Install the binaries here instead of
    %LOCALAPPDATA%\Programs\warp-suite.
#>
param(
    [string]$Prefix = (Join-Path $env:LOCALAPPDATA 'Programs\warp-suite')
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$appsRoot = Join-Path $here 'apps'
$stateDir = Join-Path $env:LOCALAPPDATA 'warp-suite'
$record = Join-Path $stateDir 'installed.txt'
$menuDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\warp-suite'

if (-not (Test-Path $appsRoot)) {
    throw "not a warp-suite bundle (no apps directory): $here"
}

$apps = @(
    @{ Name = 'Warp Station'; Exe = 'warp-station\warp-station.exe' },
    @{ Name = 'Warp Agent';   Exe = 'warp-agent\warp-agent.exe' },
    @{ Name = 'Warp Notes';   Exe = 'warp-notes\warp-notes.exe' },
    @{ Name = 'Warp SnapIt';  Exe = 'warp-snapit\warp-snapit.exe' }
)

New-Item -ItemType Directory -Force -Path $Prefix, $stateDir, $menuDir | Out-Null
Get-ChildItem -Path $appsRoot -Directory | ForEach-Object {
    Copy-Item -Path $_.FullName -Destination $Prefix -Recurse -Force
}

$shell = New-Object -ComObject WScript.Shell
$paths = New-Object System.Collections.Generic.List[string]
foreach ($app in $apps) {
    $exe = Join-Path $Prefix $app.Exe
    if (-not (Test-Path $exe)) {
        Write-Warning "missing $($app.Exe); skipping shortcut"
        continue
    }
    $link = Join-Path $menuDir "$($app.Name).lnk"
    $shortcut = $shell.CreateShortcut($link)
    $shortcut.TargetPath = $exe
    $shortcut.WorkingDirectory = Split-Path -Parent $exe
    $shortcut.Description = 'warp-suite'
    $shortcut.Save()
    $paths.Add($link)
}

Get-ChildItem -Path $Prefix -Recurse -File | ForEach-Object { $paths.Add($_.FullName) }
$paths | Sort-Object -Unique | Set-Content -Path $record -Encoding UTF8

Write-Host "installed $($paths.Count) files under $Prefix"
Write-Host "Start Menu folder: $menuDir"
Write-Host "uninstall with bundle-uninstall.ps1 or uninstall.cmd"
