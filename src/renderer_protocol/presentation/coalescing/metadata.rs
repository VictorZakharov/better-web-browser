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
    merged_runtime_bytes(&previous.runtime, &next.runtime).is_some_and(|runtime| {
        runtime.saturating_add(runtime_update_overhead(next)) <= MAX_CONTROL_PAYLOAD
    })
}

pub(super) fn runtime_update_overhead(value: &RendererRuntimeUpdate) -> usize {
    // Document id, clock flag, eighteen load counters, and the optional timer.
    8 + 1 + 18 * 8 + 1 + usize::from(value.next_timer_micros.is_some()) * 8
}

pub(super) fn runtime_bytes(value: &RuntimeReport) -> usize {
    edge_bytes(value).saturating_add(snapshot_bytes(value, value))
}

fn merged_runtime_bytes(previous: &RuntimeReport, next: &RuntimeReport) -> Option<usize> {
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
    .fold(0_usize, |bytes, text| {
        bytes.saturating_add(4).saturating_add(text.len())
    });
    value
        .history_actions
        .iter()
        .fold(strings, |bytes, action| match action {
            HistoryAction::Update { url, state, .. } => bytes
                .saturating_add(7)
                .saturating_add(url.len())
                .saturating_add(
                    state
                        .as_ref()
                        .map_or(0, |state| 4_usize.saturating_add(state.len())),
                ),
            HistoryAction::Traverse { .. } => bytes.saturating_add(5),
        })
}

fn snapshot_bytes(previous: &RuntimeReport, next: &RuntimeReport) -> usize {
    // Fixed fields and vector prefixes in encode_runtime, excluding optional bodies.
    let mut bytes = 47_usize;
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
