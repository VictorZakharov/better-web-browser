[CmdletBinding()]
param(
    [ValidateSet('debug', 'release')]
    [string] $Profile = 'debug',
    [string] $Target
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$v8Version = '152.2.0'
$archiveSha256 = '0b17ca072bae37dd4ff00e6014d2b413becb031c9342ee11cb8226a5881f62b2'
$librarySha256 = 'b62b3d08cc4e1350275a97136efaffd5232fefdccd2597f193b817b80d58d627'
$archiveUrl = "https://github.com/denoland/rusty_v8/releases/download/v$v8Version/rusty_v8_release_x86_64-pc-windows-msvc.lib.gz"

function Assert-V8Library {
    param([Parameter(Mandatory)][string] $Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $librarySha256) {
        throw "V8 static library checksum mismatch at ${Path}: expected $librarySha256, got $actual"
    }
    return $true
}

function Expand-V8Archive {
    param(
        [Parameter(Mandatory)][string] $Archive,
        [Parameter(Mandatory)][string] $Destination
    )

    $source = [IO.File]::OpenRead($Archive)
    try {
        $gzip = [IO.Compression.GZipStream]::new(
            $source,
            [IO.Compression.CompressionMode]::Decompress
        )
        try {
            $output = [IO.File]::Create($Destination)
            try {
                $gzip.CopyTo($output)
            } finally {
                $output.Dispose()
            }
        } finally {
            $gzip.Dispose()
        }
    } finally {
        $source.Dispose()
    }
}

Push-Location $repoRoot
try {
    $metadataText = cargo metadata --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed while preparing V8' }
    $metadata = $metadataText | ConvertFrom-Json
    $v8Package = $metadata.packages |
        Where-Object { $_.name -eq 'v8' -and $_.version -eq $v8Version } |
        Select-Object -First 1
    if ($null -eq $v8Package) {
        throw "cargo metadata did not resolve the pinned v8 $v8Version package"
    }

    $v8Root = [IO.Path]::GetFullPath((Split-Path -Parent $v8Package.manifest_path))
    $targetRoot = [IO.Path]::GetFullPath($metadata.target_directory)
    $profileRoot = if ([string]::IsNullOrWhiteSpace($Target)) {
        Join-Path $targetRoot $Profile
    } else {
        Join-Path (Join-Path $targetRoot $Target) $Profile
    }
    [IO.Directory]::CreateDirectory($profileRoot) | Out-Null
    if (-not [IO.Path]::GetPathRoot($v8Root).Equals(
        [IO.Path]::GetPathRoot($targetRoot),
        [StringComparison]::OrdinalIgnoreCase
    )) {
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
    }

    $libraryDirectory = Join-Path $profileRoot 'gn_out\obj'
    [IO.Directory]::CreateDirectory($libraryDirectory) | Out-Null
    $library = Join-Path $libraryDirectory 'rusty_v8.lib'
    if (-not (Assert-V8Library -Path $library)) {
        $candidateProfiles = @('debug', 'release') |
            ForEach-Object { Join-Path (Join-Path $targetRoot $_) 'gn_out\obj\rusty_v8.lib' }
        $candidate = $candidateProfiles |
            Where-Object { $_ -ne $library -and (Test-Path -LiteralPath $_ -PathType Leaf) } |
            Where-Object { Assert-V8Library -Path $_ } |
            Select-Object -First 1
        $partial = Join-Path $libraryDirectory ("rusty_v8-$([Guid]::NewGuid().ToString('N')).part")
        $download = "$partial.gz"
        try {
            if ($null -ne $candidate) {
                Copy-Item -LiteralPath $candidate -Destination $partial
            } else {
                Invoke-WebRequest -Uri $archiveUrl -OutFile $download -UseBasicParsing
                $actualArchive = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash.ToLowerInvariant()
                if ($actualArchive -ne $archiveSha256) {
                    throw "V8 archive checksum mismatch: expected $archiveSha256, got $actualArchive"
                }
                Expand-V8Archive -Archive $download -Destination $partial
            }
            [void] (Assert-V8Library -Path $partial)
            Move-Item -LiteralPath $partial -Destination $library
        } finally {
            Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $download -Force -ErrorAction SilentlyContinue
        }
    }

    $checksum = @(
        "url $archiveUrl"
        "archive-sha256 $archiveSha256"
        "sha256 $librarySha256"
    ) -join "`n"
    [IO.File]::WriteAllText(
        [IO.Path]::ChangeExtension($library, 'sum'),
        "$checksum`n",
        [Text.UTF8Encoding]::new($false)
    )
    Write-Output $library
} finally {
    Pop-Location
}
