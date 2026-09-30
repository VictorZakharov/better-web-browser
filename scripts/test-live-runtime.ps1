[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$performanceTests = @(
    'early_scroll_stays_responsive_during_post_load_selector_queries',
    'wheel_timing::continuous::rapid_wheel_input_keeps_one_continuous_native_animation'
)
# Wall-clock input latency cannot be attributed to this browser while unrelated
# integration tests start other browser/renderer trees on the same runner.
# Keep functional tests parallel, then measure the unchanged performance contract alone.
$functionalArguments = @('test', '--locked', '--test', 'live_runtime', '--')
foreach ($performanceTest in $performanceTests) { $functionalArguments += @('--skip', $performanceTest) }
& cargo @functionalArguments
if ($LASTEXITCODE -ne 0) { throw 'Live browser functional tests failed.' }
foreach ($performanceTest in $performanceTests) {
    & cargo test --locked --test live_runtime $performanceTest -- --exact --nocapture
    if ($LASTEXITCODE -ne 0) { throw "Isolated scroll acceptance failed: $performanceTest" }
}
