[CmdletBinding()]
param(
    [string] $WptRoot = $env:BREEZE_WPT_ROOT,
    [string] $Output,
    [string] $Filter,
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($WptRoot)) {
    throw 'Provide -WptRoot or set BREEZE_WPT_ROOT to an external WPT checkout.'
}
if ([string]::IsNullOrWhiteSpace($Output)) {
    $Output = Join-Path $repoRoot 'target\wpt\report.json'
}

Push-Location $repoRoot
try {
    if (-not $SkipBuild) {
        & cargo build --release --bin better-web-browser --bin wpt-runner
        if ($LASTEXITCODE -ne 0) { throw 'Could not build the WPT runner and Breeze.' }
    }

    $runner = Join-Path $repoRoot 'target\release\wpt-runner.exe'
    $browser = Join-Path $repoRoot 'target\release\better-web-browser.exe'
    $arguments = @(
        '--wpt-root', $WptRoot,
        '--browser', $browser,
        '--output', $Output
    )
    if (-not [string]::IsNullOrWhiteSpace($Filter)) {
        $arguments += @('--filter', $Filter)
    }

    # The runner launches Breeze with --benchmark and CREATE_NO_WINDOW. Benchmark mode omits
    # WS_VISIBLE, so every case stays off the user's desktop.
    & $runner @arguments
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
