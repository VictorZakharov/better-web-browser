[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$cacheScript = Join-Path $PSScriptRoot 'cargo-source-cache.ps1'
$root = Join-Path (Join-Path $PSScriptRoot '../target') ("source-cache-test-" + [Guid]::NewGuid().ToString('N'))
$source = Join-Path $root 'source'
$bundles = Join-Path $root 'bundles'
$restored = Join-Path $root 'restored'
foreach ($index in 0..31) {
    $path = Join-Path $source "registry/crate-$($index % 3)/nested/file-$index.txt"
    [IO.Directory]::CreateDirectory((Split-Path -Parent $path)) | Out-Null
    [IO.File]::WriteAllText($path, "fixture $index`n" + ('x' * ($index * 13)))
}
[IO.File]::WriteAllText((Join-Path $source 'registry/crate-0/.cargo-ok'), '{"v":1}')
[IO.Directory]::CreateDirectory((Join-Path $source 'registry/crate-0/empty')) | Out-Null
[IO.File]::WriteAllBytes((Join-Path $source 'registry/crate-0/雪.rs'), [byte[]]@())
& $cacheScript -Mode Create -SourceDirectory $source -ArchiveDirectory $bundles
& $cacheScript -Mode Restore -SourceDirectory $restored -ArchiveDirectory $bundles
$files = @(Get-ChildItem -LiteralPath $source -Recurse -File -Force)
foreach ($file in $files) {
    $copy = Join-Path $restored ([IO.Path]::GetRelativePath($source, $file.FullName))
    if ((Get-FileHash -LiteralPath $file.FullName).Hash -ne (Get-FileHash -LiteralPath $copy).Hash) {
        throw "Cache round-trip changed $($file.FullName)."
    }
}
if (@(Get-ChildItem -LiteralPath $restored -Recurse -File -Force).Count -ne $files.Count) {
    throw 'Cache round-trip changed the file count.'
}
if (-not (Test-Path -LiteralPath (Join-Path $restored 'registry/crate-0/empty') -PathType Container)) {
    throw 'Cache round-trip lost an empty directory.'
}
function Expect-Rejection {
    param([scriptblock] $Action)
    $rejected = $false
    try { & $Action } catch { $rejected = $true }
    if (-not $rejected) { throw 'Unsafe/incomplete Cargo cache operation was accepted.' }
}
Expect-Rejection { & $cacheScript -Mode Restore -SourceDirectory $restored -ArchiveDirectory $bundles }
Expect-Rejection { & $cacheScript -Mode Restore -SourceDirectory (Join-Path $root 'missing') -ArchiveDirectory (Join-Path $root 'absent') }
Expect-Rejection { & $cacheScript -Mode Create -SourceDirectory $source -ArchiveDirectory (Join-Path $source 'nested') }
Expect-Rejection { & $cacheScript -Mode Create -SourceDirectory $source -ArchiveDirectory $bundles }

# Keep native ZipFile's traversal rejection when shards extract concurrently.
$evilBundles = Join-Path $root 'evil'
[IO.Directory]::CreateDirectory($evilBundles) | Out-Null
foreach ($index in 0..3) {
    $zip = [IO.Compression.ZipFile]::Open((Join-Path $evilBundles "sources-$index.zip"), [IO.Compression.ZipArchiveMode]::Create)
    try { if ($index -eq 0) { $zip.CreateEntry('../escaped.txt') | Out-Null } } finally { $zip.Dispose() }
}
Expect-Rejection { & $cacheScript -Mode Restore -SourceDirectory (Join-Path $root 'evil-output') -ArchiveDirectory $evilBundles }
if (Test-Path -LiteralPath (Join-Path $root 'escaped.txt')) { throw 'Cache archive escaped its destination.' }
Expect-Rejection { & $cacheScript -Mode Restore -SourceDirectory (Join-Path $root 'broken-output') -ArchiveDirectory $source }
Write-Host "Cargo source cache tests passed: $($files.Count) byte-identical files, existing/missing/nested/archive/traversal rejection."
