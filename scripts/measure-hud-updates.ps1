[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $Reference,
    [Parameter(Mandatory)][string] $Candidate,
    [Parameter(Mandatory)][string] $OutputDirectory,
    [ValidateRange(1, 10)][int] $Rounds = 3,
    [ValidateNotNullOrEmpty()][ValidateSet('opacity','content')][string[]] $Workload = @('opacity','content')
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'alpha-fixture-server.ps1')
Add-Type -AssemblyName System.Drawing
$outputPath = [IO.Path]::GetFullPath($OutputDirectory)
if ([IO.Path]::GetPathRoot($outputPath) -ne 'G:\') { throw 'Store performance artifacts on G:.' }
$binaries = @{}
foreach ($entry in @(@('reference', $Reference), @('candidate', $Candidate))) {
    $path = (Resolve-Path -LiteralPath $entry[1]).Path
    $binaries[$entry[0]] = @{ path = $path; sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash }
}
[IO.Directory]::CreateDirectory($outputPath) | Out-Null
$priorTemp, $priorTmp = $env:TEMP, $env:TMP
$server = $null
$results = [Collections.Generic.List[object]]::new()
try {
    $env:TEMP = Join-Path $outputPath 'tmp'; $env:TMP = $env:TEMP
    [IO.Directory]::CreateDirectory($env:TEMP) | Out-Null
    $root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../benchmarks/ui/fixtures'))
    $fixture = Join-Path $root 'retained-hud.html'
    $fixtureHash = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash
    Copy-Item -LiteralPath $fixture -Destination (Join-Path $outputPath 'fixture.html')
    $server = Start-AlphaFixtureServer -OutputDirectory (Join-Path $outputPath 'server') -Root $root
    foreach ($case in $Workload) {
    $query = if ($case -eq 'opacity') { '?animations=1&updateMs=250' } else { '?animations=0&updateMs=0&svg=1' }
    for ($round = 0; $round -lt $Rounds; $round++) {
        $order = if ($round % 2 -eq 0) { @('reference', 'candidate') } else { @('candidate', 'reference') }
        foreach ($kind in $order) {
            $name = "hud-$case-$round-$kind"
            $reportPath = Join-Path $outputPath "$name.json"
            $screenshot = Join-Path $outputPath "$name.png"
            # The common launcher verifies --benchmark on the actual child and
            # uses CreateNoWindow plus the fail-closed automation environment.
            & (Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1') -Browser $binaries[$kind].path `
                -Url ($server.Url + 'retained-hud.html' + $query) -Output $reportPath -Screenshot $screenshot `
                -WindowWidth 1100 -WindowHeight 760 -DeviceScaleFactor 1.25 -FreshProfile `
                -RendererMemoryBudget graphics -InitialActionDelayMs 700 -ActionSequence @('activate:#start') `
                -CompletionTitle 'Retained HUD complete' -SettleMs 13000 -TimeoutSeconds 45 `
                *> (Join-Path $outputPath "$name.log")
            $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
            if ($report.error -or $report.javascript_errors.Count -or $report.renderer_exits.Count) {
                throw "Hidden HUD failed: $name. Inspect $reportPath."
            }
            $entry = @($report.javascript_console | Where-Object { $_ -match 'HUD_REPORT ' })
            if ($entry.Count -ne 1) { throw "Expected one completed HUD report: $name" }
            $metrics = ($entry[0] -replace '^.*HUD_REPORT ', '') | ConvertFrom-Json
            foreach ($phase in @('quiet-before','moving','active','quiet-after')) {
                if ($metrics.phases.$phase.count -le 0) { throw "Missing phase $phase in $name" }
            }
            $bitmap = [Drawing.Bitmap]::new($screenshot)
            try {
                # Locate the Canvas marker to account for native chrome and DPI.
                $marker = $null
                for ($y = 0; $y -lt $bitmap.Height -and $null -eq $marker; $y++) {
                    for ($x = 0; $x -lt [Math]::Min(64, $bitmap.Width); $x++) {
                        $color = $bitmap.GetPixel($x, $y)
                        if ($color.R -eq 238 -and $color.G -eq 201 -and $color.B -eq 121) {
                            $marker = @($x, $y); break
                        }
                    }
                }
                if ($null -eq $marker) { throw "Native Canvas marker missing: $name" }
                $opacity = $bitmap.GetPixel($marker[0] + 755, $marker[1] + 531)
                # .5 red over #101922, allowing native rounding by one byte.
                if ([Math]::Abs($opacity.R - 136) -gt 1 -or [Math]::Abs($opacity.G - 12) -gt 1 -or
                    [Math]::Abs($opacity.B - 17) -gt 1) { throw "Native opacity pixel mismatch: $opacity" }
                $pixels = @{ canvas = '238,201,121,255'; opacity = "$($opacity.R),$($opacity.G),$($opacity.B),$($opacity.A)" }
            } finally { $bitmap.Dispose() }
            $row = [ordered]@{
                workload = $case; round = $round; kind = $kind; binary = $binaries[$kind]; fixture_sha256 = $fixtureHash
                metrics = $metrics; pixels = $pixels
            }
            $results.Add($row)
            if ((Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash -ne $fixtureHash) { throw 'Fixture changed during comparison.' }
            $results.ToArray() | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $outputPath 'samples.json') -Encoding utf8
            $row | ConvertTo-Json -Depth 10 -Compress
        }
    }
    }
} finally {
    Stop-AlphaFixtureServer $server
    $env:TEMP = $priorTemp; $env:TMP = $priorTmp
}
