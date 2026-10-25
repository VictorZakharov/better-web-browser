# Runs Acid3 from a local copy in hidden Breeze and, with -Firefox, headless Firefox as the
# reference, then lists each browser's failing subtests. See docs/acid3.md.
[CmdletBinding()]
param(
    [string] $Browser,
    [string] $OutputDirectory = 'target/acid3',
    [switch] $Firefox,
    [switch] $CompareReference,
    [string] $FirefoxPath = (Join-Path $env:ProgramFiles 'Mozilla Firefox\firefox.exe'),
    [ValidateRange(0, 100)][int] $MinimumScore = 0,
    [ValidateRange(10, 300)][int] $TimeoutSeconds = 60
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory, $repo)
$site = Join-Path $OutputDirectory 'site'
& (Join-Path $PSScriptRoot 'fetch-acid3.ps1') -Directory $site
. (Join-Path $PSScriptRoot 'alpha-fixture-server.ps1')

function Read-Acid3Report([string] $Label) {
    $path = Join-Path $OutputDirectory "$Label.json"
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    $report = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    $failures = [ordered]@{}
    foreach ($match in [regex]::Matches($report.log, '(?m)^Test (\d+) failed: (.*)$')) { $failures[$match.Groups[1].Value] = $match.Groups[2].Value }
    [pscustomobject]@{ Label = $Label; Score = [int]$report.score; Total = [int]$report.total; Failures = $failures; Log = $report.log }
}

function Clear-Acid3Report([string] $Label) {
    $path = Join-Path $OutputDirectory "$Label.json"
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
}

$server = Start-AlphaFixtureServer -OutputDirectory $OutputDirectory -Root $site -Script (Join-Path $PSScriptRoot 'serve-acid3.ps1')
$reports = @()
try {
    Clear-Acid3Report 'breeze'
    $arguments = @{
        Url = $server.Url + '?label=breeze'; Output = (Join-Path $OutputDirectory 'breeze-run.json')
        Screenshot = (Join-Path $OutputDirectory 'breeze.png'); CompletionTitle = 'Acid3 reported'
        SettleMs = 500; TimeoutSeconds = $TimeoutSeconds; WindowWidth = 1280; WindowHeight = 900; FreshProfile = $true
    }
    if ($Browser) { $arguments.Browser = $Browser }
    & (Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1') @arguments | Out-Null
    $reports += Read-Acid3Report 'breeze'
    if ($CompareReference) {
        # Breeze's own rendering of reference.html is the baseline, so font rasterization cancels out.
        $arguments.Url = $server.Url + 'reference.html'
        $arguments.Output = Join-Path $OutputDirectory 'reference-run.json'
        $arguments.Screenshot = Join-Path $OutputDirectory 'reference.png'
        # The reference title is static, so a completion title would capture the first frame
        # before reference.png arrives; settle instead.
        $arguments.Remove('CompletionTitle')
        $arguments.SettleMs = 1500
        & (Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1') @arguments | Out-Null
    }

    if ($Firefox) {
        Clear-Acid3Report 'firefox'
        $profile = Join-Path $OutputDirectory 'firefox-profile'
        [IO.Directory]::CreateDirectory($profile) | Out-Null
        $info = [Diagnostics.ProcessStartInfo]::new($FirefoxPath)
        foreach ($argument in '--headless', '--no-remote', '-profile', $profile, '--window-size=1280,900', ($server.Url + '?label=firefox')) { $info.ArgumentList.Add($argument) }
        $info.CreateNoWindow = $true
        $info.UseShellExecute = $false
        if (-not $info.ArgumentList.Contains('--headless')) { throw 'Refusing to start Firefox without --headless.' }
        $process = [Diagnostics.Process]::Start($info)
        try {
            $clock = [Diagnostics.Stopwatch]::StartNew()
            while (-not (Test-Path -LiteralPath (Join-Path $OutputDirectory 'firefox.json')) -and $clock.Elapsed.TotalSeconds -lt $TimeoutSeconds) { Start-Sleep -Milliseconds 100 }
        } finally {
            # Firefox restarts itself into child processes, so stop this run's whole tree.
            if (-not $process.HasExited) { $process.Kill($true) }
            $process.Dispose()
        }
        $reports += Read-Acid3Report 'firefox'
    }
} finally { Stop-AlphaFixtureServer $server }

$breeze = $reports | Where-Object Label -eq 'breeze'
if ($null -eq $breeze) { throw "Breeze produced no Acid3 report; inspect $(Join-Path $OutputDirectory 'breeze-run.json') and breeze.png." }
foreach ($report in $reports) {
    Write-Host ('{0}: {1}/{2}' -f $report.Label, $report.Score, $report.Total)
    foreach ($test in $report.Failures.Keys) { Write-Host ('  {0} {1}' -f $test, $report.Failures[$test]) }
}
$reference = $reports | Where-Object Label -eq 'firefox'
if ($Firefox -and $null -eq $reference) { Write-Warning 'Firefox produced no Acid3 report within the timeout.' }
if ($reference) {
    $behind = @($breeze.Failures.Keys | Where-Object { -not $reference.Failures.Contains($_) })
    Write-Host ('Breeze fails, Firefox passes: {0}' -f ($(if ($behind) { $behind -join ' ' } else { 'none' })))
}
$pixels = $null
if ($CompareReference) {
    $pixels = & (Join-Path $PSScriptRoot 'compare-acid3-pixels.ps1') -Actual (Join-Path $OutputDirectory 'breeze.png') `
        -Reference (Join-Path $OutputDirectory 'reference.png') -Diff (Join-Path $OutputDirectory 'reference-diff.png')
    Write-Host ('Pixels differing from the reference rendering: {0} of {1} ({2})' -f $pixels.DifferentPixels, $pixels.ComparedPixels, $pixels.Diff)
}
$summary = [ordered]@{ pixels = $pixels; reports = @($reports | ForEach-Object { [ordered]@{ label = $_.Label; score = $_.Score; total = $_.Total; failures = $_.Failures } }) }
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'summary.json'), (ConvertTo-Json -Depth 5 -InputObject $summary))
if ($breeze.Score -lt $MinimumScore) { throw "Breeze scored $($breeze.Score)/100, below the required $MinimumScore." }
