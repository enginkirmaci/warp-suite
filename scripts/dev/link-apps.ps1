# Link the sibling app checkouts into apps\ (Windows wrapper for link_apps.py).
$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$python = $null
foreach ($candidate in @('python3', 'python')) {
    $command = Get-Command $candidate -ErrorAction SilentlyContinue
    if ($command) { $python = $command.Source; break }
}
if (-not $python) { throw 'link-apps: python3 not found' }

& $python (Join-Path $here 'link_apps.py') @args
exit $LASTEXITCODE
