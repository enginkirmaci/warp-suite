#Requires -Version 5.1
# Builds snapit.exe (Rust port of the SnapIt companion) and stages the
# runnable files in out\: snapit.exe plus SnapIt's default settings, so a
# standalone exe seeds %USERPROFILE%\.devtools\snapit like the .NET
# companion. No zip, no installer - out\ is the deliverable.
# Run from a Windows machine - the engine is cfg(windows)-gated (Linux can
# only run the platform-neutral tests).
# Requires the Rust toolchain on PATH.
$ErrorActionPreference = 'Stop'

$here   = $PSScriptRoot
$repo   = Split-Path -Parent (Split-Path -Parent $here)   # script lives in packaging\windows
$outDir = Join-Path $here 'out'

Push-Location $repo
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    # cargo may place artifacts off-repo (e.g. CARGO_TARGET_DIR on a fast
    # local drive while the repo lives on a network share)
    $targetDir = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).target_directory
} finally {
    Pop-Location
}

$exe = Join-Path $targetDir 'release\snapit.exe'
if (-not (Test-Path $exe)) { throw "release exe not found: $exe" }

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Get-ChildItem $outDir -Force | Remove-Item -Recurse -Force

Copy-Item $exe (Join-Path $outDir 'snapit.exe')
Copy-Item (Join-Path $here 'settings') $outDir -Recurse

Write-Host "staged in $outDir"
