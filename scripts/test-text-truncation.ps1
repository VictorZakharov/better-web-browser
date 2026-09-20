# Owned text-truncation comparison runner (Breeze vs headless Chrome).
#
# Serves tests/fixtures/text-truncation.html from the local fixture server, then
# captures one browser run into the output directory:
#
#   ./scripts/test-text-truncation.ps1 -OutputDirectory target/muse-text-truncation-proof/before
#   ./scripts/test-text-truncation.ps1 -Chrome -OutputDirectory target/muse-text-truncation-proof/chrome
#
# Viewport matching: run the Breeze pass first, read the reported content viewport
# from truncation.json, then pass those dimensions to the Chrome pass via
# -ViewportWidth/-ViewportHeight so both screenshots share the content viewport.
# Device scale and locale default to 1 and en-US for both harnesses.
[CmdletBinding()]
param(
    [string] $Browser,
    [string] $OutputDirectory = 'target/muse-text-truncation-proof/after',
    [switch] $Chrome,
    [switch] $Contracts,
    [int] $ViewportWidth = 800,
    [int] $ViewportHeight = 600,
    [double] $DeviceScaleFactor = 1,
    [string] $Locale = 'en-US'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$outputRoot = [IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
. (Join-Path $PSScriptRoot 'alpha-fixture-server.ps1')
$server = Start-AlphaFixtureServer -OutputDirectory $outputRoot -Root (Join-Path $repo 'tests/fixtures')
try {
    $name = if ($Contracts) { 'truncation-contracts' } else { 'truncation' }
    $output = Join-Path $outputRoot "$name.json"
    $screenshot = Join-Path $outputRoot "$name.png"
    $url = $server.Url + "text-$name.html"
    $selectors = @(
        '#ref-plain', '#single-ellipsis', '#single-clip', '#single-narrow',
        '#single-padding', '#clamp-1', '#clamp-2', '#clamp-3', '#clamp-exact',
        '#clamp-none', '#clamp-unicode', '#flex-clamp', '#grid-clamp-inner',
        '#clamp-sibling', '#after-sibling'
    )
    if ($Contracts) {
        $selectors = @('html[data-fixture-ready=true]', '#report', '#clip',
            '#ellipsis', '#normal', '#nested', '#mixed', '#unicode')
    }
    if ($Chrome) {
        $arguments = @(
            '--url', $url,
            '--output', $output,
            '--screenshot', $screenshot,
            '--viewport-width', "$ViewportWidth",
            '--viewport-height', "$ViewportHeight",
            '--device-scale-factor', "$DeviceScaleFactor",
            '--locale', $Locale,
            '--settle-ms', '500',
            '--timeout-ms', '60000'
        )
        foreach ($selector in $selectors) {
            $arguments += @('--diagnostic-selector', $selector)
        }
        & dotnet run --project (Join-Path $repo 'benchmarks/chromium') --configuration Release --no-build -- @arguments
        if ($LASTEXITCODE -ne 0) { throw 'Headless Chrome text-truncation run failed.' }
    } else {
        $arguments = @{
            Url = $url
            Output = $output
            Screenshot = $screenshot
            DiagnosticSelector = $selectors
            SettleMs = 500
            TimeoutSeconds = 60
            FreshProfile = $true
            WindowWidth = $ViewportWidth
            WindowHeight = $ViewportHeight
            DeviceScaleFactor = $DeviceScaleFactor
            Locale = $Locale
        }
        if ($Browser) { $arguments.Browser = $Browser }
        & (Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1') @arguments
    }
    $report = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
    if ($report.error) { throw "Browser run reported an error: $($report.error)" }
    if ($report.javascript_errors -and @($report.javascript_errors).Count -gt 0) {
        throw 'Text-truncation fixture reported JavaScript errors.'
    }
    if ($Contracts) {
        $ready = @($report.diagnostics | Where-Object selector -eq 'html[data-fixture-ready=true]')
        if ($ready.Count -ne 1 -or $ready[0].total_matches -ne 1) {
            throw 'Text-truncation Range/scroll-width assertions did not pass.'
        }
    }
    if (-not (Test-Path -LiteralPath $screenshot -PathType Leaf)) {
        throw "Browser run did not produce $screenshot."
    }
    Write-Host "Text-truncation capture complete: $screenshot"
} finally { Stop-AlphaFixtureServer $server }
