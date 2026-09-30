//! Read-only sizing of the runtime wire format: compaction must not turn valid messages
//! into an over-limit vector or payload. Sizing never clones strings or bitmap buffers.

use super::{RendererPresentation, RendererRuntimeUpdate, RuntimeReport, resources};
use crate::limits::{
    MAX_CONTROL_PAYLOAD, MAX_RENDERER_PRESENTATION_BYTES, MAX_RUNTIME_REPORT_ENTRIES,
};
use crate::renderer_protocol::HistoryAction;

pub(super) fn presentation_merge_is_bounded(
    previous: &RendererPresentation,
    next: &RendererPresentation,
) -> bool {
    // A cancellation verdict must be installed against its own control snapshot;
    // a later presentation may already contain script-written or new-generation text.
    if previous.runtime.native_text_rejection.is_some()
        || wheel_direction_changed(&previous.runtime, &next.runtime)
    {
        return false;
    }
    // The benchmark's nested-motion paint must install the verdict's own revision,
    // never a later resource/script snapshot that may already replace that scroll.
    if previous
        .runtime
        .wheel_acknowledgements
        .iter()
        .any(|value| value.decision == crate::renderer_protocol::WheelDecision::NestedScroll)
    {
        return false;
    }
    resources::merged_bytes(previous, next)
        .zip(merged_runtime_bytes(&previous.runtime, &next.runtime))
        .is_some_and(|(resources, runtime)| {
            resources.saturating_add(runtime) <= MAX_RENDERER_PRESENTATION_BYTES
        })
}

pub(super) fn runtime_merge_is_bounded(
    previous: &RendererRuntimeUpdate,
    next: &RendererRuntimeUpdate,
) -> bool {
    if previous.runtime.native_text_rejection.is_some()
        || wheel_direction_changed(&previous.runtime, &next.runtime)
    {
        return false;
    }
    merged_runtime_bytes(&previous.runtime, &next.runtime).is_some_and(|runtime| {
        runtime.saturating_add(runtime_update_overhead(next)) <= MAX_CONTROL_PAYLOAD
    })
}

fn wheel_direction_changed(previous: &RuntimeReport, next: &RuntimeReport) -> bool {
    // A new absolute scroll deliberately supersedes old wheel motion. Otherwise
    // preserve reversals as separate delivery turns: summing them erases the user's
    // interruption and leaves the shell animating toward the old distant target.
    next.viewport_scroll_y.is_none()
        && previous.viewport_wheel_delta_y != 0.0
        && next.viewport_wheel_delta_y != 0.0
        && previous.viewport_wheel_delta_y.is_sign_positive()
            != next.viewport_wheel_delta_y.is_sign_positive()
}

pub(super) fn runtime_update_overhead(value: &RendererRuntimeUpdate) -> usize {
    // Document id, clock flag, eighteen load counters, and the optional timer.
    8 + 1 + 18 * 8 + 1 + usize::from(value.next_timer_micros.is_some()) * 8
}

pub(super) fn runtime_bytes(value: &RuntimeReport) -> usize {
    edge_bytes(value).saturating_add(snapshot_bytes(value, value))
}

fn merged_runtime_bytes(previous: &RuntimeReport, next: &RuntimeReport) -> Option<usize> {
    if previous
        .wheel_acknowledgements
        .len()
        .saturating_add(next.wheel_acknowledgements.len())
        > super::super::MAX_WHEEL_ACKNOWLEDGEMENTS
    {
        return None;
    }
    if previous.native_text_rejection.is_some() && next.native_text_rejection.is_some() {
        return None;
    }
    let counts = |value: &RuntimeReport| {
        [
            value.errors.len(),
            value.console.len(),
            value.diagnostics.len(),
            value.history_actions.len(),
            value.cookie_updates.len(),
        ]
    };
    if counts(previous)
        .into_iter()
        .zip(counts(next))
        .any(|(a, b)| a.saturating_add(b) > MAX_RUNTIME_REPORT_ENTRIES)
    {
        return None;
    }
    Some(
        edge_bytes(previous)
            .saturating_add(edge_bytes(next))
            .saturating_add(snapshot_bytes(previous, next)),
    )
}

fn edge_bytes(value: &RuntimeReport) -> usize {
    let strings = [
        &value.errors,
        &value.console,
        &value.diagnostics,
        &value.cookie_updates,
    ]
    .into_iter()
    .flatten()
    .fold(
        value.wheel_acknowledgements.len().saturating_mul(21),
        |bytes, text| bytes.saturating_add(4).saturating_add(text.len()),
    );
    value
        .history_actions
        .iter()
        .fold(strings, |bytes, action| match action {
            HistoryAction::Update { url, state, .. } => bytes
                .saturating_add(11)
                .saturating_add(url.len())
                .saturating_add(
                    state
                        .as_ref()
                        .map_or(0, |state| 4_usize.saturating_add(state.len())),
                ),
            HistoryAction::Traverse { .. } => bytes.saturating_add(5),
            HistoryAction::SetScrollRestoration { .. } => bytes.saturating_add(2),
        })
}

fn snapshot_bytes(previous: &RuntimeReport, next: &RuntimeReport) -> usize {
    // Fixed fields and vector prefixes in encode_runtime, excluding optional bodies.
    let mut bytes = 52_usize;
    if let Some(rejection) = next
        .native_text_rejection
        .as_ref()
        .or(previous.native_text_rejection.as_ref())
    {
        bytes = bytes.saturating_add(8 + 4 + 16 + 4 + rejection.value.len() + 4 + 4);
    }
    if next
        .history_traversal_ack
        .or(previous.history_traversal_ack)
        .is_some()
    {
        bytes = bytes.saturating_add(8);
    }
    let navigation = if next.navigation_url.is_some() {
        next
    } else {
        previous
    };
    if let Some(url) = &navigation.navigation_url {
        bytes = bytes
            .saturating_add(13)
            .saturating_add(url.len())
            .saturating_add(navigation.navigation_options.target.len());
        if let Some(post) = &navigation.navigation_options.post {
            bytes = bytes
                .saturating_add(8)
                .saturating_add(post.content_type.len())
                .saturating_add(post.body.len());
        }
    }
    if next
        .viewport_scroll_y
        .or(previous.viewport_scroll_y)
        .is_some()
    {
        bytes = bytes.saturating_add(4);
    }
    if let Some(media) = &next.media {
        bytes = bytes
            .saturating_add(80)
            .saturating_add(media.backend.len())
            .saturating_add(media.mime_type.len())
            .saturating_add(media.video_codec.len())
            .saturating_add(media.audio_codec.len());
        if let Some(failure) = &media.failure {
            bytes = bytes.saturating_add(4).saturating_add(failure.len());
        }
    }
    bytes
}

#[cfg(test)]
mod tests;
