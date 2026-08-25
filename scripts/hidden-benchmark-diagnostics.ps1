Set-StrictMode -Version Latest

$script:MaximumBenchmarkDiagnosticCharacters = 16 * 1024

function ConvertTo-SafeBenchmarkUrl {
    param([Parameter(Mandatory)] [string] $Value)

    try {
        $builder = [UriBuilder]::new([Uri] $Value)
        $builder.UserName = ''
        $builder.Password = ''
        $builder.Query = ''
        $builder.Fragment = ''
        return $builder.Uri.AbsoluteUri
    } catch {
        return '[invalid-url]'
    }
}

function ConvertTo-BoundedBenchmarkDiagnostic {
    param(
        [AllowNull()] [string] $Value,
        [AllowNull()] [string] $ProfilePath
    )

    if ([string]::IsNullOrWhiteSpace($Value)) { return $null }
    $sanitized = $Value
    if (-not [string]::IsNullOrWhiteSpace($ProfilePath)) {
        $sanitized = $sanitized.Replace($ProfilePath, '[profile]', [StringComparison]::OrdinalIgnoreCase)
    }
    $sanitized = [regex]::Replace(
        $sanitized,
        '(?i)\b(https?://)([^/\s:@]+):([^@\s/]+)@',
        '$1[credentials-redacted]@'
    )
    $sanitized = [regex]::Replace(
        $sanitized,
        '(?i)\b(https?://[^\s?#]+)\?[^\s#]*',
        '$1?[query-redacted]'
    )
    $sanitized = [regex]::Replace(
        $sanitized,
        '(?im)\b(authorization|proxy-authorization|cookie|set-cookie|password|passwd|token|api[_-]?key)\s*[:=]\s*[^\r\n]+',
        '$1: [redacted]'
    )
    if ($sanitized.Length -gt $script:MaximumBenchmarkDiagnosticCharacters) {
        $sanitized = $sanitized.Substring(0, $script:MaximumBenchmarkDiagnosticCharacters) + "`n[truncated]"
    }
    return $sanitized
}

function Get-BenchmarkProcessTreeSnapshot {
    param([Parameter(Mandatory)] [int] $RootProcessId)

    $processes = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $tracked = [Collections.Generic.HashSet[int]]::new()
    [void] $tracked.Add($RootProcessId)
    do {
        $changed = $false
        foreach ($candidate in $processes) {
            if ($tracked.Contains([int] $candidate.ParentProcessId) -and
                $tracked.Add([int] $candidate.ProcessId)) {
                $changed = $true
            }
        }
    } while ($changed)

    $snapshot = [Collections.Generic.List[object]]::new()
    foreach ($candidate in $processes | Where-Object { $tracked.Contains([int] $_.ProcessId) }) {
        $role = if ([int] $candidate.ProcessId -eq $RootProcessId) {
            'browser'
        } elseif ([string] $candidate.CommandLine -match '(?i)(?:^|\s)--renderer-process(?:\s|$)') {
            'renderer'
        } else {
            'child'
        }
        $workingSet = $null
        $privateBytes = $null
        $cpuTimeMs = $null
        try {
            $process = Get-Process -Id ([int] $candidate.ProcessId) -ErrorAction Stop
            $workingSet = [long] $process.WorkingSet64
            $privateBytes = [long] $process.PrivateMemorySize64
            $cpuTimeMs = [Math]::Round([double] $process.CPU * 1000, 3)
        } catch {}
        $startedUtc = if ($null -ne $candidate.CreationDate) {
            $candidate.CreationDate.ToUniversalTime().ToString('o')
        } else { $null }
        $snapshot.Add([ordered]@{
            process_id = [int] $candidate.ProcessId
            parent_process_id = [int] $candidate.ParentProcessId
            role = $role
            name = [string] $candidate.Name
            state = 'running_at_failure'
            started_utc = $startedUtc
            exit_code = $null
            exit_code_unavailable_reason = 'process was still running when captured'
            working_set_bytes = $workingSet
            private_bytes = $privateBytes
            cpu_time_ms = $cpuTimeMs
        })
    }
    return @($snapshot)
}

function Get-RemainingBenchmarkProcessIds {
    param([Parameter(Mandatory)] [AllowEmptyCollection()] [object[]] $ProcessTree)

    return @($ProcessTree | ForEach-Object {
        $processId = [int] $_.process_id
        if ($null -ne (Get-Process -Id $processId -ErrorAction SilentlyContinue)) { $processId }
    })
}

