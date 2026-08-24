[CmdletBinding()]
param(
    [string] $Matrix,
    [ValidateRange(1, 10)] [int] $Iterations = 3,
    [string[]] $Fixture = @(),
    [string] $OutputDirectory,
    [ValidateSet('debug', 'release', 'performance')]
    [string] $BuildProfile = 'release',
    [switch] $Live,
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($Matrix)) {
    $Matrix = Join-Path $PSScriptRoot 'alpha\matrix.json'
}
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$configuration = Get-Content -LiteralPath (Resolve-Path $Matrix) -Raw | ConvertFrom-Json
$cases = @($configuration.fixtures)
if ($Fixture.Count -gt 0) {
    $cases = @($cases | Where-Object { $_.id -in $Fixture })
    if ($cases.Count -ne $Fixture.Count) {
        throw 'One or more -Fixture values are not present in the alpha matrix.'
    }
}
if ($Live) {
    $cases = @($cases | Where-Object { -not [string]::IsNullOrWhiteSpace($_.live_url) })
}
if ($cases.Count -eq 0) { throw 'The selected alpha matrix is empty.' }

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $runName = Get-Date -Format 'yyyyMMdd-HHmmss'
    $OutputDirectory = Join-Path $repoRoot "benchmark-results\alpha-$runName"
}
$resultDirectory = [IO.Path]::GetFullPath($OutputDirectory)
[IO.Directory]::CreateDirectory($resultDirectory) | Out-Null

$breezeExe = Join-Path $repoRoot "target\$BuildProfile\better-web-browser.exe"
$chromiumProject = Join-Path $PSScriptRoot 'chromium\ChromiumBaseline.csproj'
$chromiumDll = Join-Path $PSScriptRoot 'chromium\bin\Release\net8.0\ChromiumBaseline.dll'
if (-not $SkipBuild) {
    $cargoProfile = if ($BuildProfile -eq 'release') {
        @('--release')
    } elseif ($BuildProfile -eq 'debug') {
        @()
    } else {
        @('--profile', $BuildProfile)
    }
    & cargo build @cargoProfile --locked --manifest-path (Join-Path $repoRoot 'Cargo.toml') --bin better-web-browser
    if ($LASTEXITCODE -ne 0) { throw "Breeze $BuildProfile build failed." }
    & dotnet build $chromiumProject -c Release
    if ($LASTEXITCODE -ne 0) { throw 'Chromium harness release build failed.' }
}
if (-not (Test-Path -LiteralPath $breezeExe -PathType Leaf)) { throw "Breeze executable not found: $breezeExe" }
if (-not (Test-Path -LiteralPath $chromiumDll -PathType Leaf)) { throw "Chromium harness not found: $chromiumDll" }

$server = $null
$baseUrl = $null
$serverReady = Join-Path $resultDirectory 'fixture-server.txt'
$serverOutput = Join-Path $resultDirectory 'fixture-server.out'
$serverError = Join-Path $resultDirectory 'fixture-server.err'
if (-not $Live) {
    # A forced shutdown cannot run the server's cleanup block, so never trust a
    # marker left by an earlier run in the same output directory.
    if (Test-Path -LiteralPath $serverReady) {
        Remove-Item -LiteralPath $serverReady -Force
    }
    $server = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
        (Join-Path $repoRoot 'scripts\serve-alpha-fixtures.ps1'), '-ReadyFile', $serverReady
    ) -WindowStyle Hidden -RedirectStandardOutput $serverOutput -RedirectStandardError $serverError -PassThru
    $deadline = (Get-Date).AddSeconds(10)
    while (-not (Test-Path -LiteralPath $serverReady)) {
        if ($server.HasExited) { throw 'The alpha fixture server exited during startup.' }
        if ((Get-Date) -gt $deadline) { throw 'The alpha fixture server did not become ready.' }
        Start-Sleep -Milliseconds 50
    }
    $baseUrl = (Get-Content -LiteralPath $serverReady -Raw).Trim()
}

function Get-Median {
    param([double[]] $Values)
    $sorted = @($Values | Sort-Object)
    if ($sorted.Count -eq 0) { return 0 }
    if ($sorted.Count % 2 -eq 1) { return [double] $sorted[[Math]::Floor($sorted.Count / 2)] }
    $upper = $sorted.Count / 2
    return ([double] $sorted[$upper - 1] + [double] $sorted[$upper]) / 2
}

