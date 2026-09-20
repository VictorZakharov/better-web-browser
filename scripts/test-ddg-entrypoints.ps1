[CmdletBinding()]
param(
    [string] $Browser,
    [string] $OutputDirectory = 'target/ddg-entrypoints-proof',
    [string] $Query = 'WHATWG HTML specification'
)
$ErrorActionPreference = 'Stop'
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
[IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
$encoded = [Uri]::EscapeDataString($Query)
$cases = @(
    @{ Name='homepage'; Url='https://duckduckgo.com/'; ResultSelector=$null },
    @{ Name='homepage-search'; Url='https://duckduckgo.com/'; ResultSelector='[data-testid="result-title-a"]'; Control='textarea[name="q"]' },
    @{ Name='html-results'; Url="https://html.duckduckgo.com/html/?q=$encoded"; ResultSelector='.result__a' },
    @{ Name='html-search'; Url='https://html.duckduckgo.com/html/?q=test'; ResultSelector='.result__a'; Control='input[name="q"]' }
)
$results = @()
foreach ($case in $cases) {
    $output = Join-Path $OutputDirectory ($case.Name + '.json')
    $arguments = @{
        Url=$case.Url; Output=$output; Screenshot=(Join-Path $OutputDirectory ($case.Name + '.png'))
        FreshProfile=$true; WindowWidth=1440; WindowHeight=900; SettleMs=8000; TimeoutSeconds=90
        DiagnosticSelector=@('title','textarea[name="q"]','input[name="q"]','select','option')
    }
    if ($Browser) { $arguments.Browser=$Browser }
    if ($case.ResultSelector) { $arguments.DiagnosticSelector += $case.ResultSelector }
    if ($case.Control) {
        $arguments.ControlValue=@{ $case.Control=$Query }
        $arguments.KeyTarget='Enter,Enter'
        $arguments.NavigationDelayMs=5000
    }
    & (Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1') @arguments
    $report=Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
    $errors=@($report.javascript_errors | Where-Object { $_ }) + @($report.javascript_console | Where-Object { $_ -match '^error:' })
    $valid=-not $report.error -and $errors.Count -eq 0 -and $report.titles.document_title -notmatch 'Application error'
    if ($case.ResultSelector) {
        $matches=@(($report.diagnostics | Where-Object selector -eq $case.ResultSelector).matches)
        $visible=@($matches | Where-Object { $_.layout_rect -and $_.layout_rect.height -gt 0 -and $_.layout_rect.y -lt $report.viewport_height_css_px })
        $valid=$valid -and $visible.Count -gt 0
    } else {
        $control=($report.diagnostics | Where-Object selector -eq 'textarea[name="q"]').matches | Select-Object -First 1
        $valid=$valid -and $null -ne $control.control_rect
    }
    if ($case.Control) {
        $queryParts=([Uri]$report.final_url).Query.TrimStart('?').Split('&')
        $submitted=@($queryParts | Where-Object { $_.StartsWith('q=') } | ForEach-Object { [Uri]::UnescapeDataString($_.Substring(2).Replace('+',' ')) })
        if ($case.Name -eq 'html-search') {
            # The HTML form uses POST. Verify the returned query, not a GET-only URL.
            $fields=@(($report.diagnostics | Where-Object selector -eq 'input[name="q"]').matches)
            $returned=@($fields.attributes | Where-Object name -eq 'value' | ForEach-Object value)
            $valid=$valid -and $returned -contains $Query -and $report.titles.document_title -eq "$Query at DuckDuckGo"
        } else {
            $valid=$valid -and $submitted -contains $Query
        }
    }
    if ($case.Name.StartsWith('html-')) {
        $selects=@(($report.diagnostics | Where-Object selector -eq 'select').matches)
        $escaped=@(($report.diagnostics | Where-Object selector -eq 'option').matches | Where-Object { $_.layout_rect })
        $valid=$valid -and $selects.Count -eq 2 -and $escaped.Count -eq 0
        foreach ($select in $selects) { $valid=$valid -and $select.control_rect.height -gt 0 -and $select.control_rect.height -lt 60 }
    }
    $results += [pscustomobject]@{case=$case.Name; passed=[bool]$valid; final_url=$report.final_url; errors=$errors}
    Write-Host "$($case.Name): $valid"
}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'acceptance.json'), (ConvertTo-Json -InputObject $results -Depth 5))
if (@($results | Where-Object { -not $_.passed }).Count) {
    throw 'DDG entrypoint acceptance failed. Inspect the reports/screenshots; a challenge or empty results page is not a pass.'
}
