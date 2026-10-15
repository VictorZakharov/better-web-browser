[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$scratch = Join-Path $repo ('target/build-entry-contract-' + [Guid]::NewGuid().ToString('N'))
$builder = Join-Path $PSScriptRoot 'build-browser.ps1'
$names = @('CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'TEMP', 'TMP', 'LIBCLANG_PATH', 'CLANG_PATH', 'PATH', 'CMAKE')
$before = @{}
foreach ($name in $names) { $before[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
function Expect-Rejection([scriptblock] $Action, [string] $Message) {
    try { & $Action } catch {
        if ($_.Exception.Message -like $Message) { return }
        throw
    }
    throw "Expected failure: $Message"
}
try {
    $env:CARGO_TARGET_DIR = Join-Path $repo 'target'
    $env:LIBCLANG_PATH = Join-Path $scratch 'preserved-parser'
    $env:CLANG_PATH = Join-Path $scratch 'preserved-clang.exe'
    $env:CARGO_BUILD_JOBS = '3'
    $env:TEMP = Join-Path $scratch 'preserved-temp'
    $env:TMP = Join-Path $scratch 'preserved-tmp'
    $expected = @{}
    foreach ($name in $names) { $expected[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
    Expect-Rejection {
        & $builder -CompilerDirectory (Join-Path $scratch 'missing-parser') -ReportDirectory $scratch
    } '*Expected LLVM 19+*'
    $report = Get-Content -LiteralPath (Join-Path $scratch 'build.json') -Raw | ConvertFrom-Json
    if ($report.success -or $report.profile -ne 'release' -or $report.stages.Count -ne 1 -or
        $report.stages[0].name -ne 'toolchain preflight' -or $report.stages[0].success) {
        throw 'A missing compiler reached dependency preparation or Cargo.'
    }
    foreach ($name in $names) {
        if ([Environment]::GetEnvironmentVariable($name, 'Process') -cne $expected[$name]) {
            throw "Build failure did not restore $name."
        }
    }
    # Reject an unsafe local output volume before even creating a report directory.
    $env:CARGO_TARGET_DIR = 'C:\breeze-must-not-create'
    Expect-Rejection { & $builder -ReportDirectory $scratch } '*absolute G:*'
    $env:CARGO_TARGET_DIR = Join-Path $repo 'target'
    Expect-Rejection { & $builder -ReportDirectory 'C:\breeze-must-not-create' } '*reports must be stored on G:*'
    $env:CMAKE = Join-Path $scratch 'missing-cmake.exe'
    Expect-Rejection { & (Join-Path $PSScriptRoot 'prepare-build-tools.ps1') } '*CMAKE must name an existing absolute executable path*'
    'Build entry contracts passed: preflight, failed-stage evidence, environment restoration and output volume guards.'
} finally {
    foreach ($name in $names) {
        if ($null -eq $before[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $before[$name], 'Process') }
    }
}
