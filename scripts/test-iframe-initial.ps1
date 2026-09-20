[CmdletBinding()]
param(
    [string] $Browser,
    [string] $OutputDirectory = 'target/iframe-contexts-proof/initial',
    [switch] $Chrome
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$outputRoot = [IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
. (Join-Path $PSScriptRoot 'alpha-fixture-server.ps1')
$server = Start-AlphaFixtureServer -OutputDirectory $outputRoot -Script (Join-Path $PSScriptRoot 'serve-form-navigation.ps1')
try {
    $output = Join-Path $outputRoot 'initial.json'
    $url = $server.Url + 'iframe-initial'
    if ($Chrome) {
        dotnet run --project (Join-Path $repo 'benchmarks/chromium') --configuration Release --no-build -- --url $url --output $output --diagnostic-selector '#result' --require-fixture-ready --settle-ms 1000 --timeout-ms 15000
        if ($LASTEXITCODE -ne 0) { throw 'Headless Chrome iframe fixture failed.' }
    } else {
        $arguments = @{ Url=$url; Output=$output; DiagnosticSelector=@('#result'); SettleMs=1000; TimeoutSeconds=20; FreshProfile=$true }
        if ($Browser) { $arguments.Browser = $Browser }
        & (Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1') @arguments
    }
    $report = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
    if ($Chrome) {
        $attributes = @($report.diagnostics | Where-Object selector -eq '#result')[0].matches[0].attributes
        $passed = [int]$attributes.'data-passed'
        $failures = @($attributes.PSObject.Properties | Where-Object { $_.Value -like 'FAIL*' })
    } else {
        $passed = @($report.javascript_console | Where-Object { $_ -match '^log: IFRAME [a-z-]+: PASS$' }).Count
        $failures = @($report.javascript_console | Where-Object { $_ -match 'IFRAME .*: FAIL|^error:' })
        $failures += @($report.javascript_errors | Where-Object { $null -ne $_ })
    }
    $acceptance = [ordered]@{ passed=$passed; total=20; failures=$failures; browser_error=$report.error }
    [IO.File]::WriteAllText((Join-Path $outputRoot 'acceptance.json'), ($acceptance | ConvertTo-Json -Depth 5))
    if ($passed -ne 20 -or $failures.Count -gt 0 -or $report.error) {
        throw "Initial iframe document acceptance failed: $passed/20. Inspect $outputRoot."
    }
    Write-Host 'Initial iframe document acceptance: 20/20.'
} finally { Stop-AlphaFixtureServer $server }