function Assert-BreezeFixture {
    param($Record, [string] $Id)
    if ($null -ne $Record.error) { throw "$Id Breeze error: $($Record.error)" }
    if ([int] $Record.http_status -ne 200) { throw "$Id Breeze status was $($Record.http_status)." }
    if (@($Record.javascript_errors).Count -ne 0) { throw "$Id Breeze reported JavaScript errors." }
    if ([int] $Record.retained_draw_items -lt 10) { throw "$Id Breeze retained too few draw items." }
    $main = @($Record.diagnostics | Where-Object selector -eq '#main')
    if ($main.Count -ne 1 -or [int] $main[0].total_matches -ne 1) { throw "$Id Breeze did not produce exactly one #main region." }
    $match = @($main[0].matches)[0]
    if ([int] $match.text_length -lt 40 -or $match.style.display -eq 'none' -or -not $match.style.visibility) {
        throw "$Id Breeze collapsed or emptied its major content region."
    }
}

$records = [Collections.Generic.List[object]]::new()
try {
    foreach ($case in $cases) {
        $url = if ($Live) { [string] $case.live_url } else { $baseUrl.TrimEnd('/') + [string] $case.path }
        for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
          try {
            Write-Host "[$($case.id) $iteration/$Iterations] Breeze"
            $prefix = "$($case.id)-$iteration"
            $breezeJson = Join-Path $resultDirectory "$prefix-breeze.json"
            $breezePng = Join-Path $resultDirectory "$prefix-breeze.png"
            $breezeArguments = @{
                Url = $url
                Output = $breezeJson
                Browser = $breezeExe
                Screenshot = $breezePng
                SettleMs = [int] $configuration.settle_ms
                TimeoutSeconds = $(if ($Live) { 45 } else { 90 })
                ScrollSamples = [int] $configuration.scroll_samples
                WindowWidth = [int] $configuration.viewport.window_width
                WindowHeight = [int] $configuration.viewport.window_height
                Locale = [string] $configuration.locale
                FreshProfile = $true
                DiagnosticSelector = @('#main')
            }
            if ([bool] $case.early_scroll) { $breezeArguments.EarlyScrollTrace = $true }
            & (Join-Path $repoRoot 'scripts\run-hidden-benchmark.ps1') @breezeArguments | Write-Host
            $breeze = Get-Content -LiteralPath $breezeJson -Raw | ConvertFrom-Json
            if ($Live -and ($null -ne $breeze.error -or [int] $breeze.http_status -lt 200 -or [int] $breeze.http_status -ge 400)) {
                throw "$($case.id) Breeze live navigation failed: status $($breeze.http_status), $($breeze.error)"
            }
            if (-not $Live) { Assert-BreezeFixture $breeze $case.id }

            $viewportWidth = [int] [Math]::Round([double] $breeze.viewport_width_css_px)
            $viewportHeight = [int] [Math]::Round([double] $breeze.viewport_height_css_px)
            $scale = [double] $breeze.device_scale_factor
            Write-Host "[$($case.id) $iteration/$Iterations] Chromium"
            $chromiumJson = Join-Path $resultDirectory "$prefix-chromium.json"
            $chromiumPng = Join-Path $resultDirectory "$prefix-chromium.png"
            $chromiumArguments = @(
                $chromiumDll, '--url', $url, '--output', $chromiumJson,
                '--screenshot', $chromiumPng, '--viewport-width', $viewportWidth,
                '--viewport-height', $viewportHeight, '--device-scale-factor',
                $scale.ToString([Globalization.CultureInfo]::InvariantCulture),
                '--locale', [string] $configuration.locale, '--settle-ms',
                [string] $configuration.settle_ms, '--scroll-samples',
                [string] $configuration.scroll_samples, '--timeout-ms',
                $(if ($Live) { '45000' } else { '30000' })
            )
            if ([bool] $case.early_scroll) { $chromiumArguments += '--early-scroll' }
            if (-not $Live) { $chromiumArguments += '--require-fixture-ready' }
            & dotnet @chromiumArguments | Write-Host
            if ($LASTEXITCODE -ne 0) { throw "$($case.id) Chromium run failed." }
            $chromium = Get-Content -LiteralPath $chromiumJson -Raw | ConvertFrom-Json

            if (-not $chromium.headless -or -not $chromium.unified_headless -or
                -not $chromium.fresh_profile -or -not $chromium.cache_disabled) {
                throw "$($case.id) Chromium did not attest to the required hidden fresh-profile contract."
            }
            # Breeze can expose fractional CSS pixels at 125% Windows scaling,
            # while CDP accepts integer viewport dimensions and can quantize once
            # more to device pixels. Two CSS pixels is the maximum equivalence
            # tolerance; scale and locale still have to match exactly below.
            if ([Math]::Abs([double] $breeze.viewport_width_css_px - [double] $chromium.viewport_width_css_px) -gt 2 -or
                [Math]::Abs([double] $breeze.viewport_height_css_px - [double] $chromium.viewport_height_css_px) -gt 2 -or
                [Math]::Abs($scale - [double] $chromium.device_scale_factor) -gt 0.001 -or
                $breeze.locale -ne $chromium.locale) {
                throw "$($case.id) viewport, scale, or locale contract diverged."
            }

            $visual = $null
            $pageReadyPassed = $true
            $earlyScrollPassed = $true
            $visualCeiling = if ($Live) { 1.0 } else { [double] $case.max_visual_difference }
            $visual = & (Join-Path $PSScriptRoot 'visual-compare.ps1') `
                -BreezeScreenshot $breezePng -ChromiumScreenshot $chromiumPng `
                -BreezeReport $breezeJson -MaximumDifference $visualCeiling
            if (-not $Live) {
                $pageReadyPassed = [double] $breeze.page_ready_ms -le 2 * [double] $chromium.page_ready_ms
                if (-not $pageReadyPassed) { throw "$($case.id) Breeze page-ready exceeded 2x Chromium." }
                if ([bool] $case.early_scroll) {
                    $earlyScrollPassed = [bool] $breeze.early_scroll_trace.summary.meets_acceptance
                    if (-not $earlyScrollPassed) { throw "$($case.id) failed the Breeze early-scroll acceptance gate." }
                }
            }
            $records.Add([pscustomobject]@{
                fixture = [string] $case.id
                role = [string] $case.role
                url = $url
                iteration = $iteration
                live = [bool] $Live
                error = $null
                page_ready_passed = $pageReadyPassed
                early_scroll_passed = $earlyScrollPassed
                visual = $visual
                breeze = $breeze
                chromium = $chromium
            })
          } catch {
              if (-not $Live) { throw }
              $message = $_.Exception.Message
              Write-Warning "$($case.id) live sample $iteration failed: $message"
              $records.Add([pscustomobject]@{
                  fixture = [string] $case.id
                  role = [string] $case.role
                  url = $url
                  iteration = $iteration
                  live = $true
                  error = $message
              })
          }
        }
    }
} finally {
    if ($null -ne $server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
        $server.WaitForExit()
    }
}

