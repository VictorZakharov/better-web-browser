param(
    [Parameter(Mandatory = $true)]
    [string] $Url,
    # Comma- or space-separated executables to compare (one string, see AGENTS.md).
    [Parameter(Mandatory = $true)]
    [string] $Browser,
    [string] $Label,
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [int] $Rounds = 3,
    # The maintainer's display: 1920x1080 physical pixels at 125% scale.
    [int] $WindowWidth = 1536,
    [int] $WindowHeight = 864,
    [double] $DeviceScaleFactor = 1.25,
    # CSS-pixel rectangle "x,y,width,height" whose completion is timed, such as a side panel.
    [string] $Region,
    [int] $FilmstripIntervalMs = 100,
    [int] $FilmstripDurationMs = 8000,
    [int] $SettleMs = 7000,
    # Runs that start or end above this total CPU load are marked busy in the report.
    [int] $BusyPercent = 40,
    # Give each build its own profile, primed by one unmeasured visit, as a returning user has:
    # site-written cookies and storage (preferences such as appearance) and a warm HTTP cache.
    [switch] $WarmProfile
)

# Rotated page-load comparison of saved Breeze executables under the conditions a user sees.
#
# Each run goes through run-hidden-benchmark.ps1 (hidden, fail-closed automation guard) with a
# fresh profile, or with -WarmProfile a primed per-build profile, and a filmstrip. The report
# adds what a single benchmark run does not show:
# - machine CPU load before and after the run, because on a shared machine a page's load time
#   follows the renderer's CPU time and an idle-machine number understates what users see;
# - when the page, and optionally one region of it, stops changing: the first filmstrip frame
#   after which every frame matches the final one (Speed Index's "visually complete");
# - network completion, renderer CPU time and the renderer's presentations from the benchmark.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$executables = @($Browser -split '[ ,]+' | Where-Object { $_ })
$labels = @(if ($Label) { $Label -split '[ ,]+' | Where-Object { $_ } } else {
        $executables | ForEach-Object { [IO.Path]::GetFileNameWithoutExtension($_) }
    })
if ($labels.Count -ne $executables.Count) {
    throw "Give one label per executable ($($executables.Count)); got $($labels.Count)."
}
foreach ($executable in $executables) {
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw "Browser executable not found: $executable"
    }
}
$regionCss = $null
if ($Region) {
    $regionCss = @($Region -split '[ ,]+' | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) })
    if ($regionCss.Count -ne 4 -or $regionCss[2] -le 0 -or $regionCss[3] -le 0) {
        throw "Region must be 'x,y,width,height' in CSS pixels; got '$Region'."
    }
}

. (Join-Path $PSScriptRoot 'filmstrip-completion.ps1')

function Get-CpuLoad {
    # Three samples over about a second: one instantaneous reading is too noisy to flag a run.
    $samples = 1..3 | ForEach-Object {
        (Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average
        Start-Sleep -Milliseconds 400
    }
    [int][Math]::Round(($samples | Measure-Object -Average).Average)
}

function Get-Median([double[]] $Values) {
    $sorted = @($Values | Sort-Object)
    if ($sorted.Count -eq 0) { return $null }
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2) { $sorted[$middle] } else { ($sorted[$middle - 1] + $sorted[$middle]) / 2 }
}

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$outputRoot = (Resolve-Path -LiteralPath $OutputDirectory).Path
$benchmark = Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1'
$scale = $DeviceScaleFactor
$rows = [Collections.Generic.List[object]]::new()

function Get-ProfileArguments([int] $Index) {
    if (-not $WarmProfile) { return @('-FreshProfile') }
    @('-ProfileDirectory', (Join-Path $outputRoot "profile-$($labels[$Index])"))
}

function Get-SizeArguments {
    @('-WindowWidth', $WindowWidth, '-WindowHeight', $WindowHeight,
        '-DeviceScaleFactor', $scale.ToString([Globalization.CultureInfo]::InvariantCulture))
}

if ($WarmProfile) {
    foreach ($index in 0..($executables.Count - 1)) {
        $profileDirectory = (Get-ProfileArguments $index)[1]
        Remove-Item -LiteralPath $profileDirectory -Recurse -Force -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $profileDirectory | Out-Null
        $name = "$($labels[$index])-prime"
        $arguments = @(
            '-NoProfile', '-File', $benchmark, '-Url', $Url, '-Browser', (Resolve-Path -LiteralPath $executables[$index]).Path,
            '-Output', (Join-Path $outputRoot "$name.json"), '-SettleMs', $SettleMs
        ) + (Get-ProfileArguments $index) + (Get-SizeArguments)
        & pwsh @arguments *> (Join-Path $outputRoot "$name.log")
        if ($LASTEXITCODE -ne 0) {
            throw "Priming visit $name failed with exit code $LASTEXITCODE; see $(Join-Path $outputRoot "$name.log")."
        }
        "$name primed $profileDirectory"
    }
}

