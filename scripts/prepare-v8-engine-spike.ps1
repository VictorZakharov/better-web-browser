[CmdletBinding()]
param(
    [ValidateSet('debug', 'release')]
    [string] $Profile = 'debug'
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

Push-Location $repoRoot
try {
    $metadataText = cargo metadata --format-version 1 --locked --features v8-engine-spike
    if ($LASTEXITCODE -ne 0) {
        throw 'cargo metadata failed for the V8 engine spike'
    }
    $metadata = $metadataText | ConvertFrom-Json
    $v8Package = $metadata.packages |
        Where-Object { $_.name -eq 'v8' -and $_.version -eq '152.2.0' } |
        Select-Object -First 1
    if ($null -eq $v8Package) {
        throw 'cargo metadata did not resolve the pinned v8 152.2.0 package'
    }

    $v8Root = [IO.Path]::GetFullPath((Split-Path -Parent $v8Package.manifest_path))
    $targetRoot = [IO.Path]::GetFullPath($metadata.target_directory)
    $v8Drive = [IO.Path]::GetPathRoot($v8Root)
    $targetDrive = [IO.Path]::GetPathRoot($targetRoot)
    if ($v8Drive.Equals($targetDrive, [StringComparison]::OrdinalIgnoreCase)) {
        return
    }

    $profileRoot = Join-Path $targetRoot $Profile
    [IO.Directory]::CreateDirectory($profileRoot) | Out-Null
    $junction = Join-Path $profileRoot 'gn_root'
    if (Test-Path -LiteralPath $junction) {
        $item = Get-Item -LiteralPath $junction -Force
        $targets = @($item.Target) | ForEach-Object { [IO.Path]::GetFullPath($_) }
        if ($item.LinkType -ne 'Junction' -or $targets -notcontains $v8Root) {
            throw "Refusing to replace unexpected V8 build path: $junction"
        }
    } else {
        New-Item -ItemType Junction -Path $junction -Target $v8Root | Out-Null
    }

    Write-Output $junction
} finally {
    Pop-Location
}
