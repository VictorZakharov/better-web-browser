param(
    [string[]] $Urls = @('https://example.org/'),
    [ValidateRange(1, 20)]
    [int] $Iterations = 3,
    [ValidateRange(100, 60000)]
    [int] $SettleMs = 2000,
    [string] $ChromiumProject = '',
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($ChromiumProject)) {
    $ChromiumProject = Join-Path (Split-Path -Parent $repoRoot) 'better-web-browser-chromium-baseline'
}
$ChromiumProject = [IO.Path]::GetFullPath($ChromiumProject)
$breezeExe = Join-Path $repoRoot 'target\release\better-web-browser.exe'
$chromiumDll = Join-Path $ChromiumProject 'bin\Release\net10.0\ChromiumBaseline.dll'

if (-not (Test-Path -LiteralPath $ChromiumProject -PathType Container)) {
    throw "Chromium baseline project not found: $ChromiumProject"
}

if (-not $SkipBuild) {
    & cargo build --release --manifest-path (Join-Path $repoRoot 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Breeze release build failed.' }
    & dotnet build (Join-Path $ChromiumProject 'ChromiumBaseline.csproj') -c Release
    if ($LASTEXITCODE -ne 0) { throw 'Chromium baseline release build failed.' }
}

$runName = Get-Date -Format 'yyyyMMdd-HHmmss'
$resultDirectory = Join-Path $repoRoot "benchmark-results\$runName"
$null = New-Item -ItemType Directory -Path $resultDirectory -Force
$records = [Collections.Generic.List[object]]::new()

function ConvertTo-Slug([string] $Value) {
    $slug = $Value -replace '^https?://', '' -replace '[^A-Za-z0-9]+', '-'
    return $slug.Trim('-').ToLowerInvariant()
}

function Quote-ProcessArgument([string] $Value) {
    return '"' + $Value.Replace('"', '\"') + '"'
}

function Invoke-BreezeRun([string] $Url, [string] $Slug, [int] $Iteration) {
    Write-Host "[$Iteration/$Iterations] Breeze   $Url"
    $output = Join-Path $resultDirectory "$Slug-breeze-$Iteration.json"
    $arguments = @(
        '--benchmark', (Quote-ProcessArgument $Url),
        '--output', (Quote-ProcessArgument $output),
        '--settle-ms', $SettleMs
    ) -join ' '
    $process = Start-Process -FilePath $breezeExe -ArgumentList $arguments -PassThru -Wait
    if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $output)) {
        throw "Breeze benchmark failed for $Url (exit $($process.ExitCode))."
    }
    $record = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
    $record | Add-Member -NotePropertyName iteration -NotePropertyValue $Iteration
    return $record
}

