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
$requiredPaths = @('resources/testharness.js') + @($configuration.tests.path)
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
& git -C $destinationPath sparse-checkout set --no-cone -- $sparsePatterns
if ($LASTEXITCODE -ne 0) { throw 'Could not configure the sparse WPT checkout.' }
& git -C $destinationPath fetch --depth 1 origin $revision
if ($LASTEXITCODE -ne 0) { throw "Could not fetch pinned WPT revision $revision." }
& git -C $destinationPath checkout --detach $revision
if ($LASTEXITCODE -ne 0) { throw "Could not check out pinned WPT revision $revision." }

Write-Output "Prepared external WPT checkout at $destinationPath"
Write-Output "Pinned revision: $revision"
