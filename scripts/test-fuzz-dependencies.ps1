[CmdletBinding()]
param([switch] $Offline)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

function Read-DependencyGraph([string] $Manifest) {
    $arguments = @('metadata', '--format-version', '1', '--locked', '--manifest-path', $Manifest)
    if ($Offline) { $arguments += '--offline' }
    $json = & cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw "Cannot resolve locked dependencies for $Manifest." }
    return ($json | Out-String | ConvertFrom-Json)
}

$browser = Read-DependencyGraph (Join-Path $repo 'Cargo.toml')
$fuzz = Read-DependencyGraph (Join-Path $repo 'fuzz/Cargo.toml')
$vendor = [IO.Path]::GetFullPath((Join-Path $repo 'vendor')) + [IO.Path]::DirectorySeparatorChar
$patched = @($browser.packages | Where-Object {
    -not $_.source -and [IO.Path]::GetFullPath($_.manifest_path).StartsWith($vendor, [StringComparison]::OrdinalIgnoreCase)
})
if ($patched.Count -eq 0) { throw 'Browser dependency graph did not contain its reviewed local patches.' }
foreach ($package in $patched) {
    $matches = @($fuzz.packages | Where-Object { $_.name -eq $package.name })
    if ($matches.Count -ne 1 -or $matches[0].version -ne $package.version -or $matches[0].source -or
        [IO.Path]::GetFullPath($matches[0].manifest_path) -ne [IO.Path]::GetFullPath($package.manifest_path)) {
        throw "Fuzz workspace must use the browser's reviewed $($package.name) patch. Cargo ignores dependency-level patches."
    }
}
$browserVersions = @{}
foreach ($package in $browser.packages) {
    if (-not $browserVersions.ContainsKey($package.name)) { $browserVersions[$package.name] = @() }
    $browserVersions[$package.name] += $package.version
}
foreach ($package in $fuzz.packages) {
    if ($browserVersions.ContainsKey($package.name) -and $package.version -notin $browserVersions[$package.name]) {
        throw "Fuzz dependency $($package.name) $($package.version) differs from the browser lockfile. Refresh the fuzz lock from the browser's pinned versions."
    }
}
Write-Host "Fuzz workspace uses the browser's $($patched.Count) reviewed patches and shared locked versions."
