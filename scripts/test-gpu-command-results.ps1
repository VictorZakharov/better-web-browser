$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/gpu-command-results.ps1"
function Assert-Result($Condition, [string] $Message) { if (!$Condition) { throw $Message } }
function Assert-Rejected([scriptblock] $Action) {
    $rejected = $false
    try { & $Action | Out-Null } catch { $rejected = $true }
    Assert-Result $rejected 'Invalid benchmark result was accepted.'
}

$setters = @(Get-GpuCommandMeasurements -Fixture numeric-setters -Title 'done 100,200.5,3.25,4,5,6;pixel=0,255,0,255;errors=0,0')
Assert-Result ($setters.Count -eq 6) 'Setter phases were lost.'
Assert-Result ($setters[0].case -eq 'clearColor.constant' -and $setters[0].iterations -eq 100000) 'Wrong command or denominator.'
Assert-Result ($setters[1].microseconds_per_iteration -eq 2.005) 'Per-command units are wrong.'
Assert-Result ($setters[5].case -eq 'uniform4fv.varying') 'Varying commands were mislabeled.'
$frames = @(Get-GpuCommandMeasurements -Fixture completed-canvas-frames -Title 'done readback frames=64;ms=20,30,40;active=10,20,30;errors=0,0,0')
Assert-Result ($frames.Count -eq 3 -and $frames[0].iterations -eq 64) 'Completed-frame denominator is wrong.'
Assert-Result ($frames[0].work_unit.Contains('not one individual GL command')) 'Composite work was mislabeled as a command.'
foreach ($title in @(
    'pending', 'failed', 'done 1,2,3,4,5,6;pixel=0,0,0,0;errors=0,0',
    'done 1,2,3,4,5,6;pixel=0,255,0,255;errors=1,0',
    'done 1,2,3,4,5,6;pixel=0,255,0,255;errors=0,1280',
    'done 1,2,3,4,5;pixel=0,255,0,255;errors=0,0',
    'done NaN,2,3,4,5,6;pixel=0,255,0,255;errors=0,0',
    'done -1,2,3,4,5,6;pixel=0,255,0,255;errors=0,0',
    'done 1,2,3,4,5,6;pixel=0,255,0,255;errors=0,0;extra',
    ('done ' + ('9' * 400) + ',2,3,4,5,6;pixel=0,255,0,255;errors=0,0'),
    ('x' * 513)
)) { Assert-Rejected { Get-GpuCommandMeasurements -Fixture numeric-setters -Title $title } }
Assert-Rejected { Get-GpuCommandMeasurements -Fixture probe -Title 'done' }
Assert-Rejected { Get-GpuCommandMeasurements -Fixture completed-canvas-frames -Title 'done readback frames=63;ms=1,2,3;active=1,2,3;errors=0,0,0' }
Assert-Rejected { Get-GpuCommandMeasurements -Fixture completed-canvas-frames -Title 'done readback frames=64;ms=1,2,3;active=1,2,3;errors=0,1,0' }
$odd = Get-GpuCommandStatistics -Values @(9, 1, 2)
Assert-Result ($odd.samples -eq 3 -and $odd.median -eq 2 -and $odd.maximum -eq 9) 'Outlier was discarded.'
$even = Get-GpuCommandStatistics -Values @(1, 2, 10, 20)
Assert-Result ($even.median -eq 6) 'Even sample median is wrong.'
Assert-Rejected { Get-GpuCommandStatistics -Values @(1, [double]::NaN) }
Assert-Rejected { Get-GpuCommandStatistics -Values @(1, [double]::PositiveInfinity) }
Assert-Rejected { Get-GpuCommandStatistics -Values @(-1) }
Write-Output 'GPU command result parser tests passed.'
