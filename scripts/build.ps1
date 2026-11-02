#Requires -Version 5.1
<#
.SYNOPSIS
    Builds the Windows deliverables for every app under apps\ into dist\windows.

.DESCRIPTION
    The only Windows build path: local runs and the suite-release workflow both
    call this script. Per app it builds the release exe, stages it (plus any
    packaging\windows\settings seed) under dist\windows\<app>, writes the
    portable zip, and - when the app ships an Inno Setup script - compiles
    <app>-<version>-setup-x64.exe.

.PARAMETER Version
    Artifact and installer version. Defaults to git describe --tags --always
    --first-parent, or 'dev' outside a checkout.

.PARAMETER App
    Build only these apps (directory names under apps\). Repeatable.

.PARAMETER NoPackage
    Build and stage, but do not compile installers.

.PARAMETER DryRun
    Print the commands instead of running the build and packaging steps.
#>
param(
    [string]$Version,
    [string[]]$App,
    [switch]$NoPackage,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$root = Split-Path -Parent $here
$dist = Join-Path $root 'dist\windows'

if (-not $Version) {
    $Version = 'dev'
    if (Test-Path (Join-Path $root '.git')) {
        try {
            $described = & git -C $root describe --tags --always --first-parent 2>$null
            if ($LASTEXITCODE -eq 0 -and $described) { $Version = "$described".Trim() }
        } catch { }
    }
}

$appDirs = @(Get-ChildItem (Join-Path $root 'apps') -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName 'Cargo.toml') } |
    Sort-Object Name)
if ($App) {
    $appDirs = @($appDirs | Where-Object { $App -contains $_.Name })
    if ($appDirs.Count -eq 0) { throw "no app matched: $($App -join ', ')" }
}

$sha = 'unknown'
if (Test-Path (Join-Path $root '.git')) {
    try { $sha = (& git -C $root rev-parse HEAD 2>$null) } catch { }
}

New-Item -ItemType Directory -Force -Path $dist | Out-Null
$revisions = @()

foreach ($dir in $appDirs) {
    $name = $dir.Name
    Write-Host "== $name"

    Push-Location $dir.FullName
    try {
        $meta = & cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
        if ($LASTEXITCODE -ne 0 -or -not $meta) { throw "cargo metadata failed in $($dir.FullName)" }
    } finally {
        Pop-Location
    }

    $bins = @($meta.packages | ForEach-Object { $_.targets } |
        Where-Object { $_.kind -contains 'bin' } | ForEach-Object { $_.name })
    if ($bins.Count -eq 0) { throw "$name has no bin target" }
    # The binary name is not always the directory name (warp-snapit -> snapit).
    $bin = if ($bins -contains $name) { $name } else { $bins[0] }

    $stage = Join-Path $dist $name
    $zip = Join-Path $dist "$name-$Version-windows-portable.zip"
    $iss = Join-Path $dir.FullName "packaging\windows\$name.iss"
    $hasInstaller = (Test-Path $iss) -and -not $NoPackage

    if ($DryRun) {
        Write-Host "+ cargo build --release (in $($dir.FullName))"
        Write-Host "+ stage $($meta.target_directory)\release\${bin}.exe -> $stage"
        Write-Host "+ Compress-Archive $stage\* -> $zip"
        if ($hasInstaller) {
            Write-Host "+ iscc /DMyAppVersion=$Version /DMySourceDir=$stage /DMyOutputDir=$dist $iss"
        }
        $revisions += "$name $sha"
        continue
    }

    Push-Location $dir.FullName
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed in $($dir.FullName)" }
    } finally {
        Pop-Location
    }

    $exe = Join-Path $meta.target_directory "release\${bin}.exe"
    if (-not (Test-Path $exe)) { throw "release exe not found: $exe" }

    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    Get-ChildItem $stage -Force | Remove-Item -Recurse -Force
    Copy-Item $exe $stage
    $extra = Join-Path $dir.FullName 'packaging\windows\settings'
    if (Test-Path $extra) { Copy-Item $extra $stage -Recurse }

    Remove-Item $zip -ErrorAction SilentlyContinue
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
    Write-Host "portable: $zip"

    if ($hasInstaller) {
        if (-not (Get-Command iscc -ErrorAction SilentlyContinue)) {
            throw 'iscc not found - install Inno Setup 6 or pass -NoPackage'
        }
        & iscc "/DMyAppVersion=$Version" "/DMySourceDir=$stage" "/DMyOutputDir=$dist" $iss
        if ($LASTEXITCODE -ne 0) { throw "iscc failed with exit code $LASTEXITCODE" }
    }

    $revisions += "$name $sha"
}

if (-not $DryRun) {
    $revisions | Set-Content (Join-Path $dist 'REVISIONS.txt')
    Write-Host "staged in $dist"
}
