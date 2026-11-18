#Requires -Version 5.1
<#
.SYNOPSIS
    Pulls the latest main of every app checkout, then builds the portable
    Windows output without Inno Setup installers.

.DESCRIPTION
    The app repositories live next to warp-suite and apps\ holds junctions to
    them (scripts\dev\link-apps.ps1). This script fetches each app's origin and
    fast-forwards main, then runs scripts\build\build.ps1 -NoPackage, so
    dist\windows gets the portable folders and zips but no setup exe. A dirty
    checkout, a feature branch or a diverged history is reported and left
    alone.

.PARAMETER Source
    Directory holding the app checkouts. Default: the directory above
    warp-suite.

.PARAMETER App
    Only these apps (directory names under apps\). Repeatable.

.PARAMETER Check
    Fetch and report; change nothing and build nothing.

.PARAMETER NoBuild
    Sync the checkouts, skip the build.

.PARAMETER DryRun
    Print the git and build commands instead of running them.
#>
param(
    [string]$Source,
    [string[]]$App,
    [switch]$Check,
    [switch]$NoBuild,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$python = $null
foreach ($candidate in @('python3', 'python', 'py')) {
    $command = Get-Command $candidate -ErrorAction SilentlyContinue
    if (-not $command) { continue }
    try {
        & $command.Source -c 'import tomllib' 2>$null
        $ok = $LASTEXITCODE -eq 0
    } catch {
        $ok = $false
    }
    if ($ok) { $python = $command.Source; break }
}
if (-not $python) { throw 'refresh: python3 not found' }

$syncArgs = @()
if ($Source) { $syncArgs += @('--source', $Source) }
foreach ($name in $App) { $syncArgs += @('--app', $name) }
if ($Check) { $syncArgs += '--check' }
if ($DryRun) { $syncArgs += '--dry-run' }

& $python (Join-Path $here 'sync_apps.py') @syncArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($Check -or $NoBuild) { exit 0 }

$buildArgs = @{ NoPackage = $true }
if ($App) { $buildArgs['App'] = $App }
if ($DryRun) { $buildArgs['DryRun'] = $true }
& (Join-Path $here '..\build\build.ps1') @buildArgs
exit $LASTEXITCODE
