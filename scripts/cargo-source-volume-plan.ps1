function Get-CargoSourceVolumePlan {
    param(
        [ValidateSet('Create', 'Mount', 'Unmount')] [string] $Mode,
        [string] $TemporaryDirectory,
        [string] $ProfileDirectory
    )
    # Fixed child paths only: never accept an arbitrary disk, mount point, or image.
    foreach ($path in @($TemporaryDirectory, $ProfileDirectory)) {
        if ([string]::IsNullOrWhiteSpace($path) -or -not [IO.Path]::IsPathFullyQualified($path) -or
            $path -match '["\r\n]' -or -not (Test-Path -LiteralPath $path -PathType Container)) {
            throw 'Cargo source volume requires existing absolute runner directories without quotes/newlines.'
        }
    }
    $image = [IO.Path]::GetFullPath((Join-Path $TemporaryDirectory 'cargo-source-volume/registry.vhdx'))
    $mount = [IO.Path]::GetFullPath((Join-Path $ProfileDirectory '.cargo/registry/src'))
    foreach ($path in @($image, $(if ($Mode -eq 'Unmount') { Split-Path $mount } else { $mount }))) {
        $parent = $path
        while ($parent) {
            if ((Test-Path -LiteralPath $parent) -and
                ((Get-Item -LiteralPath $parent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                throw "Cargo source volume refuses unexpected linked paths: $parent"
            }
            $parent = Split-Path -Parent $parent
        }
    }
    if ($Mode -ne 'Unmount' -and (Test-Path -LiteralPath $mount) -and
        @(Get-ChildItem -LiteralPath $mount -Force).Count -ne 0) {
        throw 'Cargo source volume requires an empty mount directory.'
    }
    if ($Mode -eq 'Create') {
        if (Test-Path -LiteralPath $image) { throw 'Refusing to overwrite an existing source volume.' }
    } elseif (-not (Test-Path -LiteralPath $image -PathType Leaf)) {
        throw 'Missing Cargo source volume.'
    }
    [pscustomobject]@{
        Image = $image
        Mount = $mount
        # DiskPart stops at the first error (no `noerr`). Creating the new image
        # gives it focus; there is no physical-disk selection, cleanup, or deletion.
        CreationScript = @(
            "create vdisk file=`"$image`" maximum=4096 type=expandable"
            'attach vdisk'
            'create partition primary'
            'format fs=ntfs quick label=BreezeCargoSources'
            "assign mount=`"$mount`""
        ) -join "`r`n"
    }
}
