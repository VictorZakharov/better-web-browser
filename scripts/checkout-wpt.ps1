[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $Destination,

    [string] $Manifest = (Join-Path $PSScriptRoot '..\tests\wpt\manifest.json'),

    [switch] $VerifyOnly
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$manifestPath = (Resolve-Path -LiteralPath $Manifest).Path
$destinationPath = [IO.Path]::GetFullPath($Destination)
$repoPrefix = $repoRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if ($destinationPath.StartsWith($repoPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The WPT checkout must be outside the Breeze repository; upstream fixtures are not vendored.'
}

$configuration = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$repository = [string] $configuration.upstream.repository
$revision = [string] $configuration.upstream.revision
$officialRepositories = @('https://github.com/web-platform-tests/wpt.git', 'https://github.com/KhronosGroup/WebGL.git')
if ($repository -notin $officialRepositories -or $revision -notmatch '^[0-9a-fA-F]{40}$') {
    throw 'External tests require an official repository and a full pinned Git revision.'
}
$khronos = $repository -eq $officialRepositories[1]
if ($khronos -and $configuration.upstream.license -ne 'MIT') {
    throw 'The Khronos conformance suite must retain its MIT license.'
}
$harnessPaths = if ($khronos) { @('LICENSE.txt', 'sdk/tests/js/js-test-pre.js') } else { @('resources/testharness.js') }
$requiredPaths = @($harnessPaths) + @($configuration.support) + @($configuration.tests.path)
foreach ($requiredPath in $requiredPaths) {
    if ($requiredPath -notmatch '^[a-zA-Z0-9/_\-.]+$' -or
        $requiredPath.StartsWith('/') -or $requiredPath.Split('/') -contains '..') {
        throw "Unsafe external fixture path: $requiredPath"
    }
}
$sparsePatterns = @($requiredPaths | ForEach-Object { '/' + $_ })

if (Test-Path -LiteralPath $destinationPath) {
    if (-not (Test-Path -LiteralPath (Join-Path $destinationPath '.git'))) {
        throw "Destination already exists and is not a Git checkout: $destinationPath"
    }
    $changes = & git -C $destinationPath status --porcelain
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect the existing WPT checkout.' }
    if ($changes) { throw 'The existing WPT checkout has local changes; refusing to overwrite them.' }
    $origin = (& git -C $destinationPath remote get-url origin).Trim()
    if ($LASTEXITCODE -ne 0 -or $origin -ne $repository) {
        throw "The existing checkout origin is not $repository"
    }
    if ($VerifyOnly) {
        $head = (& git -C $destinationPath rev-parse HEAD).Trim()
        if ($LASTEXITCODE -ne 0 -or $head -ne $revision) {
            throw "The cached WPT checkout is not at pinned revision $revision."
        }
        foreach ($requiredPath in $requiredPaths) {
            if (-not (Test-Path -LiteralPath (Join-Path $destinationPath $requiredPath))) {
                throw "The cached WPT checkout is missing $requiredPath."
            }
        }
        Write-Output "Verified cached external WPT checkout at $destinationPath"
        Write-Output "Pinned revision: $revision"
        return
    }
} else {
    if ($VerifyOnly) {
        throw "The cached WPT checkout does not exist: $destinationPath"
    }
    & git clone --filter=blob:none --no-checkout $repository $destinationPath
    if ($LASTEXITCODE -ne 0) { throw 'Could not clone the WPT repository.' }
}

& git -C $destinationPath sparse-checkout init --no-cone
if ($LASTEXITCODE -ne 0) { throw 'Could not initialize the sparse WPT checkout.' }
# The curated suite can exceed Windows' command-line length. Paths have already
# passed the strict ASCII/path validation above; stream one sparse pattern per
# line instead of passing the complete suite as process arguments.
$sparsePatterns | & git -C $destinationPath sparse-checkout set --no-cone --stdin
if ($LASTEXITCODE -ne 0) { throw 'Could not configure the sparse WPT checkout.' }
& git -C $destinationPath fetch --depth 1 origin $revision
if ($LASTEXITCODE -ne 0) { throw "Could not fetch pinned WPT revision $revision." }
& git -C $destinationPath checkout --detach $revision
if ($LASTEXITCODE -ne 0) { throw "Could not check out pinned WPT revision $revision." }

Write-Output "Prepared external WPT checkout at $destinationPath"
Write-Output "Pinned revision: $revision"
