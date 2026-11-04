#Requires -Version 5.1
<#
.SYNOPSIS
    Installs or removes the suite's Windows apps.

.DESCRIPTION
    -Local installs a portable copy of what scripts\build.ps1 produced
    (%LOCALAPPDATA%\Programs\<app> plus a Start Menu shortcut).

    -Release installs a published release through its Inno Setup installers,
    silently; an app without one (warp-snapit) falls back to its portable zip.

    -Uninstall removes the recorded portable files and runs the Inno
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
$root = Split-Path -Parent $here
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
    $setups = @($rel.assets | Where-Object { $_.name -like '*-setup-x64.exe' })
    $zips = @($rel.assets | Where-Object { $_.name -like '*-windows-portable.zip' })
    if ($setups.Count + $zips.Count -eq 0) { throw "no Windows assets on $url" }
    $setupApps = @($setups | ForEach-Object { $_.name -replace '-[0-9].*$', '' })

    foreach ($asset in $setups) {
        $appName = $asset.name -replace '-[0-9].*$', ''
        if ($script:App -and ($script:App -notcontains $appName)) { continue }
        $tmp = Join-Path $env:TEMP $asset.name
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $tmp
        Write-Host "installing $($asset.name)"
        Start-Process $tmp -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait
    }

    # Apps without an installer (warp-snapit) ship as a portable zip.
    foreach ($asset in $zips) {
        $appName = $asset.name -replace '-[0-9].*$', ''
        if ($setupApps -contains $appName) { continue }
        if ($script:App -and ($script:App -notcontains $appName)) { continue }
        $tmp = Join-Path $env:TEMP $asset.name
        $target = Join-Path $programs $appName
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $tmp
        Write-Host "installing $($asset.name) -> $target"
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        Expand-Archive -Path $tmp -DestinationPath $target -Force
        New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
        Get-ChildItem $target -Recurse -File | ForEach-Object { Record-Path $_.FullName }
    }
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
    & (Join-Path $here 'build.ps1') @buildArgs
    if ($DryRun) { Write-Host "+ install $root\dist\windows -> $programs"; return }
    Install-Portable (Join-Path $root 'dist\windows')
} elseif ($Bundle) {
    if (-not (Test-Path $Bundle)) { throw "no such bundle: $Bundle" }
    Install-Portable $Bundle
} elseif ($Release) {
    Install-Release
} else {
    Uninstall-All
}