function Stop-BenchmarkProcessTree {
    param(
        [Parameter(Mandatory)] [Diagnostics.Process] $Process,
        [Parameter(Mandatory)] [AllowEmptyCollection()] [object[]] $ProcessTree
    )

    $observed = @{}
    foreach ($entry in $ProcessTree) {
        $processId = [int] $entry.process_id
        try { $observed[$processId] = Get-Process -Id $processId -ErrorAction Stop } catch {}
    }

    $killed = -not $Process.HasExited
    $treeKillSupported = $false
    $killErrors = [Collections.Generic.List[string]]::new()
    if (-not $Process.HasExited) {
        try {
            $Process.Kill($true)
            $treeKillSupported = $true
        } catch {
            $killErrors.Add($_.Exception.Message)
            foreach ($entry in $ProcessTree | Where-Object {
                [int] $_.process_id -ne $Process.Id
            } | Sort-Object process_id -Descending) {
                try {
                    $processId = [int] $entry.process_id
                    if ($observed.ContainsKey($processId) -and -not $observed[$processId].HasExited) {
                        $observed[$processId].Kill()
                    }
                } catch {
                    if ($_.Exception -isnot [ArgumentException]) {
                        $killErrors.Add($_.Exception.Message)
                    }
                }
            }
            try { $Process.Kill() } catch { $killErrors.Add($_.Exception.Message) }
        }
    }
    if (-not $Process.HasExited -and -not $Process.WaitForExit(5000)) {
        $killErrors.Add('browser process did not exit within five seconds after termination')
    }

    foreach ($entry in $ProcessTree) {
        $processId = [int] $entry.process_id
        if (-not $observed.ContainsKey($processId)) { continue }
        try {
            $observedProcess = $observed[$processId]
            if (-not $observedProcess.HasExited) {
                [void] $observedProcess.WaitForExit(1000)
            }
            if ($observedProcess.HasExited) {
                $entry.state = if ($killed) { 'terminated_by_harness' } else { 'exited' }
                $exitCode = $observedProcess.ExitCode
                $entry.exit_code = $exitCode
                $entry.exit_code_unavailable_reason = if ($null -eq $exitCode) {
                    'the operating system did not expose an exit code after process-tree termination'
                } else { $null }
            }
        } catch {
            $entry.state = if ($killed) { 'terminated_by_harness' } else { 'exited' }
            $entry.exit_code_unavailable_reason =
                'the operating system did not expose an exit code after process-tree termination'
        }
    }

    $deadline = (Get-Date).AddSeconds(3)
    do {
        $remaining = @(Get-RemainingBenchmarkProcessIds -ProcessTree $ProcessTree)
        if ($remaining.Count -eq 0 -or (Get-Date) -ge $deadline) { break }
        Start-Sleep -Milliseconds 50
    } while ($true)
    return [pscustomobject]@{
        killed_by_harness = $killed
        process_tree_kill_supported = $treeKillSupported
        kill_error = if ($killErrors.Count -eq 0) { $null } else { $killErrors -join '; ' }
        remaining_process_ids = @($remaining)
    }
}

function Get-BenchmarkFailureKind {
    param(
        [switch] $TimedOut,
        [AllowNull()] [string] $Stdout,
        [AllowNull()] [string] $Stderr
    )

    if ($TimedOut) { return 'harness_timeout' }
    $diagnostics = "$Stdout`n$Stderr"
    if ($diagnostics -match '(?i)renderer broker (?:has )?exited|renderer broker exit') {
        return 'renderer_broker_exit'
    }
    if ($diagnostics -match '(?i)renderer.*(?:unresponsive|watchdog|exceeded.*budget|exit 0x4a)') {
        return 'renderer_watchdog_exit'
    }
    return 'browser_exit'
}

function Get-BenchmarkHarnessOutcome {
    param([AllowNull()] [string] $PageError)

    $ordinaryPageError = -not [string]::IsNullOrWhiteSpace($PageError)
    return [pscustomobject] [ordered]@{
        outcome = if ($ordinaryPageError) { 'page_error' } else { 'success' }
        failure_kind = if ($ordinaryPageError) { 'ordinary_page_error' } else { $null }
        timed_out = $false
        killed_by_harness = $false
    }
}

function Set-BenchmarkReportProperty {
    param(
        [Parameter(Mandatory)] $Record,
        [Parameter(Mandatory)] [string] $Name,
        [AllowNull()] $Value
    )

    $Record | Add-Member -NotePropertyName $Name -NotePropertyValue $Value -Force
}

