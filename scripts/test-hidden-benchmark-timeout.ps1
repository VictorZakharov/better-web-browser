[CmdletBinding()]
param([string] $Browser)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'hidden-benchmark-diagnostics.ps1')
$redactionProfile = 'C:\private\benchmark-profile'
$boundedDiagnostic = ConvertTo-BoundedBenchmarkDiagnostic -ProfilePath $redactionProfile -Value (
    "Authorization: Bearer secret`n$redactionProfile`n" +
    "https://user:password@example.test/path?token=secret`n" + ('x' * 20000)
)
if ($boundedDiagnostic -match 'Bearer secret|user:password|benchmark-profile|token=secret' -or
    $boundedDiagnostic.Length -gt (16 * 1024 + 20) -or
    -not $boundedDiagnostic.EndsWith('[truncated]')) {
    throw 'Hidden benchmark diagnostics were not bounded and redacted.'
}
if ((Get-BenchmarkFailureKind -Stderr 'renderer broker has exited') -ne 'renderer_broker_exit' -or
    (Get-BenchmarkFailureKind -Stderr 'renderer watchdog exceeded budget') -ne 'renderer_watchdog_exit' -or
    (Get-BenchmarkFailureKind -Stderr 'ordinary browser failure') -ne 'browser_exit') {
    throw 'Hidden benchmark process failures were not classified by origin.'
}
$pageErrorOutcome = Get-BenchmarkHarnessOutcome -PageError 'ordinary page error'
$successOutcome = Get-BenchmarkHarnessOutcome -PageError $null
if ($pageErrorOutcome.outcome -ne 'page_error' -or
    $pageErrorOutcome.failure_kind -ne 'ordinary_page_error' -or
    $successOutcome.outcome -ne 'success') {
    throw 'Hidden benchmark page outcomes were not classified independently from harness failures.'
}
if ([string]::IsNullOrWhiteSpace($Browser)) {
    $Browser = Join-Path $repoRoot 'target\debug\better-web-browser.exe'
}
$browserPath = (Resolve-Path -LiteralPath $Browser).Path
$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("breeze-timeout-test-" + [Guid]::NewGuid().ToString('N'))
$readyFile = Join-Path $workRoot 'fixture-server.txt'
$serverOutput = Join-Path $workRoot 'fixture-server.out'
$serverError = Join-Path $workRoot 'fixture-server.err'
$reportPath = Join-Path $workRoot 'timeout.json'
$screenshotPath = Join-Path $workRoot 'timeout.png'
$server = $null

try {
    [IO.Directory]::CreateDirectory($workRoot) | Out-Null
    $server = Start-Process -FilePath (Join-Path $PSHOME 'pwsh.exe') -ArgumentList @(
        '-NoProfile', '-File', (Join-Path $repoRoot 'scripts\serve-alpha-fixtures.ps1'),
        '-ReadyFile', $readyFile
    ) -WindowStyle Hidden -RedirectStandardOutput $serverOutput `
        -RedirectStandardError $serverError -PassThru
    $deadline = (Get-Date).AddSeconds(10)
    while (-not (Test-Path -LiteralPath $readyFile)) {
        if ($server.HasExited) { throw 'Timeout fixture server exited during startup.' }
        if ((Get-Date) -ge $deadline) { throw 'Timeout fixture server did not become ready.' }
        Start-Sleep -Milliseconds 50
    }
    $safeUrl = (Get-Content -LiteralPath $readyFile -Raw).Trim().TrimEnd('/') + '/hung-renderer.html'
    $url = $safeUrl + '?token=must-not-survive'
    $failure = $null
    try {
        & (Join-Path $repoRoot 'scripts\run-hidden-benchmark.ps1') `
            -Url $url -Output $reportPath -Browser $browserPath -Screenshot $screenshotPath `
            -SettleMs 100 -TimeoutSeconds 5 -FreshProfile
    } catch { $failure = $_ }
    if ($null -eq $failure) { throw 'The deliberately blocked renderer unexpectedly completed.' }
    if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) {
        throw 'The timeout did not preserve a JSON report.'
    }
    $record = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
    if ($record.harness_failure.kind -ne 'harness_timeout' -or
        -not [bool] $record.harness_failure.killed_by_harness -or
        [int] $record.harness_failure.timeout_seconds -ne 5) {
        throw 'The timeout report did not classify the harness failure.'
    }
    if (-not [bool] $record.headless -or -not [bool] $record.fresh_profile -or
        -not [bool] $record.isolated_profile -or [int] $record.http_status -ne 0) {
        throw 'The timeout report lost the hidden launch contract.'
    }
    if ([string] $record.requested_url -ne $safeUrl -or
        [string] $record.harness_failure.screenshot_status -ne 'unavailable_after_failure' -or
        [string]::IsNullOrWhiteSpace([string] $record.harness_failure.renderer_diagnostics.unavailable_reason)) {
        throw 'The timeout report did not preserve safe URL and screenshot/renderer availability diagnostics.'
    }
    if ([double] $record.harness_failure.elapsed_ms -lt 4500 -or
        [double] $record.harness_failure.elapsed_ms -gt 15000) {
        throw "The timeout was not bounded: $($record.harness_failure.elapsed_ms) ms."
    }
    $tree = @($record.harness_failure.process_tree)
    if ($tree.Count -eq 0 -or @($tree | Where-Object role -eq 'browser').Count -ne 1) {
        throw 'The timeout report did not preserve the launched browser process tree.'
    }
    if (@($tree | Where-Object role -eq 'renderer').Count -lt 1 -or
        @($tree | Where-Object state -eq 'running_at_failure').Count -ne 0 -or
        $null -eq $record.harness_failure.browser_exit_code) {
        throw 'The timeout report did not preserve final browser and renderer process state.'
    }
    foreach ($entry in $tree) {
        if ($null -eq $entry.exit_code -and
            [string]::IsNullOrWhiteSpace([string] $entry.exit_code_unavailable_reason)) {
            throw "Process $($entry.process_id) omitted both its exit code and an unavailable reason."
        }
    }
    if (@($record.harness_failure.remaining_process_ids).Count -ne 0) {
        throw 'The timeout report retained a process after cleanup.'
    }
    Start-Sleep -Milliseconds 200
    $survivors = @($tree | Where-Object {
        $null -ne (Get-Process -Id ([int] $_.process_id) -ErrorAction SilentlyContinue)
    })
    if ($survivors.Count -ne 0) { throw 'A hidden browser process survived timeout cleanup.' }
    [pscustomobject]@{
        failure_kind = [string] $record.harness_failure.kind
        elapsed_ms = [double] $record.harness_failure.elapsed_ms
        captured_processes = $tree.Count
        remaining_processes = 0
    }
} finally {
    if ($null -ne $server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
        $server.WaitForExit()
    }
    $fullWorkRoot = [IO.Path]::GetFullPath($workRoot)
    $expectedPrefix = Join-Path ([IO.Path]::GetTempPath()) 'breeze-timeout-test-'
    if (-not $fullWorkRoot.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to remove an unexpected timeout-test path.'
    }
    if (Test-Path -LiteralPath $fullWorkRoot) {
        Remove-Item -LiteralPath $fullWorkRoot -Recurse -Force
    }
}