$summary = [Collections.Generic.List[object]]::new()
foreach ($case in $cases) {
    $allRuns = @($records | Where-Object fixture -eq $case.id)
    $runs = @($allRuns | Where-Object { $null -eq $_.error })
    $summary.Add([pscustomobject]@{
        fixture = [string] $case.id
        role = [string] $case.role
        iterations = $runs.Count
        failed_iterations = $allRuns.Count - $runs.Count
        breeze_page_ready_ms = Get-Median @($runs.breeze.page_ready_ms)
        chromium_page_ready_ms = Get-Median @($runs.chromium.page_ready_ms)
        breeze_first_usable_paint_ms = Get-Median @($runs.breeze.page_ready_ms)
        chromium_first_usable_paint_ms = Get-Median @($runs.chromium.first_usable_paint_ms)
        breeze_javascript_ms = Get-Median @($runs.breeze.javascript_ms)
        chromium_javascript_ms = Get-Median @($runs.chromium.javascript_ms)
        breeze_style_ms = Get-Median @($runs.breeze.style_refresh_ms)
        chromium_style_ms = Get-Median @($runs.chromium.style_refresh_ms)
        breeze_layout_paint_ms = Get-Median @($runs.breeze.layout_and_paint_ms)
        chromium_layout_ms = Get-Median @($runs.chromium.layout_ms)
        chromium_paint_capture_ms = Get-Median @($runs.chromium.paint_capture_ms)
        breeze_scroll_average_ms = Get-Median @($runs.breeze.average_scroll_paint_ms)
        breeze_scroll_maximum_ms = Get-Median @($runs.breeze.maximum_scroll_paint_ms)
        chromium_scroll_average_ms = Get-Median @($runs.chromium.steady_scroll.average_ms)
        chromium_scroll_maximum_ms = Get-Median @($runs.chromium.steady_scroll.maximum_ms)
        breeze_working_set_bytes = Get-Median @($runs.breeze.working_set_bytes)
        chromium_working_set_bytes = Get-Median @($runs.chromium.working_set_bytes)
        breeze_private_bytes = Get-Median @($runs.breeze.private_bytes)
        chromium_private_bytes = Get-Median @($runs.chromium.private_bytes)
        breeze_cpu_time_ms = Get-Median @($runs.breeze.cpu_time_ms)
        chromium_cpu_time_ms = Get-Median @($runs.chromium.cpu_time_ms)
        breeze_processes = Get-Median @($runs.breeze.process_count)
        chromium_processes = Get-Median @($runs.chromium.process_count)
        visual_difference = Get-Median @($runs.visual.perceptual_difference)
    })
}

