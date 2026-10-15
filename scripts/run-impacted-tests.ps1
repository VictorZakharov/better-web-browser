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
if ($Final) { Write-Host 'Final gate also runs full-target Clippy before the complete test plan.' }
if (-not $Run) { Write-Host 'Plan only. Add -Run to execute; subsets do not replace the final PR gate.'; return }

# Fail before Cargo if this Windows workstation would emit artifacts on C:.
$target = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) }
    else { Join-Path $repo 'target' }
if (($IsWindows -or $env:OS -eq 'Windows_NT') -and [IO.Path]::GetPathRoot($target) -ne 'G:\') {
    throw 'Set CARGO_TARGET_DIR to an absolute G: directory before running local tests.'
}
$prior = @{}
foreach ($name in @('CARGO_TARGET_DIR','CARGO_BUILD_JOBS','RUST_TEST_THREADS','TEMP','TMP',
    'CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG','CARGO_INCREMENTAL')) {
    $prior[$name] = [Environment]::GetEnvironmentVariable($name,'Process')
}
Push-Location -LiteralPath $repo
try {
    $env:CARGO_TARGET_DIR=$target; $env:CARGO_BUILD_JOBS='4'; $env:RUST_TEST_THREADS='1'
    $env:TEMP=Join-Path $target 'impacted-tests/temp'; $env:TMP=$env:TEMP
    [IO.Directory]::CreateDirectory($env:TEMP) | Out-Null
    # Reuse local compiler queries on G: across verification edits. An explicit
    # CARGO_INCREMENTAL=0 keeps the smaller non-incremental storage policy.
    if ([string]::IsNullOrWhiteSpace($prior['CARGO_INCREMENTAL'])) { $env:CARGO_INCREMENTAL='1' }
    # Match CI's correctness profiles without generating large local PDBs.
    # Debug assertions/overflow checks are unchanged; explicit debugger settings win.
    foreach ($name in @('CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG')) {
        if ([string]::IsNullOrWhiteSpace($prior[$name])) {
            [Environment]::SetEnvironmentVariable($name,'0','Process')
        }
    }
    if ($Final) {
        & cargo clippy --locked --all-targets -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'Full-target Clippy failed.' }
    }
    foreach ($command in $plan.Commands) {
        if ($command.Target -eq 'selector') {
            & (Join-Path $PSScriptRoot 'test-impacted-tests.ps1'); continue
        }
        if ($command.Target -eq 'all') {
            & (Join-Path $PSScriptRoot 'run-full-tests.ps1')
            continue
        }
        $arguments = @('test','--locked')
        if ($command.Target -eq 'lib') { $arguments += '--lib' }
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
    }
} finally {
    foreach ($name in $prior.Keys) {
        if ($null -eq $prior[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name,$prior[$name],'Process') }
    }
    Pop-Location
}
