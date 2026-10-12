[CmdletBinding()]
param(
    [string] $Base = 'HEAD',
    [AllowEmptyCollection()][string[]] $ChangedPath,
    [switch] $Run,
    [switch] $Final
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'impacted-tests-plan.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
try {
    $paths = if ($PSBoundParameters.ContainsKey('ChangedPath')) { @($ChangedPath) }
        else { @(Get-TestImpactChangedPaths -Base $Base -RepoRoot $repo) }
    $plan = Get-ImpactedTestPlan -ChangedPath $paths -Final:$Final
} catch {
    Write-Warning "Impact discovery failed; selecting full tests: $($_.Exception.Message)"
    $plan = Get-ImpactedTestPlan -ChangedPath @('unclassified discovery failure') -Final
}
$plan.Reasons | ForEach-Object { Write-Host $_ }
$plan.Unknown | ForEach-Object { Write-Warning "Unclassified/shared risk: $_" }
if ($plan.Full) { Write-Host 'Full local tests selected (final gate or conservative fallback).' }
foreach ($command in $plan.Commands) {
    Write-Host "Test group $($command.Id): target=$($command.Target) filter=$($command.Filter)"
}
if (-not $Run) { Write-Host 'Plan only. Add -Run to execute; subsets do not replace the final PR gate.'; return }

# Fail before Cargo if this Windows workstation would emit artifacts on C:.
$target = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) }
    else { Join-Path $repo 'target' }
if (($IsWindows -or $env:OS -eq 'Windows_NT') -and [IO.Path]::GetPathRoot($target) -ne 'G:\') {
    throw 'Set CARGO_TARGET_DIR to an absolute G: directory before running local tests.'
}
$prior = @{}
foreach ($name in @('CARGO_TARGET_DIR','CARGO_BUILD_JOBS','RUST_TEST_THREADS','TEMP','TMP')) {
    $prior[$name] = [Environment]::GetEnvironmentVariable($name,'Process')
}
Push-Location -LiteralPath $repo
try {
    $env:CARGO_TARGET_DIR=$target; $env:CARGO_BUILD_JOBS='4'; $env:RUST_TEST_THREADS='1'
    $env:TEMP=Join-Path $target 'impacted-tests/temp'; $env:TMP=$env:TEMP
    [IO.Directory]::CreateDirectory($env:TEMP) | Out-Null
    foreach ($command in $plan.Commands) {
        if ($command.Target -eq 'selector') {
            & (Join-Path $PSScriptRoot 'test-impacted-tests.ps1'); continue
        }
        $arguments = @('test','--locked')
        if ($command.Target -eq 'all') { $arguments += '--all-targets' }
        elseif ($command.Target -eq 'lib') { $arguments += '--lib' }
        else { $arguments += @('--test',$command.Target) }
        if ($command.Filter) {
            $arguments += $command.Filter
            # Cargo treats an unmatched filter as success; never accept that.
            $listing = & cargo @arguments -- --list
            if ($LASTEXITCODE -ne 0) { throw "Cannot list group $($command.Id)." }
            if (@($listing | Where-Object { $_ -match ': test$' }).Count -eq 0) {
                throw "Group $($command.Id) selected zero tests. Fix its ownership contract."
            }
        }
        & cargo @arguments
        if ($LASTEXITCODE -ne 0) { throw "Test group $($command.Id) failed." }
        if ($command.Target -eq 'all') {
            & cargo test --doc --locked
            if ($LASTEXITCODE -ne 0) { throw 'Doc tests failed.' }
        }
    }
} finally {
    foreach ($name in $prior.Keys) { [Environment]::SetEnvironmentVariable($name,$prior[$name],'Process') }
    Pop-Location
}
