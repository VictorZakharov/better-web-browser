[CmdletBinding()]
param(
    [string] $Browser,
    [string] $OutputDirectory = 'target/audio-codecs-proof',
    [switch] $Chrome,
    [ValidateRange(0,1000)]
    [int] $Begin = 0,
    [ValidateRange(1,1000)]
    [int] $End = 1000,
    [Nullable[int]] $ExpectedPassed
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$outputRoot = [IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
if (-not $outputRoot.StartsWith('G:\',[StringComparison]::OrdinalIgnoreCase)) {
    throw 'Audio codec captures and profiles must stay on G:.'
}
if ($Begin -ge $End) { throw 'The probe range must have Begin < End.' }
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
$previousDotnetHome = $env:DOTNET_CLI_HOME
$scratch = Join-Path $outputRoot 'temp'
[IO.Directory]::CreateDirectory($scratch) | Out-Null
$env:TEMP = $scratch
$env:TMP = $scratch
. (Join-Path $PSScriptRoot 'alpha-fixture-server.ps1')
$server = $null
try {
    $server = Start-AlphaFixtureServer -OutputDirectory $outputRoot -Root (Join-Path $repo 'tests/audio-codec-fixtures')
    $url = $server.Url + "probe.html?begin=$Begin&end=$End"
    $output = Join-Path $outputRoot 'contracts.json'
    $selectors = @('#summary','#results tr')
    if ($Chrome) {
        # ChromeLaunch supplies unified --headless, CreateNoWindow and a fresh
        # profile. This wrapper does not construct another browser launch path.
        $env:DOTNET_CLI_HOME = Join-Path $repo 'target/dotnet-home'
        dotnet run --project (Join-Path $repo 'benchmarks/chromium') --configuration Release --no-build -- `
            --url $url --output $output --diagnostic-selector '#summary' --diagnostic-selector '#results tr' `
            --require-fixture-ready --settle-ms 1000 --timeout-ms 15000
        if ($LASTEXITCODE -ne 0) { throw 'Headless Chromium audio comparison failed.' }
    } else {
        $arguments = @{ Url=$url; Output=$output; DiagnosticSelector=$selectors;
            SettleMs=1000; TimeoutSeconds=20; FreshProfile=$true }
        if ($Browser) { $arguments.Browser=$Browser }
        & (Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1') @arguments
    }
    $report = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
    if ($report.error) { throw "Browser measurement failed: $($report.error)" }
    $summary = @($report.diagnostics | Where-Object selector -eq '#summary')[0].matches[0]
    if ($null -eq $summary) { throw 'Audio summary is missing.' }
    $attributes = @{}
    if ($Chrome) {
        foreach ($attribute in $summary.attributes.PSObject.Properties) {
            $attributes[$attribute.Name]=$attribute.Value
        }
    } else {
        foreach ($attribute in $summary.attributes) { $attributes[$attribute.name]=$attribute.value }
        if (@($report.javascript_errors).Count -ne 0) { throw 'Audio fixture reported a JavaScript exception.' }
    }
    if (-not $attributes.ContainsKey('data-passed') -or -not $attributes.ContainsKey('data-total')) {
        throw 'Audio probes did not finish; no acceptance counts were published.'
    }
    $passed = [int]$attributes['data-passed']
    $total = [int]$attributes['data-total']
    if ($total -le 0 -or $passed -gt $total) { throw 'Audio acceptance counts are invalid.' }
    $acceptance = [ordered]@{ browser=$report.browser; passed=$passed; total=$total;
        failures=$attributes['data-failures']; source='project-owned synthetic samples';
        begin=$Begin; end=$End }
    [IO.File]::WriteAllText((Join-Path $outputRoot 'acceptance.json'),($acceptance|ConvertTo-Json -Depth 4))
    if ($null -ne $ExpectedPassed -and $passed -ne $ExpectedPassed) {
        throw "Audio acceptance: $passed/$total; expected $ExpectedPassed passes. Inspect $outputRoot."
    }
    Write-Host "Audio contracts: $passed/$total. Report: $output"
} finally {
    Stop-AlphaFixtureServer $server
    $env:TEMP = $previousTemp
    $env:TMP = $previousTmp
    $env:DOTNET_CLI_HOME = $previousDotnetHome
}
