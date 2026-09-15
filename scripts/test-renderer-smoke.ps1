[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
# Keep process isolation, recovery, native input, first paint, and media ownership
# on PRs. Main runs the complete renderer suite, including every new test.
$contracts = @(
    'app_container_denies_children_loopback_and_internet',
    'hidden_contained_renderer_handshakes_pings_and_shuts_down',
    'crashed_tab_renderer_preserves_its_sibling_and_can_be_reloaded',
    'fatal_native_failures_are_tab_local_and_reload_with_fresh_identity',
    'hung_task_is_detected_and_terminated_without_blocking_the_browser',
    'startup::startup_faults_fail_closed_within_the_deadline',
    'input::native_input_lifecycle_and_navigation_cross_the_real_renderer_boundary',
    'state::typed_state_snapshots_and_mutations_cross_the_renderer_boundary',
    'async_scripts::rendering::initial_stylesheets_block_paint_not_async_scripts_or_heartbeats',
    'backpressure::navigation_discards_a_queued_fetch_batch_from_the_replaced_document',
    'media::contained_renderer_decodes_and_presents_video_without_browser_frame_ownership',
    'media::cadence::video_pixels_advance_while_a_javascript_callback_owns_the_document_thread',
    'media::cadence::advancing_video_does_not_mask_a_document_watchdog_timeout',
    'media::failure::a_late_video_decode_error_does_not_stop_the_document'
)
$arguments = @('test', '--locked', '--test', 'renderer_process', '--', '--exact', '--include-ignored', '--test-threads=1') + $contracts
# libtest succeeds when a filter matches zero tests. Reject renamed/missing
# contracts before running, rather than silently losing a required smoke check.
$listed = & cargo @arguments --list
if ($LASTEXITCODE -ne 0) { throw 'Could not enumerate renderer smoke contracts.' }
$actual = @($listed | Where-Object { $_ -match ': test$' } | ForEach-Object { $_ -replace ': test$', '' })
if ($actual.Count -ne $contracts.Count -or @(Compare-Object $contracts $actual).Count -ne 0) {
    throw 'Renderer smoke selection does not match the required contracts.'
}
& cargo @arguments
if ($LASTEXITCODE -ne 0) { throw 'Renderer smoke contracts failed.' }
