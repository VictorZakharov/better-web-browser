[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'cargo-source-volume-plan.ps1')
$root = Join-Path $PSScriptRoot "../target/source-volume-test-$([Guid]::NewGuid().ToString('N'))"
$root = [IO.Path]::GetFullPath($root)
$temporary = Join-Path $root 'temp'
$testProfile = Join-Path $root 'profile'
[IO.Directory]::CreateDirectory($temporary) | Out-Null
[IO.Directory]::CreateDirectory($testProfile) | Out-Null
$arguments = @{ TemporaryDirectory = $temporary; ProfileDirectory = $testProfile }
function Expect-Rejection {
    param([scriptblock] $Action)
    $rejected = $false
    try { & $Action } catch { $rejected = $true }
    if (-not $rejected) { throw 'Unsafe source-volume operation was accepted.' }
}
$plan = Get-CargoSourceVolumePlan -Mode Create @arguments
if ($plan.Image -ne (Join-Path $temporary 'cargo-source-volume/registry.vhdx') -or
    $plan.Mount -ne (Join-Path $testProfile '.cargo/registry/src')) { throw 'Unexpected volume paths.' }
$commands = $plan.CreationScript -split "`r`n"
if ($commands.Count -ne 5 -or $commands[0] -ne "create vdisk file=`"$($plan.Image)`" maximum=4096 type=expandable" -or
    $commands[1] -ne 'attach vdisk' -or $commands[2] -ne 'create partition primary' -or
    $commands[3] -ne 'format fs=ntfs quick label=BreezeCargoSources' -or
    $commands[4] -ne "assign mount=`"$($plan.Mount)`"") { throw 'Unexpected virtual-image creation commands.' }
Expect-Rejection { Get-CargoSourceVolumePlan -Mode Mount @arguments }
Expect-Rejection { Get-CargoSourceVolumePlan -Mode Unmount @arguments }
Expect-Rejection { Get-CargoSourceVolumePlan -Mode Create -TemporaryDirectory '.' -ProfileDirectory $testProfile }
Expect-Rejection { Get-CargoSourceVolumePlan -Mode Create -TemporaryDirectory "$temporary`nattach vdisk" -ProfileDirectory $testProfile }
Expect-Rejection { Get-CargoSourceVolumePlan -Mode Create -TemporaryDirectory ($temporary + '"') -ProfileDirectory $testProfile }
[IO.Directory]::CreateDirectory((Split-Path $plan.Image)) | Out-Null
[IO.File]::WriteAllText($plan.Image, 'a fixture, never mounted')
Expect-Rejection { Get-CargoSourceVolumePlan -Mode Create @arguments }
Get-CargoSourceVolumePlan -Mode Mount @arguments | Out-Null
[IO.Directory]::CreateDirectory($plan.Mount) | Out-Null
[IO.File]::WriteAllText((Join-Path $plan.Mount 'existing.txt'), 'preserve')
Expect-Rejection { Get-CargoSourceVolumePlan -Mode Mount @arguments }
$linkedProfile = Join-Path $root 'linked'
New-Item -ItemType Junction -Path $linkedProfile -Target $testProfile | Out-Null
Expect-Rejection { Get-CargoSourceVolumePlan -Mode Mount -TemporaryDirectory $temporary -ProfileDirectory $linkedProfile }
if ([IO.File]::ReadAllText((Join-Path $plan.Mount 'existing.txt')) -ne 'preserve') { throw 'Existing files changed.' }
# Do not invoke any Storage cmdlet or DiskPart locally, even for the positive cases.
# Exercise the execution guard only when this is not a supported hosted environment.
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') {
    foreach ($mode in @('Create', 'Mount', 'Unmount')) {
        Expect-Rejection { & (Join-Path $PSScriptRoot 'cargo-source-volume.ps1') -Mode $mode }
    }
}
Write-Host 'Cargo source-volume path, non-overwrite, command-scope, linked-path, and local-execution guard tests passed.'