function Invoke-ChromiumRun([string] $Url, [string] $Slug, [int] $Iteration) {
    Write-Host "[$Iteration/$Iterations] Chromium $Url"
    $output = Join-Path $resultDirectory "$Slug-chromium-$Iteration.json"
    & dotnet $chromiumDll `
        --url $Url `
        --output $output `
        --settle-ms $SettleMs | Write-Host
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $output)) {
        throw "Chromium benchmark failed for $Url (exit $LASTEXITCODE)."
    }
    $record = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
    $record | Add-Member -NotePropertyName iteration -NotePropertyValue $Iteration
    return $record
}

foreach ($url in $Urls) {
    $slug = ConvertTo-Slug $url
    for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
        if ($iteration % 2 -eq 1) {
            $records.Add((Invoke-BreezeRun $url $slug $iteration))
            $records.Add((Invoke-ChromiumRun $url $slug $iteration))
        } else {
            $records.Add((Invoke-ChromiumRun $url $slug $iteration))
            $records.Add((Invoke-BreezeRun $url $slug $iteration))
        }
    }
}

function Get-Median([double[]] $Values) {
    $sorted = @($Values | Sort-Object)
    if ($sorted.Count % 2 -eq 1) {
        return [double]$sorted[[math]::Floor($sorted.Count / 2)]
    }
    $upper = $sorted.Count / 2
    return ([double]$sorted[$upper - 1] + [double]$sorted[$upper]) / 2
}

function Format-Milliseconds([double] $Value) { return '{0:N1} ms' -f $Value }
function Format-Mebibytes([double] $Value) { return '{0:N1} MiB' -f ($Value / 1MB) }
function Format-Ratio([double] $Baseline, [double] $Breeze) {
    if ($Breeze -le 0) { return 'n/a' }
    return '{0:N2}x' -f ($Baseline / $Breeze)
}

$summary = [Collections.Generic.List[object]]::new()
$markdown = [Text.StringBuilder]::new()
$null = $markdown.AppendLine('# Breeze vs Chromium benchmark')
$null = $markdown.AppendLine()
$null = $markdown.AppendLine("Generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')")
$null = $markdown.AppendLine()
$null = $markdown.AppendLine("Iterations: $Iterations; settle period: $SettleMs ms; each Chromium run uses a fresh temporary profile.")
$null = $markdown.AppendLine()

foreach ($url in $Urls) {
    $breezeRuns = @($records | Where-Object { $_.requested_url -eq $url -and $_.browser -eq 'breeze' })
    $chromiumRuns = @($records | Where-Object { $_.requested_url -eq $url -and $_.browser -eq 'chromium' })
    $breezeMedian = [ordered]@{
        window_ready_ms = Get-Median @($breezeRuns.window_ready_ms)
        page_ready_ms = Get-Median @($breezeRuns.page_ready_ms)
        navigation_ms = Get-Median @($breezeRuns.navigation_ms)
        working_set_bytes = Get-Median @($breezeRuns.working_set_bytes)
        private_bytes = Get-Median @($breezeRuns.private_bytes)
        cpu_time_ms = Get-Median @($breezeRuns.cpu_time_ms)
        process_count = Get-Median @($breezeRuns.process_count)
    }
    $chromiumMedian = [ordered]@{
        window_ready_ms = Get-Median @($chromiumRuns.window_ready_ms)
        page_ready_ms = Get-Median @($chromiumRuns.page_ready_ms)
        navigation_ms = Get-Median @($chromiumRuns.navigation_ms)
        working_set_bytes = Get-Median @($chromiumRuns.working_set_bytes)
        private_bytes = Get-Median @($chromiumRuns.private_bytes)
        cpu_time_ms = Get-Median @($chromiumRuns.cpu_time_ms)
        process_count = Get-Median @($chromiumRuns.process_count)
    }
    $summary.Add([ordered]@{ url = $url; breeze = $breezeMedian; chromium = $chromiumMedian })

    $null = $markdown.AppendLine("## $url")
    $null = $markdown.AppendLine()
    $null = $markdown.AppendLine('| Metric (median) | Breeze | Chromium | Chromium / Breeze |')
    $null = $markdown.AppendLine('|---|---:|---:|---:|')
    $null = $markdown.AppendLine("| Window ready | $(Format-Milliseconds $breezeMedian.window_ready_ms) | $(Format-Milliseconds $chromiumMedian.window_ready_ms) | $(Format-Ratio $chromiumMedian.window_ready_ms $breezeMedian.window_ready_ms) |")
    $null = $markdown.AppendLine("| Process start to page ready | $(Format-Milliseconds $breezeMedian.page_ready_ms) | $(Format-Milliseconds $chromiumMedian.page_ready_ms) | $(Format-Ratio $chromiumMedian.page_ready_ms $breezeMedian.page_ready_ms) |")
    $null = $markdown.AppendLine("| Navigation | $(Format-Milliseconds $breezeMedian.navigation_ms) | $(Format-Milliseconds $chromiumMedian.navigation_ms) | $(Format-Ratio $chromiumMedian.navigation_ms $breezeMedian.navigation_ms) |")
    $null = $markdown.AppendLine("| Working set | $(Format-Mebibytes $breezeMedian.working_set_bytes) | $(Format-Mebibytes $chromiumMedian.working_set_bytes) | $(Format-Ratio $chromiumMedian.working_set_bytes $breezeMedian.working_set_bytes) |")
    $null = $markdown.AppendLine("| Private memory | $(Format-Mebibytes $breezeMedian.private_bytes) | $(Format-Mebibytes $chromiumMedian.private_bytes) | $(Format-Ratio $chromiumMedian.private_bytes $breezeMedian.private_bytes) |")
    $null = $markdown.AppendLine("| CPU time | $(Format-Milliseconds $breezeMedian.cpu_time_ms) | $(Format-Milliseconds $chromiumMedian.cpu_time_ms) | $(Format-Ratio $chromiumMedian.cpu_time_ms $breezeMedian.cpu_time_ms) |")
    $null = $markdown.AppendLine("| Processes | $($breezeMedian.process_count) | $($chromiumMedian.process_count) | $(Format-Ratio $chromiumMedian.process_count $breezeMedian.process_count) |")
    $null = $markdown.AppendLine()
}

$null = $markdown.AppendLine('## Scope warning')
$null = $markdown.AppendLine()
$null = $markdown.AppendLine('Breeze currently downloads HTML and renders a safe readable-document subset. Chromium executes the complete page, including CSS, JavaScript, images, media, accessibility infrastructure, GPU services, and site-isolated subprocesses. The results compare the architectural fast path, not feature-equivalent browsers.')

$records | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $resultDirectory 'raw-results.json') -Encoding UTF8
$summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $resultDirectory 'summary.json') -Encoding UTF8
$markdown.ToString() | Set-Content -LiteralPath (Join-Path $resultDirectory 'REPORT.md') -Encoding UTF8

Write-Host "`nReport: $(Join-Path $resultDirectory 'REPORT.md')"
