[CmdletBinding()]
param(
    [Parameter(Mandatory)] [ValidatePattern('^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$')] [string] $Tag,
    [string] $MainRef = 'origin/main',
    [string] $GitHubOutputPath
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

function Invoke-Git {
    param([Parameter(ValueFromRemainingArguments)] [string[]] $Arguments)
    $output = & git -c "safe.directory=$repoRoot" -C $repoRoot @Arguments
    if ($LASTEXITCODE -ne 0) { throw "git $($Arguments -join ' ') failed." }
    return ($output -join "`n").Trim()
}

& git -c "safe.directory=$repoRoot" -C $repoRoot show-ref --verify --quiet "refs/tags/$Tag"
if ($LASTEXITCODE -ne 0) { throw "Release tag does not exist: $Tag" }
$commit = Invoke-Git rev-list -n 1 $Tag
Invoke-Git rev-parse --verify $MainRef | Out-Null
& git -c "safe.directory=$repoRoot" -C $repoRoot merge-base --is-ancestor $commit $MainRef
if ($LASTEXITCODE -ne 0) { throw "$Tag is not an ancestor of protected main ($MainRef)." }

$version = $Tag.Substring(1)
$inPackage = $false
$cargoVersion = $null
foreach ($line in Get-Content -LiteralPath (Join-Path $repoRoot 'Cargo.toml')) {
    if ($line -match '^\s*\[(.+)\]\s*$') { $inPackage = $Matches[1] -eq 'package'; continue }
    if ($inPackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $cargoVersion = $Matches[1]; break }
}
if ($cargoVersion -ne $version) { throw "Tag version $version does not match Cargo package version $cargoVersion." }

$result = [pscustomobject]@{ tag = $Tag; version = $version; commit = $commit.ToLowerInvariant() }
if (-not [string]::IsNullOrWhiteSpace($GitHubOutputPath)) {
    Add-Content -LiteralPath $GitHubOutputPath -Encoding UTF8 -Value @(
        "tag=$($result.tag)",
        "version=$($result.version)",
        "commit=$($result.commit)"
    )
}
$result
