[CmdletBinding()]
param([Parameter(Mandatory)] [ValidateSet('Create', 'Mount', 'Unmount')] [string] $Mode)

$ErrorActionPreference = 'Stop'
# This helper must never mount or format anything on a developer/self-hosted PC.
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or
    $env:RUNNER_OS -ne 'Windows') {
    throw 'Cargo source volumes are restricted to disposable GitHub-hosted Windows runners.'
}
. (Join-Path $PSScriptRoot 'cargo-source-volume-plan.ps1')
$plan = Get-CargoSourceVolumePlan -Mode $Mode -TemporaryDirectory $env:RUNNER_TEMP -ProfileDirectory $env:USERPROFILE
$clock = [Diagnostics.Stopwatch]::StartNew()
if ($Mode -eq 'Create') {
    [IO.Directory]::CreateDirectory((Split-Path $plan.Image)) | Out-Null
    [IO.Directory]::CreateDirectory($plan.Mount) | Out-Null
    $script = Join-Path (Split-Path $plan.Image) 'create.txt'
    [IO.File]::WriteAllText($script, $plan.CreationScript, [Text.Encoding]::ASCII)
    # One DiskPart invocation per job; later attachment uses Storage cmdlets.
    & diskpart.exe /s $script
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the disposable Cargo source image.' }
} elseif ($Mode -eq 'Mount') {
    [IO.Directory]::CreateDirectory($plan.Mount) | Out-Null
    Mount-DiskImage -ImagePath $plan.Image -NoDriveLetter -Access ReadWrite | Out-Null
}

# Resolve only the exact image just created/restored, never an arbitrary disk number.
$image = Get-DiskImage -ImagePath $plan.Image
if (-not $image.Attached) { throw 'Cargo source image was not attached.' }
$disks = @($image | Get-Disk)
if ($disks.Count -ne 1 -or $disks[0].IsBoot -or $disks[0].IsSystem) {
    throw 'Cargo source image did not resolve to one non-system virtual disk.'
}
$partitions = @($disks[0] | Get-Partition)
if ($partitions.Count -ne 1) { throw 'Cargo source image must contain exactly one partition.' }
$partition = $partitions[0]
$volume = $partition | Get-Volume
if ($volume.FileSystem -ne 'NTFS' -or $volume.FileSystemLabel -ne 'BreezeCargoSources') {
    throw 'Cargo source image has an unexpected filesystem or label.'
}
if ($Mode -eq 'Mount') {
    $partition | Add-PartitionAccessPath -AccessPath $plan.Mount
} elseif ($Mode -eq 'Unmount') {
    if ($partition.AccessPaths -notcontains ($plan.Mount.TrimEnd('\') + '\')) {
        throw 'Cargo source image is not mounted at the expected directory.'
    }
    $partition | Remove-PartitionAccessPath -AccessPath $plan.Mount
    Dismount-DiskImage -ImagePath $plan.Image
}
Write-Host ("Cargo source volume {0}: {1:N2}s" -f $Mode, $clock.Elapsed.TotalSeconds)
