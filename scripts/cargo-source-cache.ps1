[CmdletBinding()]
param(
    [Parameter(Mandatory)] [ValidateSet('Create', 'Restore')] [string] $Mode,
    [Parameter(Mandatory)] [string] $SourceDirectory,
    [Parameter(Mandatory)] [string] $ArchiveDirectory
)

$ErrorActionPreference = 'Stop'
$clock = [Diagnostics.Stopwatch]::StartNew()
$source = [IO.Path]::GetFullPath($SourceDirectory).TrimEnd([IO.Path]::DirectorySeparatorChar)
$archiveRoot = [IO.Path]::GetFullPath($ArchiveDirectory).TrimEnd([IO.Path]::DirectorySeparatorChar)
$sourcePrefix = $source + [IO.Path]::DirectorySeparatorChar
$archivePrefix = $archiveRoot + [IO.Path]::DirectorySeparatorChar
if ($source -eq $archiveRoot -or $archiveRoot.StartsWith($sourcePrefix, [StringComparison]::OrdinalIgnoreCase) -or
    $source.StartsWith($archivePrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Cargo source and archive directories must be separate, non-nested paths.'
}
$shards = @(0..3 | ForEach-Object { Join-Path $archiveRoot "sources-$_.zip" })
foreach ($root in @($source, $archiveRoot)) {
    $parent = $root
    while ($parent) {
        if ((Test-Path -LiteralPath $parent) -and
            ((Get-Item -LiteralPath $parent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Cargo source caching refuses linked paths: $parent"
        }
        $parent = Split-Path -Parent $parent
    }
}

if ($Mode -eq 'Create') {
    # Cache only registry/src, never Cargo credentials/configuration or target outputs.
    # One ZIP per worker avoids sharing a ZipArchive stream between extraction threads.
    $entries = @(Get-ChildItem -LiteralPath $source -Recurse -Force | Sort-Object FullName)
    $fileCount = @($entries | Where-Object { -not $_.PSIsContainer }).Count
    if ($fileCount -eq 0) { throw 'Refusing to cache an empty Cargo source directory.' }
    if (@($entries | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count -ne 0) {
        throw 'Cargo source caching does not follow reparse points.'
    }
    [IO.Directory]::CreateDirectory($archiveRoot) | Out-Null
    $archives = @()
    try {
        foreach ($path in $shards) {
            # Create mode refuses an existing file: a failed/partial cache is not reused.
            $archives += [IO.Compression.ZipFile]::Open($path, [IO.Compression.ZipArchiveMode]::Create)
        }
        $index = 0
        foreach ($entry in $entries) {
            $relative = [IO.Path]::GetRelativePath($source, $entry.FullName).Replace('\', '/')
            $shard = $archives[$index % $shards.Count]
            # The cache service compresses the bundles; do not compress every file twice.
            if ($entry.PSIsContainer) {
                $shard.CreateEntry("$relative/") | Out-Null
            } else {
                [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                    $shard, $entry.FullName, $relative,
                    [IO.Compression.CompressionLevel]::NoCompression) | Out-Null
            }
            $index++
        }
    } finally {
        foreach ($archive in $archives) { $archive.Dispose() }
    }
    Write-Host "Cached $fileCount Cargo source files in $($shards.Count) shards."
} else {
    foreach ($path in $shards) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing Cargo source shard: $path" }
    }
    if ((Test-Path -LiteralPath $source) -and @(Get-ChildItem -LiteralPath $source -Force).Count -ne 0) {
        throw 'Cargo source restoration requires an empty destination.'
    }
    # ZipFile rejects traversal outside this root and duplicate file writes. A fresh
    # destination also prevents pre-existing links from redirecting archive writes.
    [IO.Directory]::CreateDirectory($source) | Out-Null
    $shards | ForEach-Object -Parallel {
        $ErrorActionPreference = 'Stop'
        [IO.Compression.ZipFile]::ExtractToDirectory($_, $using:source, $false)
    } -ThrottleLimit 4
}
Write-Host ("Cargo source cache {0}: {1:N2}s" -f $Mode, $clock.Elapsed.TotalSeconds)