function Write-BenchmarkFailureReport {
    param(
        [Parameter(Mandatory)] [string] $OutputPath,
        [Parameter(Mandatory)] [string] $RequestedUrl,
        [Parameter(Mandatory)] [string] $Locale,
        [Parameter(Mandatory)] [bool] $FreshProfile,
        [Parameter(Mandatory)] [bool] $IsolatedProfile,
        [AllowNull()] [string] $ScreenshotPath,
        [Parameter(Mandatory)] [string] $FailureKind,
        [Parameter(Mandatory)] [string] $ErrorMessage,
        [Parameter(Mandatory)] [double] $ElapsedMs,
        [Parameter(Mandatory)] [int] $TimeoutSeconds,
        [Parameter(Mandatory)] [bool] $KilledByHarness,
        [Parameter(Mandatory)] [int] $BrowserProcessId,
        [AllowNull()] $BrowserExitCode,
        [Parameter(Mandatory)] [AllowEmptyCollection()] [object[]] $ProcessTree,
        [Parameter(Mandatory)] [AllowEmptyCollection()] [int[]] $RemainingProcessIds,
        [Parameter(Mandatory)] [bool] $ProcessTreeKillSupported,
        [AllowNull()] [string] $KillError,
        [AllowNull()] [string] $Stdout,
        [AllowNull()] [string] $Stderr,
        [AllowNull()] [string] $ProfilePath
    )

    $record = $null
    if (Test-Path -LiteralPath $OutputPath -PathType Leaf) {
        try { $record = Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json } catch {}
    }
    if ($null -eq $record) {
        $record = [pscustomobject] [ordered]@{
            browser = 'breeze'
            headless = $true
            requested_url = ConvertTo-SafeBenchmarkUrl -Value $RequestedUrl
            final_url = $null
            error = $ErrorMessage
            http_status = 0
            locale = $Locale
            fresh_profile = $FreshProfile
            isolated_profile = $IsolatedProfile
            screenshot = $null
        }
    } else {
        Set-BenchmarkReportProperty -Record $record -Name error -Value $ErrorMessage
        Set-BenchmarkReportProperty -Record $record -Name locale -Value $Locale
        Set-BenchmarkReportProperty -Record $record -Name fresh_profile -Value $FreshProfile
        Set-BenchmarkReportProperty -Record $record -Name isolated_profile -Value $IsolatedProfile
    }
    foreach ($urlProperty in @('requested_url', 'final_url')) {
        $property = $record.PSObject.Properties[$urlProperty]
        if ($null -ne $property -and -not [string]::IsNullOrWhiteSpace([string] $property.Value)) {
            Set-BenchmarkReportProperty -Record $record -Name $urlProperty `
                -Value (ConvertTo-SafeBenchmarkUrl -Value ([string] $property.Value))
        }
    }

    $screenshotStatus = if ([string]::IsNullOrWhiteSpace($ScreenshotPath)) {
        'not_requested'
    } elseif (Test-Path -LiteralPath $ScreenshotPath -PathType Leaf) {
        'captured_before_failure'
    } else {
        'unavailable_after_failure'
    }
    if ($screenshotStatus -eq 'captured_before_failure') {
        Set-BenchmarkReportProperty -Record $record -Name screenshot -Value $ScreenshotPath
    }
    Set-BenchmarkReportProperty -Record $record -Name harness_failure -Value ([pscustomobject] [ordered]@{
        kind = $FailureKind
        elapsed_ms = [Math]::Round($ElapsedMs, 3)
        timeout_seconds = $TimeoutSeconds
        killed_by_harness = $KilledByHarness
        browser_process_id = $BrowserProcessId
        browser_exit_code = $BrowserExitCode
        process_tree = @($ProcessTree)
        process_tree_unavailable_reason = if ($ProcessTree.Count -eq 0) {
            'Windows process inspection returned no launched-process snapshot'
        } else { $null }
        remaining_process_ids = @($RemainingProcessIds)
        process_tree_kill_supported = $ProcessTreeKillSupported
        kill_error = $KillError
        screenshot_status = $screenshotStatus
        stdout = ConvertTo-BoundedBenchmarkDiagnostic -Value $Stdout -ProfilePath $ProfilePath
        stderr = ConvertTo-BoundedBenchmarkDiagnostic -Value $Stderr -ProfilePath $ProfilePath
        renderer_diagnostics = [pscustomobject] [ordered]@{
            last_progress_utc = $null
            active_task = $null
            queue_depths = $null
            unavailable_reason = 'the browser did not expose live renderer diagnostics before termination'
        }
    })

    $json = $record | ConvertTo-Json -Depth 20
    $temporary = $OutputPath + '.partial-' + [Guid]::NewGuid().ToString('N')
    [IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination $OutputPath -Force
}
