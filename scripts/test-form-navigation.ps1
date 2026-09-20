[CmdletBinding()]
param([string] $Browser, [string] $OutputDirectory = 'target/forms-navigation-proof/owned', [switch] $Chrome,
    [ValidateSet('direct','post','plain','multipart','redirect','cancel','removed-during-submit','removed','synthetic','flow','iframe','iframe-external')]
    [string[]] $Cases = @('direct','post','plain','multipart','redirect','cancel','removed-during-submit','removed','synthetic','flow'))
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$OutputDirectory = [IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
. (Join-Path $PSScriptRoot 'alpha-fixture-server.ps1')
$server = Start-AlphaFixtureServer -OutputDirectory $OutputDirectory -Script (Join-Path $PSScriptRoot 'serve-form-navigation.ps1')
$results = @()
try {
    foreach ($case in $Cases) {
        $beforeCount = if (Test-Path (Join-Path $OutputDirectory 'requests.json')) {
            @(Get-Content (Join-Path $OutputDirectory 'requests.json') -Raw | ConvertFrom-Json).Count
        } else { 0 }
        $url = $server.Url + '?case=' + $case
        if ($case -eq 'flow') { $url = $server.Url + 'flow?q=first' }
        $output = Join-Path $OutputDirectory ($case + '.json')
        if ($Chrome) {
            $actions = if ($case -eq 'flow') { @('--activate-link-after-ready', ($server.Url + 'destination'), '--back-after-ready', '--submit-control-selector', '#query', '--submit-control-value', 'second 🦀') } else { @('--click-after-ready', '50,40') }
            dotnet run --project (Join-Path $repo 'benchmarks/chromium') --configuration Release --no-build -- --url $url --output $output @actions --navigation-delay-ms 1000 --settle-ms 1000 --timeout-ms 15000
            if ($LASTEXITCODE -ne 0) { throw "Chrome failed: $case" }
        } else {
            $arguments = @{ Url=$url; Output=$output; ClickTarget='50,40'; NavigationDelayMs=500; SettleMs=1000; TimeoutSeconds=20; FreshProfile=$true }
            if ($case -eq 'flow') {
                $arguments.Remove('ClickTarget')
                $arguments.SelectorActivationTarget = '#result-link'
                $arguments.BackAfterReady = $true
                $arguments.ControlValue = @{'#query'='second 🦀'}
                $arguments.KeyTarget = 'Enter,Enter'
                $arguments.NavigationDelayMs = 1000
            }
            if ($Browser) { $arguments.Browser = $Browser }
            & (Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1') @arguments
        }
        $report = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
        $requests = @(Get-Content -LiteralPath (Join-Path $OutputDirectory 'requests.json') -Raw | ConvertFrom-Json)
        $observed = @($requests | Select-Object -Skip $beforeCount | Where-Object { $_.path -like '/echo*' -or $_.path -eq '/destination' -or $_.path -eq '/redirect' })
        $last = $observed | Select-Object -Last 1
        $valid = switch ($case) {
            'direct' { $last.method -eq 'GET' -and $last.path -eq '/echo?q=standards+%F0%9F%A6%80&text=line1%0D%0Aline2' }
            'post' { $last.method -eq 'POST' -and $last.path -eq '/echo?old=discard' -and $last.body -eq 'q=standards+%F0%9F%A6%80&text=line1%0D%0Aline2&go=yes' -and $last.content_type -eq 'application/x-www-form-urlencoded' -and $last.origin -eq $server.Url.TrimEnd('/') }
            'plain' { $last.method -eq 'POST' -and $last.content_type -eq 'text/plain' -and $last.body -eq "q=standards 🦀`r`ntext=line1`r`nline2`r`ngo=yes`r`n" }
            'multipart' { $last.method -eq 'POST' -and $last.content_type -like 'multipart/form-data; boundary=*' -and $last.body.Contains("name=`"q`"`r`n`r`nstandards 🦀`r`n") -and $last.body.Contains("name=`"text`"`r`n`r`nline1`r`nline2`r`n") }
            'redirect' { $observed[-2].method -eq 'POST' -and $observed[-2].path -eq '/redirect' -and $last.method -eq 'GET' -and $last.path -eq '/echo?redirected=yes' -and $last.body -eq '' }
            'cancel' { $report.final_url -eq $url -and $observed.Count -eq 0 }
            'removed-during-submit' { $report.final_url -eq $url -and $observed.Count -eq 0 }
            'removed' { $last.method -eq 'GET' -and $last.path -eq '/echo?q=standards+%F0%9F%A6%80&text=line1%0D%0Aline2' }
            'synthetic' { $last.path -eq '/destination' -and $report.final_url -eq ($server.Url + 'destination') }
            'iframe' { $last.path -eq '/destination' -and $report.final_url -eq ($server.Url + 'destination') }
            'iframe-external' { $last.path -eq '/destination' -and $report.final_url -eq ($server.Url + 'destination') }
            'flow' { $report.final_url -eq ($server.Url + 'flow?q=second+%F0%9F%A6%80') -and @($observed | Where-Object path -eq '/destination').Count -eq 1 -and @($requests | Where-Object path -eq '/flow?q=second+%F0%9F%A6%80').Count -eq 1 }
        }
        $consoleErrors = @($report.javascript_console | Where-Object { $_ -match '^error:' })
        if ($report.error -or $consoleErrors.Count -gt 0 -or @($report.javascript_errors | Where-Object { $null -ne $_ }).Count -gt 0) { $valid = $false }
        $results += [pscustomobject]@{ case=$case; passed=[bool]$valid; final_url=$report.final_url }
        Write-Host "$case : $valid"
    }
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'acceptance.json'), (ConvertTo-Json -InputObject $results))
    if (@($results | Where-Object { -not $_.passed }).Count) { throw 'Form navigation acceptance failed; inspect requests.json and acceptance.json.' }
} finally { Stop-AlphaFixtureServer $server }
