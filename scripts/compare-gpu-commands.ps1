[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $CandidateBrowser,
    [string] $BaselineBrowser,
    [Parameter(Mandatory)][string] $ReferenceAssembly,
    [Parameter(Mandatory)][string] $FixtureBaseUrl,
    [Parameter(Mandatory)][string] $OutputDirectory,
    [ValidateRange(1, 20)][int] $Repetitions = 3,
    [ValidateRange(1000, 30000)][int] $SettleMs = 5000,
    [string] $Chrome
)

$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/gpu-command-results.ps1"
$root = [IO.Path]::GetFullPath($OutputDirectory)
$summaryPath = Join-Path $root 'summary.json'
if (Test-Path -LiteralPath $summaryPath) { throw 'Refusing to overwrite a GPU command comparison.' }
$variants = @([pscustomobject]@{ name = 'candidate'; browser = (Resolve-Path -LiteralPath $CandidateBrowser).Path })
if ($BaselineBrowser) {
    $variants = @([pscustomobject]@{ name = 'baseline'; browser = (Resolve-Path -LiteralPath $BaselineBrowser).Path }) + $variants
}
$variants += [pscustomobject]@{ name = 'chrome'; browser = $null }
$reference = (Resolve-Path -LiteralPath $ReferenceAssembly).Path
$fixtureUri = [Uri]$FixtureBaseUrl
if (!$fixtureUri.IsAbsoluteUri -or $fixtureUri.Scheme -notin @('http', 'https')) {
    throw 'Provide the HTTP(S) root serving benchmarks/gpu/fixtures.'
}
[IO.Directory]::CreateDirectory($root) | Out-Null
$oldTemp, $oldTmp = $env:TEMP, $env:TMP
$env:TEMP = Join-Path $root 'temp'
$env:TMP = $env:TEMP
[IO.Directory]::CreateDirectory($env:TEMP) | Out-Null
$rows = [Collections.Generic.List[object]]::new()
$warnings = [Collections.Generic.List[string]]::new()
try {
    foreach ($fixture in @('numeric-setters', 'completed-canvas-frames')) {
        $url = [Uri]::new($fixtureUri, "$fixture.html").AbsoluteUri
        $fixturePath = Join-Path $PSScriptRoot "../benchmarks/gpu/fixtures/$fixture.html"
        $fixtureHash = (Get-FileHash -LiteralPath $fixturePath -Algorithm SHA256).Hash
        $response = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 15
        $servedHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($response.RawContentStream.ToArray()))
        if ($servedHash -ne $fixtureHash) { throw "Fixture server does not serve the checked-in bytes: $url" }
        foreach ($round in 1..$Repetitions) {
            for ($slot = 0; $slot -lt $variants.Count; $slot++) {
                $variant = $variants[($slot + $round - 1) % $variants.Count]
                $name = "$fixture-$($variant.name)-$round"
                $reportPath = Join-Path $root "$name.json"
                $shotPath = Join-Path $root "$name.png"
                if (Test-Path -LiteralPath $reportPath) { throw "Refusing to overwrite $name" }
                if ($variant.browser) {
                    & "$PSScriptRoot/run-hidden-benchmark.ps1" -Url $url -Browser $variant.browser `
                        -Output $reportPath -Screenshot $shotPath -FreshProfile -DeviceScaleFactor 1 `
                        -SettleMs $SettleMs -TimeoutSeconds 120
                } else {
                    $arguments = @($reference, '--url', $url, '--output', $reportPath,
                        '--screenshot', $shotPath, '--viewport-width', '1262', '--viewport-height', '539',
                        '--device-scale-factor', '1', '--settle-ms', [string]$SettleMs, '--timeout-ms', '120000')
                    if ($Chrome) { $arguments += @('--chrome', $Chrome) }
                    & dotnet @arguments
                    if ($LASTEXITCODE -ne 0 -and !(Test-Path -LiteralPath $reportPath)) {
                        throw "Reference capture failed before producing a report: $name"
                    }
                }
                $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
                if ($report.error -or $report.http_status -ne 200 -or $report.javascript_runtime_stopped -or
                    $report.javascript_errors.Count -or $report.titles.document_title_truncated) {
                    throw "Capture failed its document/runtime checks: $name"
                }
                if ($report.cleanup_error) {
                    $warnings.Add("$name`: $($report.cleanup_error)")
                    Write-Warning $warnings[-1]
                }
                foreach ($measurement in @(Get-GpuCommandMeasurements -Fixture $fixture -Title $report.titles.document_title)) {
                    $rows.Add([pscustomobject]@{ variant = $variant.name; round = $round; fixture = $fixture;
                        fixture_sha256 = $fixtureHash; case = $measurement.case; active_ms = $measurement.active_ms;
                        iterations = $measurement.iterations; microseconds_per_iteration = $measurement.microseconds_per_iteration;
                        work_unit = $measurement.work_unit; report = $reportPath; chrome_version = $report.chrome_version })
                }
                Write-Output "$name`: $($report.titles.document_title)"
            }
        }
    }
    $statistics = @($rows | Group-Object variant, case | ForEach-Object {
        $group = $_.Group
        $sample = Get-GpuCommandStatistics -Values @($group.microseconds_per_iteration)
        [pscustomobject]@{ variant = $group[0].variant; case = $group[0].case; samples = $sample.samples;
            median_microseconds = $sample.median; minimum_microseconds = $sample.minimum;
            maximum_microseconds = $sample.maximum; work_unit = $group[0].work_unit }
    })
    $binaries = @($variants | Where-Object browser | ForEach-Object {
        [pscustomobject]@{ variant = $_.name; path = $_.browser; sha256 = (Get-FileHash -LiteralPath $_.browser).Hash }
    })
    [pscustomobject]@{ schema_version = 1; captured_utc = [DateTime]::UtcNow.ToString('o');
        clock_scope = 'wall time inside each active command loop, with inter-task yields excluded';
        environment_note = 'Shared-machine scheduling and driver caches are not isolated. No samples are discarded.';
        binaries = $binaries; repetitions = $Repetitions; samples = $rows.ToArray(); statistics = $statistics;
        cleanup_failures = $warnings.ToArray() } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $summaryPath -Encoding utf8
    $statistics | Format-Table variant, case, samples, median_microseconds, minimum_microseconds, maximum_microseconds
    if ($warnings.Count) { throw "Measurements retained at $summaryPath, but reference cleanup failed." }
} finally {
    $env:TEMP, $env:TMP = $oldTemp, $oldTmp
}
