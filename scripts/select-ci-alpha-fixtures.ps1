[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('push', 'pull_request')]
    [string] $EventName,
    [string] $Matrix = (Join-Path $PSScriptRoot '../benchmarks/alpha/matrix.json')
)

$ErrorActionPreference = 'Stop'
$configuration = Get-Content -LiteralPath $Matrix -Raw | ConvertFrom-Json
$all = @($configuration.fixtures | ForEach-Object { [string] $_.id })
if ($all.Count -eq 0 -or @($all | Sort-Object -Unique).Count -ne $all.Count -or
    @($all | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -ne 0) {
    throw 'The alpha matrix must contain unique, nonempty fixture IDs.'
}

# PRs exercise long-form early scrolling, table/flex/grid layout, Shadow DOM and
# stylesheet adoption. Every fixture still runs on main; no assertion is relaxed.
$selected = if ($EventName -eq 'pull_request') {
    @('encyclopedia-article', 'layout-matrix', 'shadow-components', 'constructed-stylesheets')
} else {
    $all
}
foreach ($id in $selected) {
    if ($id -notin $all) { throw "Required CI alpha fixture '$id' is missing from the matrix." }
}
$selected
