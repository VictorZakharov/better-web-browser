[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$resourceTests = @(
    'streaming_network::eight_streams_report_progress_and_an_abort_can_retry_without_blocking',
    'early_scroll_stays_responsive_during_post_load_selector_queries',
    'wheel_timing::continuous::rapid_wheel_input_keeps_one_continuous_native_animation',
    'wheel_timing::backlog::delayed_older_defaults_never_restart_after_a_physical_reverse',
    'wheel_timing::backlog::a_cancelled_physical_reverse_still_retires_older_viewport_defaults',
    'wheel_timing::backlog::physical_reverse_stops_active_motion_before_delayed_older_defaults_arrive'
)
# Wall-clock input latency and peak resident memory cannot be attributed to this
# browser while unrelated tests start other browser/renderer trees on the runner.
# Keep functional tests parallel, then measure unchanged latency/memory contracts alone.
$functionalArguments = @('test', '--locked', '--test', 'live_runtime', '--')
foreach ($resourceTest in $resourceTests) { $functionalArguments += @('--skip', $resourceTest) }
& cargo @functionalArguments
if ($LASTEXITCODE -ne 0) { throw 'Live browser functional tests failed.' }
foreach ($resourceTest in $resourceTests) {
    & cargo test --locked --test live_runtime $resourceTest -- --exact --nocapture
    if ($LASTEXITCODE -ne 0) { throw "Isolated resource acceptance failed: $resourceTest" }
}
