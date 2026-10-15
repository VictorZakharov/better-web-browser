$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'full-test-plan.ps1')
$names = @('engine::script::tests::attributes::first', 'engine::script::tests::attributes::second',
    'engine::script::tests::canvas::mixed_native_contract', 'engine::script::tests::canvas_text::pixels',
    'engine::script::tests::parent::direct', 'engine::script::tests::parent::webgl::native',
    'engine::script::tests::parent::child::cpu', 'engine::css::tests::parse',
    'future::unknown::case', 'gpu_composition::tests::pixels')
foreach ($workers in 1..3) {
    $plan = Get-FullLibraryTestPlan -Names $names -CpuWorkers $workers
    if ($plan.Total -ne 10 -or $plan.Cpu.Count -ne $workers -or $plan.NativeNames.Count -ne 5) {
        throw 'Native, mixed-parent or unknown tests escaped conservative classification.'
    }
    $seen = @($plan.NativeNames)
    foreach ($shard in $plan.Cpu) {
        $selected = @($names | Where-Object {
            $name = $_
            @($shard.Prefixes | Where-Object { $name.Contains($_, [StringComparison]::Ordinal) }).Count -gt 0
        })
        if (@(Compare-Object ($selected | Sort-Object) ($shard.Names | Sort-Object)).Count) {
            throw 'Real libtest substring filters differ from the predicted CPU partition.'
        }
        $seen += $selected
    }
    if ($seen.Count -ne 10 -or @($seen | Select-Object -Unique).Count -ne 10) { throw 'Coverage changed with worker count.' }
}
$unknown = Get-FullLibraryTestPlan -Names @('new::unknown::test')
if ($unknown.NativeNames.Count -ne 1 -or $unknown.CpuPrefixes.Count) { throw 'Unknown tests must stay serial.' }
$rejected = $false
try { Get-FullLibraryTestPlan -Names @('duplicate', 'duplicate') } catch { $rejected = $true }
if (-not $rejected) { throw 'Duplicate discovery was accepted.' }
$ambiguous = Get-FullLibraryTestPlan -Names @('engine::css::tests::parse', 'unknown::engine::css::tests::parse')
if ($ambiguous.NativeNames.Count -ne 2 -or $ambiguous.CpuPrefixes.Count) {
    throw 'A substring collision with an unknown root entered a CPU worker.'
}
$storage = Get-FullLibraryTestPlan -Names @('storage::sync_tests::order', 'cache_storage::tests::quota',
    'renderer_protocol::tests::round_trip', 'renderer_protocol::gpu::tests::frame')
if ($storage.NativeNames.Count -ne 1 -or $storage.NativeNames[0] -cne 'renderer_protocol::gpu::tests::frame') {
    throw 'Pure storage/protocol coverage or conservative native GPU classification changed.'
}
Write-Host 'Full-test resource partitions passed: exact coverage, mixed parents, unknowns and 1-3 CPU processes.'

$browserNames = @('network::fetch', 'parser_writes::order', 'indexed_db::retirement',
    'streaming_network::peak_memory', 'wheel_timing::latency', 'new::unknown', 'Network::case_sensitive',
    'network::peak_memory', 'dynamic_scripts::concurrent_request_start_spread')
foreach ($workers in 1..2) {
    $plan = Get-BrowserTestPlan -Names $browserNames -Workers $workers
    $seen = @($plan.Exclusive) + @($plan.Functional | ForEach-Object { $_.Names.ToArray() })
    if ($plan.Functional.Count -ne $workers -or $plan.Exclusive.Count -ne 6 -or
        $seen.Count -ne $browserNames.Count -or @($seen | Select-Object -Unique).Count -ne $seen.Count -or
        @(Compare-Object ($seen | Sort-Object) ($browserNames | Sort-Object) -CaseSensitive).Count) {
        throw 'Browser scheduling changed coverage or admitted an unknown/resource-sensitive test.'
    }
}
Write-Host 'Browser partitions passed: exact functional coverage and exclusive memory/latency/unknown cases.'
$renderer = Get-BrowserTestPlan -Names @('input::click', 'async_scripts::parsing::order',
    'async_scripts::rendering::canvas_pixels', 'webgl::pixels', 'media::decode', 'backpressure::queue', 'new::unknown') -Target renderer_process
if ($renderer.Exclusive.Count -ne 5 -or @($renderer.Functional | ForEach-Object { $_.Names.ToArray() }).Count -ne 2) {
    throw 'Renderer concurrency admitted GPU/media/backpressure/unknown work or omitted functional tests.'
}
Write-Host 'Renderer partitions passed: concurrent functional input/parsing only; GPU/media/backpressure/unknown work stays serial.'
$native = Get-BrowserTestPlan -Names @('webgl::pixels', 'canvas_frames::frames', 'media::decode',
    'media::cadence::callback', 'new::unknown', 'backpressure::queue') -Target renderer_process
if ($native.EarlyNative.Count -ne 4 -or @($native.Functional | ForEach-Object { $_.Names.ToArray() }).Count -or
    @($native.EarlyNative | Where-Object { $_ -match 'cadence|unknown' }).Count) {
    throw 'Serial native scheduling admitted a measurement/unknown case or allowed concurrent GPU tests.'
}
