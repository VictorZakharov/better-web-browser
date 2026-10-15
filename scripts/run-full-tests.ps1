[CmdletBinding()]
param([string] $ReportDirectory, [ValidateRange(1, 3)][int] $CpuWorkers = 3,
    [ValidateRange(1, 2)][int] $BrowserWorkers = 2)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'full-test-plan.ps1')
. (Join-Path $PSScriptRoot 'full-test-process.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$target = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) } else { Join-Path $repo 'target' }
if (-not $IsWindows -or [IO.Path]::GetPathRoot($target) -ne 'G:\') { throw 'Full local verification requires Windows and a G: target directory.' }
if (-not $ReportDirectory) { $ReportDirectory = Join-Path $target ('test-reports/' + [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')) }
$ReportDirectory = [IO.Path]::GetFullPath($ReportDirectory)
if ([IO.Path]::GetPathRoot($ReportDirectory) -ne 'G:\') { throw 'Test reports must stay on G:.' }
[IO.Directory]::CreateDirectory($ReportDirectory) | Out-Null
$clock = [Diagnostics.Stopwatch]::StartNew()
$workers = [Collections.Generic.List[object]]::new()
$success = $false
$compileSeconds = 0.0
Push-Location -LiteralPath $repo
try {
    $buildMessages = Join-Path $ReportDirectory 'build.jsonl'
    $compileClock = [Diagnostics.Stopwatch]::StartNew()
    & cargo test --locked --all-targets --no-run --message-format=json --timings > $buildMessages 2> (Join-Path $ReportDirectory 'build.log')
    $compileSeconds = $compileClock.Elapsed.TotalSeconds
    if ($LASTEXITCODE -ne 0) { throw "Full test compilation failed; inspect $ReportDirectory/build.log and build.jsonl" }
    $artifacts = @(Get-Content -LiteralPath $buildMessages | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object {
        $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable
    })
    $library = @($artifacts | Where-Object { $_.target.kind -contains 'lib' })
    if ($library.Count -ne 1 -or $library[0].target.name -ne 'better_web_browser') { throw 'Expected exactly one workspace library test harness.' }
    foreach ($artifact in $artifacts) {
        if (@($artifact.target.kind | Where-Object { $_ -notin @('lib', 'bin', 'test', 'example') }).Count) {
            throw "Unclassified executable kind: $($artifact.target.kind). Extend the complete runner before using it."
        }
    }
    $names = @(Get-ExecutableTestNames $library[0].executable)
    $plan = Get-FullLibraryTestPlan -Names $names -CpuWorkers $CpuWorkers
    $plan | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'library-plan.json') -Encoding utf8
    $cpu = @(foreach ($shard in $plan.Cpu) {
        if ($shard.Names.Count) {
            $worker = Start-FullTestWorker -Name $shard.Name -Executable $library[0].executable -Arguments $shard.Prefixes.ToArray() -Expected $shard.Names.ToArray() -Directory $ReportDirectory -Cpu
            $workers.Add($worker)
            $worker
        }
    })
    # Exactly one serial native worker; CPU processes cannot initialize our
    # ANGLE/D3D devices. Unknown tests remain here and no test is omitted.
    $skip = @(foreach ($prefix in @($plan.CpuPrefixes) + @($plan.ExclusiveNames)) { '--skip'; $prefix })
    $nativeNames = @($plan.NativeNames | Where-Object { $_ -cnotin $plan.ExclusiveNames })
    if ($nativeNames.Count) {
        $worker = Start-FullTestWorker -Name 'native-library' -Executable $library[0].executable -Arguments $skip -Expected $nativeNames -Directory $ReportDirectory
        $workers.Add($worker)
        Wait-FullTestWorker $worker $cpu
    }
    foreach ($artifact in @($artifacts | Where-Object { $_.target.kind -notcontains 'lib' -and $_.target.name -ne 'live_runtime' } | Sort-Object {$_.target.name})) {
        $expected = @(Get-ExecutableTestNames $artifact.executable)
        if ($artifact.target.name -eq 'renderer_process') {
            $rendererPlan = Get-BrowserTestPlan -Names $expected -Workers $BrowserWorkers -Target renderer_process
            $rendererPlan | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'renderer-plan.json') -Encoding utf8
            if ($rendererPlan.EarlyNative.Count) {
                $worker = Start-FullTestWorker -Name 'renderer-native' -Executable $artifact.executable -Arguments (@('--exact') + $rendererPlan.EarlyNative) -Expected $rendererPlan.EarlyNative -Directory $ReportDirectory
                $workers.Add($worker)
                Wait-FullTestWorker $worker $cpu
            }
            # Keep at most four active test processes, including CPU workers.
            while (@($cpu | Where-Object { -not $_.Process.HasExited }).Count + $BrowserWorkers -gt 4) {
                foreach ($peer in $cpu) {
                    if (-not $peer.Completed -and $peer.Process.HasExited) { Complete-FullTestWorker $peer }
                }
                Start-Sleep -Milliseconds 200
            }
            $functional = @(foreach ($shard in $rendererPlan.Functional) {
                if ($shard.Names.Count) {
                    $worker = Start-FullTestWorker -Name $shard.Name -Executable $artifact.executable -Arguments (@('--exact') + $shard.Names.ToArray()) -Expected $shard.Names.ToArray() -Directory $ReportDirectory
                    $workers.Add($worker)
                    $worker
                }
            })
            foreach ($worker in $functional) { Wait-FullTestWorker $worker (@($functional) + @($cpu)) }
            foreach ($worker in $cpu) { Wait-FullTestWorker $worker @() }
            $exclusive = @($rendererPlan.Exclusive | Where-Object { $_ -cnotin $rendererPlan.EarlyNative })
            if ($exclusive.Count) {
                $worker = Start-FullTestWorker -Name 'renderer-exclusive' -Executable $artifact.executable -Arguments (@('--exact') + $exclusive) -Expected $exclusive -Directory $ReportDirectory
                $workers.Add($worker)
                Wait-FullTestWorker $worker @()
            }
            continue
        }
        $worker = Start-FullTestWorker -Name ($artifact.target.kind[0] + '-' + $artifact.target.name) -Executable $artifact.executable -Arguments @() -Expected $expected -Directory $ReportDirectory
        $workers.Add($worker)
        Wait-FullTestWorker $worker $cpu
    }
    foreach ($worker in $cpu) { Wait-FullTestWorker $worker @() }
    if ($plan.ExclusiveNames.Count) {
        $worker = Start-FullTestWorker -Name 'exclusive-library' -Executable $library[0].executable -Arguments (@('--exact') + $plan.ExclusiveNames) -Expected $plan.ExclusiveNames -Directory $ReportDirectory
        $workers.Add($worker)
        Wait-FullTestWorker $worker @()
    }
    # Timing/memory-sensitive browser cases run after CPU work has finished.
    foreach ($artifact in @($artifacts | Where-Object { $_.target.name -eq 'live_runtime' })) {
        $expected = @(Get-ExecutableTestNames $artifact.executable)
        $browserPlan = Get-BrowserTestPlan -Names $expected -Workers $BrowserWorkers
        $browserPlan | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'browser-plan.json') -Encoding utf8
        $functional = @(foreach ($shard in $browserPlan.Functional) {
            if ($shard.Names.Count) {
                $worker = Start-FullTestWorker -Name $shard.Name -Executable $artifact.executable -Arguments (@('--exact') + $shard.Names.ToArray()) -Expected $shard.Names.ToArray() -Directory $ReportDirectory
                $workers.Add($worker)
                $worker
            }
        })
        foreach ($worker in $functional) { Wait-FullTestWorker $worker $functional }
        if ($browserPlan.Exclusive.Count) {
            $worker = Start-FullTestWorker -Name 'browser-exclusive' -Executable $artifact.executable -Arguments (@('--exact') + $browserPlan.Exclusive) -Expected $browserPlan.Exclusive -Directory $ReportDirectory
            $workers.Add($worker)
            Wait-FullTestWorker $worker @()
        }
    }
    & cargo test --doc --locked *> (Join-Path $ReportDirectory 'doc-tests.log')
    if ($LASTEXITCODE -ne 0) { throw "Doc tests failed; inspect $ReportDirectory/doc-tests.log" }
    & (Join-Path $PSScriptRoot 'test-full-test-plan.ps1')
    & (Join-Path $PSScriptRoot 'test-full-test-process.ps1')
    $success = $true
} finally {
    foreach ($worker in $workers) {
        if (-not $worker.Process.HasExited) {
            # Only processes started above are owned by this invocation.
            $worker.Process.Kill($true)
            $worker.Process.WaitForExit()
        }
        if (-not $worker.Completed) {
            $worker.Output.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $ReportDirectory ($worker.Name + '.stdout.log')) -Encoding utf8
            $worker.Errors.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $ReportDirectory ($worker.Name + '.stderr.log')) -Encoding utf8
            $worker.Exit = $worker.Process.ExitCode
            $worker.Seconds = $worker.Clock.Elapsed.TotalSeconds
        }
    }
    [ordered]@{
        revision = (& git rev-parse HEAD); working_tree_clean = -not [bool](& git status --porcelain)
        success = $success; seconds = $clock.Elapsed.TotalSeconds; compile_seconds = $compileSeconds
        cpu_workers = $CpuWorkers; exclusive_native_workers = 1; functional_browser_workers = $BrowserWorkers
        workers = @($workers | Select-Object Name, Cpu, Started, Seconds, Exit, Passed, Ignored)
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'full-tests.json') -Encoding utf8
    foreach ($worker in $workers) { $worker.Process.Dispose() }
    Pop-Location
    Write-Host "Complete test evidence: $ReportDirectory"
}
