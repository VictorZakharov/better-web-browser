$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'impacted-tests-plan.ps1')
function Expect-Plan([string[]]$Paths,[string[]]$Groups,[bool]$Full=$false) {
    $plan=Get-ImpactedTestPlan -ChangedPath $Paths
    $actual=@($plan.Commands | ForEach-Object { $_.Id })
    if ($plan.Full -ne $Full -or ($actual -join ',') -ne ($Groups -join ',')) {
        throw "Unexpected impact plan for $($Paths -join ','): $($actual -join ',') / full=$($plan.Full)"
    }
    return $plan
}
$images=@('image-regions','image-rows','image-bindings','video-sources','owned-uploads','queued-uploads','float-images','image-api')
$canvas=@('canvas-lib')+$images+@('canvas-renderer','webgl-renderer')
$uploads=$images+@('webgl-renderer')
$webgl=@('webgl-lib','webgl-renderer')
$script=@('script-lib','renderer','live-runtime')
$full=@('full','selector')
foreach ($path in @('src/engine/webgl/gpu_frames/copy.rs','src/gpu_composition/frames.rs',
    'src/windows_app/gpu_composition/groups.rs','src/renderer_protocol/gpu/tests.rs',
    'benchmarks/ui/fixtures/canvas-gpu-composition.html')) {
    $plan=Expect-Plan @($path) $full $true
    if ($plan.Unknown.Count -ne 0) {throw "Shared GPU presentation ownership is missing: $path"}
}
$null=Expect-Plan @('src/engine/script/canvas_host/glyph_paint/shadow.rs') $canvas
$null=Expect-Plan @('src\engine\script\bootstrap\canvas_text_shadow.js') $canvas
$null=Expect-Plan @('tests/canvas/text-shadow-regions.js') $canvas
$null=Expect-Plan @('benchmarks/ui/fixtures/canvas-frame-publication.html') $canvas
$null=Expect-Plan @('src/engine/script/bootstrap/webgl2_image_rows.js') $uploads
$null=Expect-Plan @('src/engine/webgl/command_batch/uploads.rs') $webgl
$null=Expect-Plan @('src/engine/script/engine/bridge.rs') $script
$null=Expect-Plan @('src/engine/script/bootstrap.rs','src/engine/script/bootstrap/canvas_text.js') $script
$null=Expect-Plan @('docs/new notes.md','AGENTS.md') @()
$null=Expect-Plan @() @()
foreach ($path in @('build.rs','Cargo.lock','src/not-yet-owned.rs','../src/unsafe.rs','C:\foreign.rs',"bad`npath",'')) {
    $null=Expect-Plan @($path) $full $true
}
$null=Expect-Plan @('deleted/old.rs','src/engine/script/bootstrap/canvas_text.js') $full $true
$null=Expect-Plan @('scripts/run-impacted-tests.ps1') @('selector')
$final=Get-ImpactedTestPlan -ChangedPath @('README.md') -Final
if (-not $final.Full -or (@($final.Commands | ForEach-Object {$_.Id}) -join ',') -ne 'full,selector') {
    throw 'Final gate must never be reduced, even for an empty/Markdown-only diff.'
}
$deduplicated=Get-ImpactedTestPlan -ChangedPath @('src/engine/script/bootstrap/canvas_text.js','src/engine/script/bootstrap/canvas_text.js')
if ($deduplicated.Commands.Count -ne $canvas.Count -or -not @($deduplicated.Reasons | Where-Object {$_ -eq 'canvas -> downstream webgl-images'}).Count) {
    throw 'Downstream closure or deduplication failed.'
}
Write-Host 'Impacted-test ownership, downstream closure, overlap, unknown/deleted/path guards and final-gate tests passed.'

# Exercise runner orchestration without compiling or launching any browser.
# The mocks exist only in this script's scope, not the user's shell/profile.
$calls=[Collections.Generic.List[object]]::new()
$emptyListing=$false
function cargo {
    $calls.Add(@($args))
    $global:LASTEXITCODE=0
    if ($args -contains '--list' -and -not $emptyListing) { 'contract_case: test' }
}
$priorTarget=$env:CARGO_TARGET_DIR
$priorJobs=$env:CARGO_BUILD_JOBS
$priorWorkers=$env:RUST_TEST_THREADS
$priorTemp=$env:TEMP
$priorTmp=$env:TMP
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
try {
    $env:CARGO_TARGET_DIR=Join-Path $root 'target'
    & (Join-Path $PSScriptRoot 'run-impacted-tests.ps1') -ChangedPath src/engine/webgl/command_batch/uploads.rs -Run
    if ($calls.Count -ne 4 -or $calls[0] -notcontains '--lib' -or
        $calls[2] -notcontains 'renderer_process' -or $calls[1] -contains '--all-targets') {
        throw 'A narrow WebGL edit ran unrelated targets or omitted its downstream contract.'
    }
    if ([string]$env:CARGO_BUILD_JOBS -ne [string]$priorJobs -or [string]$env:RUST_TEST_THREADS -ne [string]$priorWorkers -or
        [string]$env:TEMP -ne [string]$priorTemp -or [string]$env:TMP -ne [string]$priorTmp) { throw 'Runner leaked environment changes.' }
    $emptyListing=$true; $rejected=$false; $calls.Clear()
    try {
        & (Join-Path $PSScriptRoot 'run-impacted-tests.ps1') -ChangedPath src/engine/webgl/command_batch/uploads.rs -Run
    } catch { $rejected=$_.Exception.Message -like '*selected zero tests*' }
    if (-not $rejected -or $calls.Count -ne 1 -or $env:TEMP -ne $priorTemp) {
        throw 'An empty filter silently passed or failed to restore its environment.'
    }
} finally {
    [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR',$priorTarget,'Process')
}
Write-Host 'Impacted-test runner selection, zero-match rejection and environment restoration tests passed (Cargo mocked).'
