Set-StrictMode -Version Latest

function Start-HiddenTestProcess([Diagnostics.ProcessStartInfo] $Info) {
    if ($Info.UseShellExecute -or -not $Info.CreateNoWindow) { throw 'Refusing a visible test-process launch.' }
    [Diagnostics.Process]::Start($Info)
}

function Get-ExecutableTestNames([string] $Executable) {
    $info = [Diagnostics.ProcessStartInfo]::new($Executable)
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
    $info.Environment['BREEZE_REQUIRE_HIDDEN_BENCHMARK'] = '1'
    $info.ArgumentList.Add('--list'); $info.ArgumentList.Add('--format=terse')
    $process = Start-HiddenTestProcess $info
    try {
        $output = $process.StandardOutput.ReadToEndAsync()
        $errors = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $text = $output.GetAwaiter().GetResult()
        $errorText = $errors.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "Test discovery failed for ${Executable}: $errorText" }
        $names = @($text -split '\r?\n' | ForEach-Object { if ($_ -match '^(.+): test$') { $Matches[1] } })
        # Stable libtest's terse listing is empty for a zero-test harness.
        if (-not $names.Count -and -not [string]::IsNullOrWhiteSpace($text) -and $text -notmatch '(?m)^0 tests?, 0 benchmarks?\s*$') {
            throw "Unrecognized test discovery output from $Executable"
        }
        return $names
    } finally { $process.Dispose() }
}

function Start-FullTestWorker {
    param([string] $Name, [string] $Executable, [string[]] $Arguments,
        [AllowEmptyCollection()][string[]] $Expected, [string] $Directory, [switch] $Cpu)
    # Keep comfortably below Windows' command-line limit, including quoting.
    if (($Arguments -join ' ').Length + $Executable.Length -gt 28000) {
        throw 'Test filters exceed the command-line budget; use a more conservative partition.'
    }
    $info = [Diagnostics.ProcessStartInfo]::new($Executable)
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
    $info.Environment['BREEZE_REQUIRE_HIDDEN_BENCHMARK'] = '1'
    foreach ($argument in @('--test-threads=1', '--format=pretty', '--color=never') + $Arguments) {
        $info.ArgumentList.Add($argument)
    }
    $info.Environment.Remove('BREEZE_TEST_CPU_ONLY') | Out-Null
    if ($Cpu) { $info.Environment['BREEZE_TEST_CPU_ONLY'] = '1' }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-HiddenTestProcess $info
    [pscustomobject]@{
        Name = $Name; Process = $process; Clock = $clock; Cpu = [bool]$Cpu
        Started = [DateTimeOffset]::UtcNow.ToString('o'); Seconds = 0.0; Exit = $null
        Output = $process.StandardOutput.ReadToEndAsync(); Errors = $process.StandardError.ReadToEndAsync()
        Expected = $Expected; Directory = $Directory; Completed = $false; Passed = 0; Ignored = 0
    }
}

function Complete-FullTestWorker($Worker) {
    if ($Worker.Completed) { return }
    $Worker.Process.WaitForExit()
    $Worker.Seconds = $Worker.Clock.Elapsed.TotalSeconds
    $Worker.Exit = $Worker.Process.ExitCode
    $output = $Worker.Output.GetAwaiter().GetResult()
    $errors = $Worker.Errors.GetAwaiter().GetResult()
    $output | Set-Content -LiteralPath (Join-Path $Worker.Directory ($Worker.Name + '.stdout.log')) -Encoding utf8
    $errors | Set-Content -LiteralPath (Join-Path $Worker.Directory ($Worker.Name + '.stderr.log')) -Encoding utf8
    $Worker.Completed = $true
    if ($Worker.Exit -ne 0) {
        throw "Test worker $($Worker.Name) failed with exit $($Worker.Exit); inspect $($Worker.Directory)/$($Worker.Name).stdout.log and .stderr.log"
    }
    $observed = @([regex]::Matches($output, '(?m)^test (.+?)(?: - should panic)? \.\.\. (?:ok|ignored[^\r\n]*)\r?$') | ForEach-Object { $_.Groups[1].Value })
    $summary = [regex]::Match($output, 'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;')
    $remaining = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($name in $Worker.Expected) { $remaining.Add($name) | Out-Null }
    $exact = $true
    foreach ($name in $observed) { if (-not $remaining.Remove($name)) { $exact = $false } }
    if (-not $summary.Success -or $observed.Count -ne $Worker.Expected.Count -or -not $exact -or $remaining.Count) {
        throw "Test worker $($Worker.Name) did not execute its exact discovered partition."
    }
    $Worker.Passed = [int]$summary.Groups[1].Value
    $Worker.Ignored = [int]$summary.Groups[2].Value
    if ($Worker.Passed + $Worker.Ignored -ne $Worker.Expected.Count) { throw 'Test summary count differs from discovery.' }
    Write-Host ('{0}: {1:N2}s; {2} passed, {3} ignored' -f $Worker.Name, $Worker.Seconds, $Worker.Passed, $Worker.Ignored)
}

function Wait-FullTestWorker($Worker, [object[]] $Peers) {
    while (-not $Worker.Process.WaitForExit(200)) {
        foreach ($peer in $Peers) {
            if (-not $peer.Completed -and $peer.Process.HasExited) { Complete-FullTestWorker $peer }
        }
    }
    Complete-FullTestWorker $Worker
}
