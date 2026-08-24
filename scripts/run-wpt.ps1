[CmdletBinding()]
param(
    [string] $WptRoot = $env:BREEZE_WPT_ROOT,
    [string] $Manifest,
    [string] $Output,
    [string] $Filter,
    [ValidateSet('release', 'debug')]
    [string] $BuildProfile = 'release',
    [ValidateRange(1, 16)]
    [int] $Jobs = 1,
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
if ([string]::IsNullOrWhiteSpace($Manifest)) {
    $Manifest = Join-Path $repoRoot 'tests\wpt\manifest.json'
}

Push-Location $repoRoot
try {
    if (-not $SkipBuild) {
        [string[]] $profileArguments = if ($BuildProfile -eq 'release') {
            @('--release')
        } else {
            @()
        }
        & cargo build @profileArguments --bin better-web-browser --bin wpt-runner
        if ($LASTEXITCODE -ne 0) { throw 'Could not build the WPT runner and Breeze.' }
    }

    $binaryDirectory = Join-Path $repoRoot "target\$BuildProfile"
    $runner = Join-Path $binaryDirectory 'wpt-runner.exe'
    $browser = Join-Path $binaryDirectory 'better-web-browser.exe'
    $arguments = @(
        '--wpt-root', $WptRoot,
        '--manifest', $Manifest,
        '--browser', $browser,
        '--output', $Output,
        '--jobs', $Jobs
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
