//! Same-document history traversal and viewport restoration.

use super::*;
use crate::renderer_protocol::{HistoryTraversalInput, ScrollRestorationMode};

impl DocumentRuntime {
    pub(super) fn apply_history_input(&mut self, input: HistoryTraversalInput) -> ScriptOutcome {
        let mut outcome = self
            .script_runtime
            .as_mut()
            .map(|runtime| {
                runtime.apply_history_traversal(
                    &input.url,
                    input.state.as_deref(),
                    input.history_length,
                    input.history_index,
                    input.scroll_restoration,
                )
            })
            .unwrap_or_default();
        self.page.source_url.clone_from(&input.url);
        self.reader.source_url.clone_from(&input.url);
        let final_mode =
            outcome
                .history_actions
                .iter()
                .fold(input.scroll_restoration, |mode, action| match action {
                    crate::engine::script::ScriptHistoryAction::SetScrollRestoration { mode } => {
                        *mode
                    }
                    _ => mode,
                });
        // A popstate handler can choose a new scroll position. Otherwise only `auto`
        // may restore saved viewport data or use the fragment fallback. Its setter runs
        // before persisted scroll state is applied, so use the post-popstate mode.
        if outcome.viewport_scroll_y.is_none() && final_mode == ScrollRestorationMode::Auto {
            outcome.viewport_scroll_y = input.scroll_y.or_else(|| {
                crate::engine::fragment_navigation::scroll_to_fragment(
                    &self.page.dom.document,
                    &input.url,
                    &self.layout.node_bounds,
                    &self.layout.scroll_boxes,
                    self.layout.content_height - self.viewport.height,
                )
            });
        }
        if let Some(y) = outcome.viewport_scroll_y {
            self.page.dom.document.scroll_offset.set((0.0, y));
        }
        outcome.request_full_render();
        outcome
    }
}
