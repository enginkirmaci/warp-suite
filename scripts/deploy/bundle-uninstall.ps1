<#
.SYNOPSIS
    Removes what bundle-install.ps1 wrote.

.DESCRIPTION
    Reads %LOCALAPPDATA%\warp-suite\installed.txt and deletes exactly the
    files and Start Menu shortcuts recorded there. It does not touch other
    user files.
#>
$ErrorActionPreference = 'Stop'
$stateDir = Join-Path $env:LOCALAPPDATA 'warp-suite'
$record = Join-Path $stateDir 'installed.txt'
$prefix = Join-Path $env:LOCALAPPDATA 'Programs\warp-suite'
$menuDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\warp-suite'

if (-not (Test-Path $record)) {
    Write-Host "nothing recorded in $record"
    exit 0
}

$removed = 0
Get-Content -Path $record | ForEach-Object {
    if ($_ -and (Test-Path $_)) {
        Remove-Item -LiteralPath $_ -Force -ErrorAction SilentlyContinue
        $removed++
    }
}

Remove-Item -LiteralPath $stateDir -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $menuDir -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $prefix -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "removed $removed files"