$records | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $resultDirectory 'raw-results.json') -Encoding UTF8
$summary | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $resultDirectory 'summary.json') -Encoding UTF8
$firstChromium = @($records | Where-Object { $null -ne $_.chromium } | Select-Object -First 1)
$environment = [pscustomobject]@{
    generated_at = [DateTimeOffset]::Now.ToString('o')
    commit = (& git -c "safe.directory=$repoRoot" -C $repoRoot rev-parse HEAD).Trim()
    os = [Environment]::OSVersion.VersionString
    processors = [Environment]::ProcessorCount
    locale = [string] $configuration.locale
    local_fixtures = -not [bool] $Live
    breeze_build_profile = $BuildProfile
    chromium_version = if ($firstChromium.Count -eq 0) { $null } else { [string] $firstChromium[0].chromium.chrome_version }
}
$environment | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $resultDirectory 'environment.json') -Encoding UTF8

$markdown = [Text.StringBuilder]::new()
[void] $markdown.AppendLine('# Public alpha compatibility comparison')
[void] $markdown.AppendLine()
[void] $markdown.AppendLine("Mode: $(if ($Live) { 'opt-in live evidence' } else { 'deterministic loopback gate' }); Breeze profile: $BuildProfile; iterations: $Iterations; locale: $($configuration.locale).")
[void] $markdown.AppendLine()
[void] $markdown.AppendLine('| Fixture | Samples | Breeze ready | Chromium ready | Breeze layout/paint | Chromium layout/capture | Breeze/Chromium memory | Visual diff |')
[void] $markdown.AppendLine('|---|---:|---:|---:|---:|---:|---:|---:|')
foreach ($row in $summary) {
    $memory = '{0:N1}/{1:N1} MiB' -f ($row.breeze_working_set_bytes / 1MB), ($row.chromium_working_set_bytes / 1MB)
    $visual = '{0:N3}' -f $row.visual_difference
    $chromiumRender = '{0:N1}/{1:N1} ms' -f $row.chromium_layout_ms, $row.chromium_paint_capture_ms
    [void] $markdown.AppendLine("| $($row.fixture) | $($row.iterations)/$($row.iterations + $row.failed_iterations) | $([Math]::Round($row.breeze_page_ready_ms,1)) ms | $([Math]::Round($row.chromium_page_ready_ms,1)) ms | $([Math]::Round($row.breeze_layout_paint_ms,1)) ms | $chromiumRender | $memory | $visual |")
}
[void] $markdown.AppendLine()
[void] $markdown.AppendLine('| Fixture | Breeze/Chromium JS | Breeze/Chromium style | Breeze scroll avg/max | Chromium scroll avg/max | Breeze/Chromium CPU | Processes |')
[void] $markdown.AppendLine('|---|---:|---:|---:|---:|---:|---:|')
foreach ($row in $summary) {
    $javascript = '{0:N1}/{1:N1} ms' -f $row.breeze_javascript_ms, $row.chromium_javascript_ms
    $style = '{0:N1}/{1:N1} ms' -f $row.breeze_style_ms, $row.chromium_style_ms
    $breezeScroll = '{0:N1}/{1:N1} ms' -f $row.breeze_scroll_average_ms, $row.breeze_scroll_maximum_ms
    $chromiumScroll = '{0:N1}/{1:N1} ms' -f $row.chromium_scroll_average_ms, $row.chromium_scroll_maximum_ms
    $cpu = '{0:N1}/{1:N1} ms' -f $row.breeze_cpu_time_ms, $row.chromium_cpu_time_ms
    [void] $markdown.AppendLine("| $($row.fixture) | $javascript | $style | $breezeScroll | $chromiumScroll | $cpu | $($row.breeze_processes)/$($row.chromium_processes) |")
}
[void] $markdown.AppendLine()
$failures = @($records | Where-Object { $null -ne $_.error })
if ($failures.Count -gt 0) {
    [void] $markdown.AppendLine('## Bounded live-run failures')
    [void] $markdown.AppendLine()
    [void] $markdown.AppendLine('| Fixture | Iteration | Diagnostic |')
    [void] $markdown.AppendLine('|---|---:|---|')
    foreach ($failure in $failures) {
        $diagnostic = ([string] $failure.error).Replace('|', '\|').Replace("`r", ' ').Replace("`n", ' ')
        [void] $markdown.AppendLine("| $($failure.fixture) | $($failure.iteration) | $diagnostic |")
    }
    [void] $markdown.AppendLine()
}
[void] $markdown.AppendLine('Breeze page-ready follows its first owned layout and paint; Chromium ready follows load, while first usable paint is recorded separately in raw results. Local fixture gates require nonblank captures, intact major content, no Breeze script errors, Breeze page-ready no slower than 2x Chromium, and per-fixture perceptual thresholds. Live captures are checked only for nonblank surfaces and their visual differences are diagnostics, not thresholds, because third-party content and networks vary.')
$markdown.ToString() | Set-Content -LiteralPath (Join-Path $resultDirectory 'REPORT.md') -Encoding UTF8
Write-Output (Join-Path $resultDirectory 'REPORT.md')