foreach ($round in 1..$Rounds) {
    # Alternate the order so neither build always runs first on a warming machine.
    $order = if ($round % 2) { 0..($executables.Count - 1) } else { ($executables.Count - 1)..0 }
    foreach ($index in $order) {
        $name = "$($labels[$index])-r$round"
        $json = Join-Path $outputRoot "$name.json"
        $film = Join-Path $outputRoot "film-$name"
        Remove-Item -LiteralPath $film -Recurse -Force -ErrorAction SilentlyContinue
        $cpuBefore = Get-CpuLoad
        $arguments = @(
            '-NoProfile', '-File', $benchmark, '-Url', $Url, '-Browser', (Resolve-Path -LiteralPath $executables[$index]).Path,
            '-Output', $json, '-SettleMs', $SettleMs,
            '-FilmstripDirectory', $film, '-FilmstripIntervalMs', $FilmstripIntervalMs,
            '-FilmstripDurationMs', $FilmstripDurationMs
        ) + (Get-ProfileArguments $index) + (Get-SizeArguments)
        & pwsh @arguments *> (Join-Path $outputRoot "$name.log")
        $exitCode = $LASTEXITCODE
        $cpuAfter = Get-CpuLoad
        if ($exitCode -ne 0) {
            throw "Benchmark $name failed with exit code $exitCode; see $(Join-Path $outputRoot "$name.log")."
        }
        $result = Get-Content -LiteralPath $json -Raw | ConvertFrom-Json
        $frames = @(Get-ChildItem -LiteralPath $film -Filter 'frame-*ms.png' | Sort-Object { [int]($_.BaseName -replace '\D', '') })
        if ($frames.Count -eq 0) { throw "Benchmark $name produced no filmstrip frames." }
        $first = [Drawing.Image]::FromFile($frames[0].FullName)
        $size = @($first.Width, $first.Height)
        $first.Dispose()
        $regionMs = $null
        if ($regionCss) {
            $device = @($regionCss | ForEach-Object { [int][Math]::Round($_ * $scale) })
            $regionMs = Get-CompletionMs $frames $device 0.01
        }
        $presentations = @($result.renderer_runtime_timeline | Where-Object { $_.rendered })
        $row = [pscustomobject]@{
            run              = $name
            build            = $labels[$index]
            cpu_before       = $cpuBefore
            cpu_after        = $cpuAfter
            busy             = ($cpuBefore -gt $BusyPercent -or $cpuAfter -gt $BusyPercent)
            region_ms        = $regionMs
            # A whole-page tolerance must stay below the share of a small region such as a panel.
            visual_ms        = Get-CompletionMs $frames @(0, 0, $size[0], $size[1]) 0.001
            network_ms       = [int]$result.network_ms
            renderer_cpu_ms  = [int]$result.renderer_cpu_time_ms
            script_ms        = [int]$result.javascript_ms
            style_ms         = [int]$result.style_refresh_ms
            layout_paint_ms  = [int]$result.layout_and_paint_ms
            presentations    = $presentations.Count
            last_present_ms  = if ($presentations.Count) { [int]$presentations[-1].elapsed_ms } else { $null }
        }
        $rows.Add($row)
        "$name exit 0; cpu $cpuBefore%/$cpuAfter%; region $regionMs ms; visual $($row.visual_ms) ms"
    }
}

$rows | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $outputRoot 'summary.json') -Encoding utf8
$rows | Format-Table run, cpu_before, cpu_after, busy, region_ms, visual_ms, network_ms, renderer_cpu_ms, script_ms, style_ms, layout_paint_ms, presentations -AutoSize | Out-String -Width 220
foreach ($build in $labels) {
    $own = @($rows | Where-Object build -eq $build)
    $quiet = @($own | Where-Object { -not $_.busy })
    $line = "{0}: region median {1} ms [{2}], renderer CPU median {3} ms, {4} of {5} runs busy" -f $build,
        (Get-Median @($own | Where-Object { $null -ne $_.region_ms } | ForEach-Object region_ms)),
        (($own | ForEach-Object region_ms) -join ', '),
        (Get-Median @($own | ForEach-Object renderer_cpu_ms)), ($own.Count - $quiet.Count), $own.Count
    $line
}
