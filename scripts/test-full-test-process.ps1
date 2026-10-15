$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'full-test-process.ps1')
$directory = Join-Path ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))) ('target/test-process-contract-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($directory) | Out-Null
$fakeOutput = ''; $fakeError = ''; $fakeExit = 0
$launches = [Collections.Generic.List[object]]::new()
function Start-HiddenTestProcess([Diagnostics.ProcessStartInfo] $Info) {
    $launches.Add($Info)
    if ($Info.UseShellExecute -or -not $Info.CreateNoWindow) { throw 'Test adapter attempted visible UI.' }
    $process = [pscustomobject]@{
        StandardOutput = [IO.StringReader]::new($fakeOutput)
        StandardError = [IO.StringReader]::new($fakeError)
        ExitCode = $fakeExit; HasExited = $true
    }
    $process | Add-Member ScriptMethod WaitForExit { if ($args.Count) { return $true } }
    $process | Add-Member ScriptMethod Dispose { $this.StandardOutput.Dispose(); $this.StandardError.Dispose() }
    $process
}
function Check-Worker([string] $Name, [string] $Text, [string[]] $Expected, [bool] $Reject = $false, [int] $Exit = 0) {
    $script:fakeOutput = $Text; $script:fakeExit = $Exit
    $worker = Start-FullTestWorker -Name $Name -Executable 'G:\fake-test.exe' -Arguments @() -Expected $Expected -Directory $directory -Cpu
    $failed = $false
    try { Complete-FullTestWorker $worker } catch { $failed = $true }
    finally { $worker.Process.Dispose() }
    if ($failed -ne $Reject) { throw "Unexpected validation result for $Name" }
}
$good = "test case::one ... ok`ntest case::two ... ignored, diagnostic probe`ntest result: ok. 1 passed; 0 failed; 1 ignored;`n"
Check-Worker 'good' $good @('case::one', 'case::two')
Check-Worker 'missing' $good @('case::one', 'case::three') $true
Check-Worker 'duplicate' ($good.Replace('case::two', 'case::one')) @('case::one', 'case::two') $true
Check-Worker 'case-sensitive' $good @('case::One', 'case::two') $true
Check-Worker 'bad-summary' ($good.Replace('1 passed', '0 passed')) @('case::one', 'case::two') $true
Check-Worker 'cpu-violation' '' @('case::one') $true 86
Check-Worker 'zero' "test result: ok. 0 passed; 0 failed; 0 ignored;`n" @()
Check-Worker 'should-panic' "test case::panic - should panic ... ok`ntest result: ok. 1 passed; 0 failed; 0 ignored;`n" @('case::panic')
if (@($launches | Where-Object { $_.Environment['BREEZE_TEST_CPU_ONLY'] -ne '1' }).Count) { throw 'CPU workers lack their fail-closed resource guard.' }
$fakeOutput = "case::one: test`ncase::two: test`n2 tests, 0 benchmarks`n"
if (@(Get-ExecutableTestNames 'G:\fake-test.exe').Count -ne 2) { throw 'Discovery lost tests.' }
$fakeOutput = ''
if (@(Get-ExecutableTestNames 'G:\fake-test.exe').Count) { throw 'Empty terse discovery must represent zero tests.' }
$fakeOutput = 'not libtest output'
$rejected = $false
try { Get-ExecutableTestNames 'G:\fake-test.exe' } catch { $rejected = $true }
if (-not $rejected) { throw 'Unrecognized nonempty discovery silently passed.' }
$fakeOutput = ''
$native = Start-FullTestWorker -Name 'native' -Executable 'G:\fake-test.exe' -Arguments @() -Expected @() -Directory $directory
if ($launches[$launches.Count - 1].Environment.ContainsKey('BREEZE_TEST_CPU_ONLY')) { throw 'Native worker inherited the CPU guard.' }
$native.Process.Dispose()
Write-Host 'Test-process contracts passed: hidden launches, CPU guard, exact names/counts, failures, ignored/zero/should-panic cases.'
