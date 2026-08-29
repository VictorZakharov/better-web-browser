[CmdletBinding()]
param(
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$')] [string] $Version,
    [Parameter(Mandatory)] [ValidatePattern('^[0-9a-fA-F]{40}$')] [string] $Commit,
    [Parameter(Mandatory)] [string] $Executable,
    [Parameter(Mandatory)] [string] $OutputDirectory
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$outputPath = [IO.Path]::GetFullPath($OutputDirectory)
[IO.Directory]::CreateDirectory($outputPath) | Out-Null

$manifestLines = Get-Content -LiteralPath (Join-Path $repoRoot 'Cargo.toml')
$inPackage = $false
$cargoVersion = $null
foreach ($line in $manifestLines) {
    if ($line -match '^\s*\[(.+)\]\s*$') { $inPackage = $Matches[1] -eq 'package'; continue }
    if ($inPackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $cargoVersion = $Matches[1]; break }
}
if ($cargoVersion -ne $Version) { throw "Cargo package version $cargoVersion does not match release $Version." }

$stream = [IO.File]::OpenRead($executablePath)
try {
    $reader = [IO.BinaryReader]::new($stream)
    if ($reader.ReadUInt16() -ne 0x5A4D) { throw 'Release executable lacks an MZ header.' }
    $stream.Position = 0x3C
    $peOffset = $reader.ReadInt32()
    $stream.Position = $peOffset
    if ($reader.ReadUInt32() -ne 0x00004550) { throw 'Release executable lacks a PE signature.' }
    if ($reader.ReadUInt16() -ne 0x8664) { throw 'Release executable is not Windows x64.' }
} finally {
    if ($null -ne $reader) { $reader.Dispose() } else { $stream.Dispose() }
}

& (Join-Path $PSScriptRoot 'generate-third-party-notices.ps1') -Check | Write-Host
$metadataText = & cargo metadata --locked --filter-platform x86_64-pc-windows-msvc --format-version 1
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed while packaging licenses.' }
$metadata = $metadataText | ConvertFrom-Json
$nodes = @{}
foreach ($node in $metadata.resolve.nodes) { $nodes[[string] $node.id] = $node }
$reachable = @{}
$queue = [Collections.Generic.Queue[string]]::new()
$queue.Enqueue([string] $metadata.resolve.root)
while ($queue.Count -gt 0) {
    $id = $queue.Dequeue()
    if ($reachable.ContainsKey($id)) { continue }
    $reachable[$id] = $true
    foreach ($dependency in $nodes[$id].deps) { $queue.Enqueue([string] $dependency.pkg) }
}
$packages = @($metadata.packages | Where-Object {
    $reachable.ContainsKey([string] $_.id) -and [string] $_.id -ne [string] $metadata.resolve.root
} | Sort-Object name, version)

$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("breeze-package-" + [Guid]::NewGuid().ToString('N'))
$packageName = "breeze-v$Version-x86_64-pc-windows-msvc"
$packageRoot = Join-Path $temporaryRoot $packageName
$licenseRoot = Join-Path $packageRoot 'licenses'
[IO.Directory]::CreateDirectory($licenseRoot) | Out-Null

function Write-Utf8File {
    param([string] $Path, [string] $Text)
    [IO.File]::WriteAllText($Path, $Text.Replace("`r`n", "`n"), [Text.UTF8Encoding]::new($false))
}

function Copy-ReleaseText {
    param([string] $Source, [string] $Destination)
    Write-Utf8File -Path $Destination -Text ([IO.File]::ReadAllText($Source))
}

try {
    [IO.File]::Copy($executablePath, (Join-Path $packageRoot 'better-web-browser.exe'), $true)
    Copy-ReleaseText (Join-Path $repoRoot 'LICENSE') (Join-Path $packageRoot 'LICENSE.txt')
    Copy-ReleaseText (Join-Path $repoRoot 'README.md') (Join-Path $packageRoot 'README.md')
    Copy-ReleaseText (Join-Path $repoRoot 'THIRD_PARTY_NOTICES.md') (Join-Path $packageRoot 'THIRD_PARTY_NOTICES.md')
    Copy-ReleaseText (Join-Path $repoRoot 'docs\technical-alpha-release.md') (Join-Path $packageRoot 'TECHNICAL_ALPHA.md')
    Copy-ReleaseText (Join-Path $repoRoot 'docs\accessibility.md') (Join-Path $packageRoot 'accessibility.md')
    Write-Utf8File (Join-Path $packageRoot 'VERSION.txt') @"
Breeze $Version
commit $($Commit.ToLowerInvariant())
target x86_64-pc-windows-msvc
rust 1.95.0
"@

    foreach ($package in $packages) {
        if ([string]::IsNullOrWhiteSpace($package.license)) {
            throw "$($package.name) $($package.version) has no license expression."
        }
        $directoryName = "$($package.name)-$($package.version)"
        if ($directoryName -notmatch '^[0-9A-Za-z._+-]+$') { throw "Unsafe package directory: $directoryName" }
        $destination = Join-Path $licenseRoot $directoryName
        [IO.Directory]::CreateDirectory($destination) | Out-Null
        $repository = if ([string]::IsNullOrWhiteSpace($package.repository)) { [string] $package.source } else { [string] $package.repository }
        Write-Utf8File (Join-Path $destination 'PACKAGE.txt') @"
$($package.name) $($package.version)
license: $($package.license)
source: $repository
"@
        $sourceDirectory = Split-Path -Parent $package.manifest_path
        $licenseFiles = @(Get-ChildItem -LiteralPath $sourceDirectory -File | Where-Object {
            $_.Name -match '^(?i)(LICENSE|LICENCE|COPYING|UNLICENSE|NOTICE)(?:[-._].*)?$'
        } | Sort-Object Name)
        foreach ($licenseFile in $licenseFiles) {
            [IO.File]::Copy($licenseFile.FullName, (Join-Path $destination $licenseFile.Name), $true)
        }
        if ([string] $package.name -like 'accesskit*') {
            Copy-ReleaseText (Join-Path $repoRoot 'third_party\accesskit\LICENSE.chromium') (Join-Path $destination 'LICENSE.chromium')
        }
        if ($licenseFiles.Count -eq 0) {
            $fallback = @('ABOUT.md', 'README.md') | ForEach-Object { Join-Path $sourceDirectory $_ } | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
            if ($null -ne $fallback) { Copy-ReleaseText $fallback (Join-Path $destination 'PACKAGE-SOURCE-NOTICE.md') }
        }
    }

    $payload = @(Get-ChildItem -LiteralPath $packageRoot -File -Recurse | Sort-Object FullName | ForEach-Object {
        [ordered]@{
            path = $_.FullName.Substring($packageRoot.Length + 1).Replace('\', '/')
            bytes = $_.Length
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    })
    $artifactManifest = [ordered]@{
        schema = 1
        product = 'Breeze'
        package_version = $Version
        commit = $Commit.ToLowerInvariant()
        target = 'x86_64-pc-windows-msvc'
        executable = 'better-web-browser.exe'
        signed = $false
        files = $payload
    }
    $manifestJson = ($artifactManifest | ConvertTo-Json -Depth 8) + "`n"
    Write-Utf8File (Join-Path $packageRoot 'artifact-manifest.json') $manifestJson

    Add-Type -AssemblyName System.IO.Compression
    $archive = Join-Path $outputPath "$packageName.zip"
    $archiveStream = [IO.File]::Open($archive, [IO.FileMode]::Create, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
        $zip = [IO.Compression.ZipArchive]::new($archiveStream, [IO.Compression.ZipArchiveMode]::Create, $false)
        try {
            foreach ($file in @(Get-ChildItem -LiteralPath $packageRoot -File -Recurse | Sort-Object FullName)) {
                $relative = $file.FullName.Substring($temporaryRoot.Length + 1).Replace('\', '/')
                $entry = $zip.CreateEntry($relative, [IO.Compression.CompressionLevel]::Optimal)
                $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
                $input = [IO.File]::OpenRead($file.FullName)
                $output = $entry.Open()
                try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
            }
        } finally { $zip.Dispose() }
    } finally { $archiveStream.Dispose() }

    $externalManifest = Join-Path $outputPath "$packageName-manifest.json"
    Write-Utf8File $externalManifest $manifestJson
    $archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    $checksums = Join-Path $outputPath 'SHA256SUMS.txt'
    Write-Utf8File $checksums "$archiveHash *$packageName.zip`n"
    [pscustomobject]@{
        archive = $archive
        manifest = $externalManifest
        checksums = $checksums
        sha256 = $archiveHash
        dependencies = $packages.Count
    }
} finally {
    $fullTemporaryRoot = [IO.Path]::GetFullPath($temporaryRoot)
    $expectedPrefix = Join-Path ([IO.Path]::GetTempPath()) 'breeze-package-'
    if (-not $fullTemporaryRoot.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to remove an unexpected package staging path.'
    }
    if (Test-Path -LiteralPath $fullTemporaryRoot) { Remove-Item -LiteralPath $fullTemporaryRoot -Recurse -Force }
}
