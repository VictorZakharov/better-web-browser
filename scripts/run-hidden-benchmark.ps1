[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $Url,
    [Parameter(Mandatory)]
    [string] $Output,
    [string] $Browser,
    [string] $Screenshot,
    [string] $FilmstripDirectory,
    [ValidateRange(100, 10000)]
    [int] $FilmstripIntervalMs = 500,
    [ValidateRange(100, 60000)]
    [int] $FilmstripDurationMs = 10000,
    [ValidateRange(100, 60000)]
    [int] $SettleMs = 2000,
    [ValidateRange(5, 600)]
    [int] $TimeoutSeconds = 120,
    [ValidateRange(0, 120)]
    [int] $ScrollSamples = 0,
    [switch] $EarlyScrollTrace,
    [ValidateRange(320, 7680)]
    [int] $WindowWidth = 1280,
    [ValidateRange(240, 4320)]
    [int] $WindowHeight = 720,
    [ValidatePattern('^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*$')]
    [string] $Locale = 'en-US',
    [switch] $FreshProfile,
    [string] $ProfileDirectory,
    [string[]] $DiagnosticSelector = @(),
    [string[]] $NavigationTarget = @(),
    [string[]] $LinkActivationTarget = @(),
    [string[]] $SelectorActivationTarget = @(),
    [string[]] $ClickTarget = @(),
    [ValidateRange(0, 60000)]
    [int] $NavigationDelayMs = 0
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'hidden-benchmark-diagnostics.ps1')
if ([string]::IsNullOrWhiteSpace($Browser)) {
    $Browser = Join-Path $repoRoot 'target\release\better-web-browser.exe'
}
$Browser = (Resolve-Path $Browser).Path
$outputPath = [System.IO.Path]::GetFullPath($Output)
$screenshotPath = if ([string]::IsNullOrWhiteSpace($Screenshot)) {
    $null
} else {
    [System.IO.Path]::GetFullPath($Screenshot)
}
$profilePath = $null
if ($FreshProfile -and -not [string]::IsNullOrWhiteSpace($ProfileDirectory)) {
    throw 'Use either -FreshProfile or -ProfileDirectory, not both.'
}
if ($FreshProfile) {
    $profilePath = Join-Path ([IO.Path]::GetTempPath()) ("breeze-benchmark-" + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($profilePath) | Out-Null
} elseif (-not [string]::IsNullOrWhiteSpace($ProfileDirectory)) {
    $profilePath = [IO.Path]::GetFullPath($ProfileDirectory)
    if (-not [IO.Path]::IsPathFullyQualified($profilePath)) {
        throw '-ProfileDirectory must resolve to an absolute path.'
    }
}
$outputDirectory = Split-Path -Parent $outputPath
if (-not [string]::IsNullOrEmpty($outputDirectory)) {
    [System.IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
}
if (Test-Path -LiteralPath $outputPath) { Remove-Item -LiteralPath $outputPath -Force }
if ($null -ne $screenshotPath -and (Test-Path -LiteralPath $screenshotPath)) {
    Remove-Item -LiteralPath $screenshotPath -Force
}

function ConvertTo-WindowsCommandLineArgument {
    param([AllowEmptyString()][string] $Value)

    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') {
        return $Value
    }
    $builder = [System.Text.StringBuilder]::new()
    [void] $builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $slashes++
            continue
        }
        if ($character -eq '"') {
            [void] $builder.Append(('\' * ($slashes * 2 + 1)))
            [void] $builder.Append('"')
            $slashes = 0
            continue
        }
        if ($slashes -gt 0) {
            [void] $builder.Append(('\' * $slashes))
            $slashes = 0
        }
        [void] $builder.Append($character)
    }
    if ($slashes -gt 0) {
        [void] $builder.Append(('\' * ($slashes * 2)))
    }
    [void] $builder.Append('"')
    $builder.ToString()
}

$arguments = [System.Collections.Generic.List[string]]::new()
$arguments.Add('--benchmark')
$arguments.Add($Url)
$arguments.Add('--output')
$arguments.Add($outputPath)
$arguments.Add('--settle-ms')
$arguments.Add($SettleMs.ToString([System.Globalization.CultureInfo]::InvariantCulture))
$arguments.Add('--window-width')
$arguments.Add($WindowWidth.ToString([System.Globalization.CultureInfo]::InvariantCulture))
$arguments.Add('--window-height')
$arguments.Add($WindowHeight.ToString([System.Globalization.CultureInfo]::InvariantCulture))
if ($ScrollSamples -gt 0) {
    $arguments.Add('--scroll-samples')
    $arguments.Add($ScrollSamples.ToString([System.Globalization.CultureInfo]::InvariantCulture))
}
if ($EarlyScrollTrace) {
    $arguments.Add('--early-scroll-trace')
}
if (-not [string]::IsNullOrWhiteSpace($Screenshot)) {
    $arguments.Add('--screenshot')
    $arguments.Add($screenshotPath)
}
if (-not [string]::IsNullOrWhiteSpace($FilmstripDirectory)) {
    $filmstripPath = [System.IO.Path]::GetFullPath($FilmstripDirectory)
    $arguments.Add('--filmstrip-directory')
    $arguments.Add($filmstripPath)
    $arguments.Add('--filmstrip-interval-ms')
    $arguments.Add($FilmstripIntervalMs.ToString([System.Globalization.CultureInfo]::InvariantCulture))
    $arguments.Add('--filmstrip-duration-ms')
    $arguments.Add($FilmstripDurationMs.ToString([System.Globalization.CultureInfo]::InvariantCulture))
}
foreach ($selector in $DiagnosticSelector) {
    $arguments.Add('--diagnostic-selector')
    $arguments.Add($selector)
}
foreach ($target in $NavigationTarget) {
    if ([string]::IsNullOrWhiteSpace($target)) {
        throw '-NavigationTarget values cannot be empty.'
    }
    $arguments.Add('--navigate-after-ready')
    $arguments.Add($target)
}
foreach ($target in $LinkActivationTarget) {
    if ([string]::IsNullOrWhiteSpace($target)) {
        throw '-LinkActivationTarget values cannot be empty.'
    }
    $arguments.Add('--activate-link-after-ready')
    $arguments.Add($target)
}
foreach ($target in $SelectorActivationTarget) {
    if ([string]::IsNullOrWhiteSpace($target)) {
        throw '-SelectorActivationTarget values cannot be empty.'
    }
    $arguments.Add('--activate-selector-after-ready')
    $arguments.Add($target)
}
foreach ($target in $ClickTarget) {
    if ($target -notmatch '^\d+\s*,\s*\d+$') {
        throw '-ClickTarget values must use non-negative x,y coordinates.'
    }
    $arguments.Add('--click-after-ready')
    $arguments.Add($target)
}
if ($NavigationTarget.Count -gt 0 -or $LinkActivationTarget.Count -gt 0 -or
    $SelectorActivationTarget.Count -gt 0 -or $ClickTarget.Count -gt 0) {
    $arguments.Add('--navigation-delay-ms')
    $arguments.Add($NavigationDelayMs.ToString([System.Globalization.CultureInfo]::InvariantCulture))
} elseif ($NavigationDelayMs -ne 0) {
    throw '-NavigationDelayMs requires at least one navigation, link, selector, or click target.'
}

$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $Browser
$startInfo.Arguments = ($arguments | ForEach-Object {
    ConvertTo-WindowsCommandLineArgument $_
}) -join ' '
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
$startInfo.EnvironmentVariables['BREEZE_REQUIRE_HIDDEN_BENCHMARK'] = '1'
if ($null -ne $profilePath) {
    $startInfo.EnvironmentVariables['BREEZE_PROFILE_DIRECTORY'] = $profilePath
}
$startInfo.EnvironmentVariables['BREEZE_BENCHMARK_LOCALE'] = $Locale

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $startInfo
if (-not $process.Start()) {
    throw 'Could not start the hidden Breeze benchmark.'
}
$stdout = $process.StandardOutput.ReadToEndAsync()
$stderr = $process.StandardError.ReadToEndAsync()
$stopwatch = [Diagnostics.Stopwatch]::StartNew()
$timedOut = $false
$processTree = @()
$cleanup = [pscustomobject]@{
    killed_by_harness = $false
    process_tree_kill_supported = $false
    kill_error = $null
    remaining_process_ids = @()
}
try {
    $actual = Get-CimInstance Win32_Process -Filter "ProcessId = $($process.Id)"
    if ($null -eq $actual -or $actual.CommandLine -notmatch '(?i)(?:^|\s)--benchmark(?:\s|$)') {
        $processTree = @(Get-BenchmarkProcessTreeSnapshot -RootProcessId $process.Id)
        $cleanup = Stop-BenchmarkProcessTree -Process $process -ProcessTree $processTree
        throw 'Refusing benchmark run: the actual Breeze child command line lacks --benchmark.'
    }
    $processTree = @(Get-BenchmarkProcessTreeSnapshot -RootProcessId $process.Id)
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        $timedOut = $true
        $processTree = @(Get-BenchmarkProcessTreeSnapshot -RootProcessId $process.Id)
        $cleanup = Stop-BenchmarkProcessTree -Process $process -ProcessTree $processTree
    } else {
        # Flush redirected asynchronous reads after the bounded wait succeeds.
        $process.WaitForExit()
    }
} finally {
    if (-not $process.HasExited) {
        if ($processTree.Count -eq 0) {
            $processTree = @(Get-BenchmarkProcessTreeSnapshot -RootProcessId $process.Id)
        }
        $cleanup = Stop-BenchmarkProcessTree -Process $process -ProcessTree $processTree
    }
    if ($FreshProfile -and (Test-Path -LiteralPath $profilePath)) {
        $fullProfile = [IO.Path]::GetFullPath($profilePath)
        $expectedPrefix = Join-Path ([IO.Path]::GetTempPath()) 'breeze-benchmark-'
        if (-not $fullProfile.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove an unexpected Breeze benchmark profile path.'
        }
        Remove-Item -LiteralPath $fullProfile -Recurse -Force
    }
}
$stopwatch.Stop()

$standardOutput = $stdout.GetAwaiter().GetResult()
$standardError = $stderr.GetAwaiter().GetResult()
$browserExitCode = if ($process.HasExited) { $process.ExitCode } else { $null }
if ($null -ne $browserExitCode) {
    foreach ($entry in $processTree | Where-Object role -eq 'browser') {
        if (-not $timedOut) { $entry.state = 'exited' }
        $entry.exit_code = $browserExitCode
        $entry.exit_code_unavailable_reason = $null
    }
}
$failureReportArguments = @{
    OutputPath = $outputPath
    RequestedUrl = $Url
    Locale = $Locale
    FreshProfile = [bool] $FreshProfile
    IsolatedProfile = $null -ne $profilePath
    ScreenshotPath = $screenshotPath
    FailureKind = $null
    ErrorMessage = $null
    ElapsedMs = $stopwatch.Elapsed.TotalMilliseconds
    TimeoutSeconds = $TimeoutSeconds
    KilledByHarness = [bool] $cleanup.killed_by_harness
    BrowserProcessId = $process.Id
    BrowserExitCode = $browserExitCode
    ProcessTree = @($processTree)
    RemainingProcessIds = @($cleanup.remaining_process_ids)
    ProcessTreeKillSupported = [bool] $cleanup.process_tree_kill_supported
    KillError = $cleanup.kill_error
    Stdout = $standardOutput
    Stderr = $standardError
    ProfilePath = $profilePath
}
if ($timedOut) {
    $message = "Hidden Breeze benchmark exceeded the $TimeoutSeconds-second process timeout."
    $failureReportArguments.FailureKind = Get-BenchmarkFailureKind -TimedOut
    $failureReportArguments.ErrorMessage = $message
    Write-BenchmarkFailureReport @failureReportArguments
    throw $message
}
if ($process.ExitCode -ne 0) {
    $rawDetail = if ([string]::IsNullOrWhiteSpace($standardError)) {
        'no stderr was reported'
    } else {
        $standardError.Trim()
    }
    $detail = ConvertTo-BoundedBenchmarkDiagnostic -Value $rawDetail -ProfilePath $profilePath
    $message = "Hidden Breeze benchmark failed with exit code $($process.ExitCode): $detail"
    $failureReportArguments.FailureKind = Get-BenchmarkFailureKind -Stdout $standardOutput -Stderr $standardError
    $failureReportArguments.ErrorMessage = $message
    Write-BenchmarkFailureReport @failureReportArguments
    throw $message
}
if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
    $message = "Hidden Breeze benchmark did not create $outputPath."
    $failureReportArguments.FailureKind = 'missing_output'
    $failureReportArguments.ErrorMessage = $message
    Write-BenchmarkFailureReport @failureReportArguments
    throw $message
}
if (-not [string]::IsNullOrWhiteSpace($standardOutput)) {
    Write-Output $standardOutput.TrimEnd()
}
if (-not [string]::IsNullOrWhiteSpace($standardError)) {
    Write-Warning $standardError.Trim()
}
$record = Get-Content -LiteralPath $outputPath -Raw | ConvertFrom-Json
$record | Add-Member -NotePropertyName locale -NotePropertyValue $Locale
$record | Add-Member -NotePropertyName fresh_profile -NotePropertyValue ([bool] $FreshProfile)
$record | Add-Member -NotePropertyName isolated_profile -NotePropertyValue ($null -ne $profilePath)
$outcome = Get-BenchmarkHarnessOutcome -PageError ([string] $record.error)
$record | Add-Member -NotePropertyName harness_outcome -NotePropertyValue $outcome -Force
$record | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $outputPath -Encoding UTF8
Write-Output $outputPath
