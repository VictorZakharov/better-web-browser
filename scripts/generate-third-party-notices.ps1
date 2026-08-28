[CmdletBinding()]
param(
    [string] $Output,
    [switch] $Check
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($Output)) {
    $Output = Join-Path $repoRoot 'THIRD_PARTY_NOTICES.md'
}
$outputPath = [IO.Path]::GetFullPath($Output)

$metadataText = & cargo metadata --locked --filter-platform x86_64-pc-windows-msvc --format-version 1
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed while generating third-party notices.' }
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
} | Sort-Object `
    @{ Expression = { [BitConverter]::ToString([Text.Encoding]::UTF8.GetBytes([string] $_.name)) } }, `
    @{ Expression = { [BitConverter]::ToString([Text.Encoding]::UTF8.GetBytes([string] $_.version)) } })
if ($packages.Count -eq 0) { throw 'The Windows release dependency graph is empty.' }

# Git may materialize CRLF on Windows and LF on CI. Hash the canonical repository text so the
# generated notice is identical across hosts.
$lockText = [IO.File]::ReadAllText((Join-Path $repoRoot 'Cargo.lock'))
$canonicalLockBytes = [Text.Encoding]::UTF8.GetBytes($lockText.Replace("`r`n", "`n").Replace("`r", "`n"))
$sha256 = [Security.Cryptography.SHA256]::Create()
try {
    $lockHash = [BitConverter]::ToString($sha256.ComputeHash($canonicalLockBytes)).Replace('-', '').ToLowerInvariant()
} finally {
    $sha256.Dispose()
}
$markdown = [Text.StringBuilder]::new()
[void] $markdown.AppendLine('# Third-party notices')
[void] $markdown.AppendLine()
[void] $markdown.AppendLine('This file describes the complete third-party Rust graph linked into the Windows x64 release. It is generated from the locked target graph by `scripts/generate-third-party-notices.ps1`; CI rejects a stale copy.')
[void] $markdown.AppendLine()
[void] $markdown.AppendLine('- Target: `x86_64-pc-windows-msvc`')
[void] $markdown.AppendLine("- Cargo.lock SHA-256: ``$lockHash``")
[void] $markdown.AppendLine("- Third-party packages: $($packages.Count)")
[void] $markdown.AppendLine()
[void] $markdown.AppendLine('| Package | Version | License expression | Source |')
[void] $markdown.AppendLine('|---|---:|---|---|')
foreach ($package in $packages) {
    if ([string]::IsNullOrWhiteSpace($package.license)) {
        throw "$($package.name) $($package.version) has no machine-readable license expression."
    }
    $source = if (-not [string]::IsNullOrWhiteSpace($package.repository)) {
        "[upstream]($($package.repository))"
    } elseif ([string] $package.source -like 'registry+*') {
        "[crates.io](https://crates.io/crates/$($package.name)/$($package.version))"
    } else {
        'repository path dependency'
    }
    $license = ([string] $package.license).Replace('|', '\|')
    [void] $markdown.AppendLine("| ``$($package.name)`` | $($package.version) | $license | $source |")
}

[void] $markdown.AppendLine()
[void] $markdown.AppendLine('## Bundled sources and data')
[void] $markdown.AppendLine()
[void] $markdown.AppendLine('- `boa_ast`, `boa_engine`, and `boa_parser` are locally modified path dependencies from Boa revision `bc36c3fac0969ea21ea0570b62e7846f97389b73`, offered under Unlicense OR MIT. Changes are recorded in each crate''s `LOCAL_CHANGES.md`; both upstream license texts are preserved beside the sources and copied into release archives.')
[void] $markdown.AppendLine('- Boa 0.21 dependencies named `paste` resolve to the locally packaged, source-unchanged `paste-complete` 1.0.15 fork from <https://github.com/esrauch/paste> (MIT OR Apache-2.0). This removes archived upstream `paste` without ignoring `RUSTSEC-2024-0436`; provenance is recorded in `vendor/paste-complete/LOCAL_CHANGES.md`.')
[void] $markdown.AppendLine('- `psl2` embeds a compact Mozilla Public Suffix List snapshot. The crate is MIT OR Apache-2.0; the list data is MPL-2.0. The crate and list versions are pinned by `Cargo.lock` and `psl2::psl_version()`.')
[void] $markdown.AppendLine('- The AccessKit crates are MIT OR Apache-2.0 and contain portions derived from Chromium under a BSD license. The required upstream notice is preserved at `third_party/accesskit/LICENSE.chromium` and copied beside every AccessKit package notice in release archives.')
[void] $markdown.AppendLine()
[void] $markdown.AppendLine('The release archive also contains each available package license/notice file under `licenses/<crate>-<version>/`. When a published crate omits a standalone license file, its package notice records the Cargo license expression and upstream repository. SPDX expressions in this document state the choices declared by each package; they do not relicense third-party work.')

$expected = $markdown.ToString().Replace("`r`n", "`n")
if ($Check) {
    if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
        throw "Third-party notices are missing: $outputPath"
    }
    $actual = [IO.File]::ReadAllText($outputPath).Replace("`r`n", "`n")
    if ($actual -ne $expected) {
        throw 'THIRD_PARTY_NOTICES.md is stale; run scripts/generate-third-party-notices.ps1.'
    }
    Write-Output "Third-party notices match $($packages.Count) Windows release packages."
    return
}

[IO.File]::WriteAllText($outputPath, $expected, [Text.UTF8Encoding]::new($false))
Write-Output $outputPath
