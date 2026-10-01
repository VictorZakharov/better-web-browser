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
    'input::hit_testing::native_hit_target_index_preserves_trusted_cancelled_wheels_on_a_long_plain_document',
    'input::publication_diagnostics::scoped_effect_hover_preserves_immediate_style_stats_and_timing_without_diagnostics',
    'input::publication_diagnostics::scoped_effect_hover_waits_for_tail_script_after_an_early_streamed_paint',
    'state::typed_state_snapshots_and_mutations_cross_the_renderer_boundary',
    'async_scripts::rendering::initial_stylesheets_block_paint_not_async_scripts_or_heartbeats',
    'backpressure::navigation_discards_a_queued_fetch_batch_from_the_replaced_document',
    'media::contained_renderer_decodes_and_presents_video_without_browser_frame_ownership',
    'media::audio_only::opus::contained_renderer_opus_mono_and_stereo_sources_play_and_seek_without_video',
    'media::audio_only::opus::contained_renderer_decodes_opus_audio_buffers_with_real_resampled_pcm_and_async_callbacks',
    'capture::opus_recorder::round_trip::hidden_renderer_emitted_chunks_decode_real_stereo_with_exact_lookahead_and_eos_trim',
    'canvas_presentation::modern_images::modern_image_pixels_cross_the_contained_renderer_without_double_premultiplication',
    'canvas_presentation::modern_images::malformed_modern_images_reject_without_stopping_the_contained_document',
    'canvas_presentation::image_frames::image_decoder_outputs_real_animation_pixels_in_the_contained_renderer',
    'canvas_presentation::image_frames::malformed_image_decoder_rejects_but_the_renderer_remains_usable',
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
