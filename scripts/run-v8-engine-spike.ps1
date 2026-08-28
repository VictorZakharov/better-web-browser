[CmdletBinding()]
param(
    [ValidateRange(1, 20)]
    [int] $Samples = 3
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

Push-Location $repoRoot
try {
    & (Join-Path $PSScriptRoot 'prepare-v8-engine-spike.ps1') -Profile release | Out-Null
    $testArguments = @(
        'test',
        '--release',
        '--locked',
        '--features',
        'v8-engine-spike',
        '--test',
        'renderer_process',
        'contained_v8_probe_matches_boa_and_retains_non_jit_restrictions',
        '--',
        '--nocapture'
    )
    for ($sample = 1; $sample -le $Samples; $sample++) {
        Write-Host "V8 engine spike sample $sample of $Samples"
        & cargo @testArguments
        if ($LASTEXITCODE -ne 0) {
            throw "V8 engine spike sample $sample failed"
        }
    }
} finally {
    Pop-Location
}
