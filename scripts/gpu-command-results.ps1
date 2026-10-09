# Closed result grammar for the checked-in pixel-verified command fixtures.
# This parser never evaluates page text as PowerShell or accepts arbitrary labels.
function Get-GpuCommandMeasurements {
    param([Parameter(Mandatory)][string] $Fixture, [Parameter(Mandatory)][string] $Title)

    if ($Title.Length -gt 512) { throw 'GPU fixture title exceeds its result budget.' }
    $number = '[0-9]+(?:\.[0-9]+)?'
    if ($Fixture -eq 'numeric-setters') {
        $pattern = '^done (?<values>' + $number + '(?:,' + $number + '){5});pixel=0,255,0,255;errors=0,0$'
        $names = @('clearColor.constant', 'uniformMatrix4fv.constant', 'uniform4fv.constant',
            'clearColor.varying', 'uniformMatrix4fv.varying', 'uniform4fv.varying')
        $count = 100000
        $unit = 'converted setter, including queue drain; not GPU completion'
    } elseif ($Fixture -eq 'completed-canvas-frames') {
        $pattern = '^done readback frames=64;ms=(?<wall>' + $number + '(?:,' + $number + '){2});active=(?<values>' +
            $number + '(?:,' + $number + '){2});errors=0,0,0$'
        $names = @('canvas-frame.opaque', 'canvas-frame.premultiplied', 'canvas-frame.straight')
        $count = 64
        $unit = 'completed frame group control; not one individual GL command'
    } else { throw "Unknown GPU fixture: $Fixture" }
    if ($Title -cnotmatch $pattern) { throw "GPU fixture did not finish its pixel/error checks: $Title" }
    $elapsed = $Matches.values.Split(',')
    for ($index = 0; $index -lt $names.Count; $index++) {
        $value = [double]::Parse($elapsed[$index], [Globalization.CultureInfo]::InvariantCulture)
        if (![double]::IsFinite($value) -or $value -lt 0) { throw 'Invalid GPU fixture duration.' }
        [pscustomobject]@{ case = $names[$index]; active_ms = $value; iterations = $count;
            microseconds_per_iteration = $value * 1000 / $count; work_unit = $unit }
    }
}

function Get-GpuCommandStatistics {
    param([Parameter(Mandatory)][double[]] $Values)
    if (!$Values.Count -or @($Values | Where-Object { ![double]::IsFinite($_) -or $_ -lt 0 }).Count) {
        throw 'GPU statistics require finite nonnegative samples.'
    }
    $ordered = @($Values | Sort-Object)
    $middle = [int][Math]::Floor($ordered.Count / 2)
    $median = if ($ordered.Count % 2) { $ordered[$middle] } else {
        ($ordered[$middle - 1] + $ordered[$middle]) / 2
    }
    [pscustomobject]@{ samples = $ordered.Count; median = $median; minimum = $ordered[0]; maximum = $ordered[-1] }
}
