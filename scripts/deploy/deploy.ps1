#Requires -Version 5.1
<#
.SYNOPSIS
    Installs or removes the suite's Windows apps.

.DESCRIPTION
    -Local installs a portable copy of what scripts\build\build.ps1 produced
    (%LOCALAPPDATA%\Programs\<app> plus a Start Menu shortcut).

    -Release downloads the single warp-suite Windows ZIP and runs its
    bundle-install.ps1.

    -Uninstall removes the recorded portable files and the old Inno
    uninstallers.
#>
param(
    [switch]$Local,
    [string]$Bundle,
    [switch]$Release,
    [string]$Tag = 'latest',
    [string[]]$App,
    [switch]$Uninstall,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$root = Split-Path -Parent (Split-Path -Parent $here)
# the GitHub slug releases live under: derived from origin, WARP_SUITE_REPO
# overrides, and the historical default stands in for anything undecidable
if ($env:WARP_SUITE_REPO) {
    $repo = $env:WARP_SUITE_REPO
} else {
    $url = & git -C $root remote get-url origin 2>$null
    $repo = if ($url -match 'github\.com[:/](.+?)(\.git)?$') { $Matches[1] } else { 'enginkirmaci/warp-suite' }
}
$programs = Join-Path $env:LOCALAPPDATA 'Programs'
$stateDir = Join-Path $env:LOCALAPPDATA 'warp-suite'
$record = Join-Path $stateDir 'installed.txt'
$startMenu = [Environment]::GetFolderPath('Programs')

$modes = @(@($Local.IsPresent, [bool]$Bundle, $Release.IsPresent, $Uninstall.IsPresent) |
    Where-Object { $_ })
if ($modes.Count -ne 1) {
    throw 'pick exactly one of -Local, -Bundle <dir>, -Release, -Uninstall'
}

function Record-Path {
    param([string]$Path)
    Add-Content -Path $record -Value $Path
}

function Install-Portable {
    param([string]$Source)
    $dirs = @(Get-ChildItem $Source -Directory |
        Where-Object { -not $script:App -or $script:App -contains $_.Name } |
        Sort-Object Name)
    if ($dirs.Count -eq 0) { throw "no staged app under $Source" }
    if (-not $DryRun) { New-Item -ItemType Directory -Force -Path $stateDir | Out-Null }

    foreach ($d in $dirs) {
        $target = Join-Path $programs $d.Name
        if ($DryRun) { Write-Host "+ copy $($d.FullName)\* -> $target"; continue }
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        Copy-Item (Join-Path $d.FullName '*') $target -Recurse -Force
        Get-ChildItem $target -Recurse -File | ForEach-Object { Record-Path $_.FullName }

        $exe = Get-ChildItem $target -Filter *.exe -File | Select-Object -First 1
        if (-not $exe) { throw "no exe in $target" }
        $lnkPath = Join-Path $startMenu "$($d.Name).lnk"
        $shell = New-Object -ComObject WScript.Shell
        $lnk = $shell.CreateShortcut($lnkPath)
        $lnk.TargetPath = $exe.FullName
        $lnk.WorkingDirectory = $target
        $lnk.Save()
        Record-Path $lnkPath
        Write-Host "installed $($d.Name) -> $target"
    }
}

function Install-Release {
    $api = "https://api.github.com/repos/$repo/releases"
    $url = if ($Tag -eq 'latest') { "$api/latest" } else { "$api/tags/$Tag" }
    if ($DryRun) { Write-Host "+ download release assets from $url"; return }

    $rel = Invoke-RestMethod -Uri $url -Headers @{ 'User-Agent' = 'warp-suite-deploy' }
    $bundle = @($rel.assets | Where-Object { $_.name -like 'warp-suite-*-windows-x64.zip' }) |
        Select-Object -First 1
    if (-not $bundle) { throw "no combined Windows bundle on $url" }

    $tmp = Join-Path $env:TEMP ("warp-suite-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    $zip = Join-Path $tmp $bundle.name
    Invoke-WebRequest -Uri $bundle.browser_download_url -OutFile $zip
    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    $top = Get-ChildItem $tmp -Directory | Select-Object -First 1
    if (-not $top) { throw "empty suite bundle: $($bundle.name)" }
    $installer = Join-Path $top.FullName 'bundle-install.ps1'
    if (-not (Test-Path $installer)) { throw "no bundle-install.ps1 in $($bundle.name)" }
    & $installer
}

function Uninstall-All {
    if (Test-Path $record) {
        foreach ($path in Get-Content $record) {
            if (-not $path) { continue }
            if ($DryRun) { Write-Host "+ remove $path"; continue }
            if (Test-Path $path) { Remove-Item $path -Recurse -Force }
        }
        if (-not $DryRun) { Remove-Item $record -Force }
    } else {
        Write-Host "nothing recorded in $record"
    }

    $uninstallRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
    $entries = @()
    if (Test-Path $uninstallRoot) {
        $entries = @(Get-ChildItem $uninstallRoot | ForEach-Object { Get-ItemProperty $_.PSPath } |
            Where-Object { $_.DisplayName -like 'Warp *' -and $_.UninstallString })
    }
    foreach ($entry in $entries) {
        $exe = $entry.UninstallString.Trim('"')
        if ($DryRun) { Write-Host "+ $exe /VERYSILENT"; continue }
        Write-Host "uninstalling $($entry.DisplayName)"
        Start-Process $exe -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait
    }
}

if ($Local) {
    $buildArgs = @{}
    if ($script:App) { $buildArgs['App'] = $script:App }
    if ($DryRun) { $buildArgs['DryRun'] = $true }
    & (Join-Path $here '..\build\build.ps1') @buildArgs
    if ($DryRun) { Write-Host "+ install $root\dist\windows -> $programs"; return }
    Install-Portable (Join-Path $root 'dist\windows')
} elseif ($Bundle) {
    if (-not (Test-Path $Bundle)) { throw "no such bundle: $Bundle" }
    $installer = Join-Path $Bundle 'bundle-install.ps1'
    if (Test-Path $installer) {
        if ($DryRun) { Write-Host "+ $installer" } else { & $installer }
    } else {
        Install-Portable $Bundle
    }
} elseif ($Release) {
    Install-Release
} else {
    Uninstall-All
}
