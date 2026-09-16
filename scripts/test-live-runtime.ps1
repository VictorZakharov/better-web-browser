[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$performanceTest = 'early_scroll_stays_responsive_during_post_load_selector_queries'
# Wall-clock input latency cannot be attributed to this browser while unrelated
# integration tests start other browser/renderer trees on the same runner.
# Keep functional tests parallel, then measure the unchanged performance contract alone.
& cargo test --locked --test live_runtime -- --skip $performanceTest
if ($LASTEXITCODE -ne 0) { throw 'Live browser functional tests failed.' }
& cargo test --locked --test live_runtime $performanceTest -- --exact --nocapture
if ($LASTEXITCODE -ne 0) { throw 'Isolated early-scroll acceptance failed.' }
