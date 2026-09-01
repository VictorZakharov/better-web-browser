use crate::engine::invalidation::RenderInvalidation;
use crate::engine::{ScriptOutcome, StyleRefreshStats};
use crate::renderer_protocol::{HistoryUpdate, MediaRuntimeReport, RuntimeReport, StyleReport};
use std::time::Duration;

pub(super) fn merge_outcome(
    target: &mut ScriptOutcome,
    mut source: ScriptOutcome,
    document_root: crate::engine::dom::NodeId,
) {
    if source.render_requested && source.invalidation.is_empty() {
        source.invalidation = RenderInvalidation::full(document_root);
    }
    target
        .invalidation
        .merge_conservatively(source.invalidation, document_root);
    target.executed = target.executed.saturating_add(source.executed);
    target.mutation_count = target.mutation_count.saturating_add(source.mutation_count);
    target.errors.append(&mut source.errors);
    target.console.append(&mut source.console);
    target.diagnostics.append(&mut source.diagnostics);
    if source.navigation_url.is_some() {
        target.navigation_url = source.navigation_url;
    }
    target.history_actions.append(&mut source.history_actions);
    target.cookie_updates.append(&mut source.cookie_updates);
    target.storage_updates.append(&mut source.storage_updates);
    target.fetch_actions.append(&mut source.fetch_actions);
    target.worker_actions.append(&mut source.worker_actions);
    target
        .fullscreen_actions
        .append(&mut source.fullscreen_actions);
    target.media_actions.append(&mut source.media_actions);
    target.runtime_stopped |= source.runtime_stopped;
    target.render_requested |= source.render_requested;
}

pub(super) fn runtime_report(
    mut outcome: ScriptOutcome,
    runtime_active: bool,
    media: Option<MediaRuntimeReport>,
) -> RuntimeReport {
    RuntimeReport {
        scripts_executed: outcome.executed as u64,
        dom_mutations: outcome.mutation_count as u64,
        errors: std::mem::take(&mut outcome.errors),
        console: std::mem::take(&mut outcome.console),
        diagnostics: std::mem::take(&mut outcome.diagnostics),
        navigation_url: outcome.navigation_url,
        history_updates: outcome
            .history_actions
            .into_iter()
            .map(|action| HistoryUpdate {
                url: action.url,
                replace: action.replace,
            })
            .collect(),
        cookie_updates: outcome.cookie_updates,
        runtime_active,
        runtime_stopped: outcome.runtime_stopped,
        render_requested: outcome.render_requested,
        media,
    }
}

pub(super) fn style_report(style: StyleRefreshStats) -> StyleReport {
    StyleReport {
        invalidated_nodes: style.invalidated_nodes as u64,
        total_styles: style.total_styles as u64,
        recomputed_styles: style.recomputed_styles as u64,
        changed_styles: style.changed_styles as u64,
        removed_styles: style.removed_styles as u64,
        layout_changed: style.layout_changed,
        full_rebuild: style.full_rebuild,
    }
}

pub(super) fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}
