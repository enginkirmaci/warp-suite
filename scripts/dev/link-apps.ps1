# Link the sibling app checkouts into apps\ (Windows wrapper for link_apps.py).
$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
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
if (-not $python) { throw 'link-apps: python3 not found' }

& $python (Join-Path $here 'link_apps.py') @args
exit $LASTEXITCODE
