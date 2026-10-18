//! Opt-in attribution of style ownership and retained rendering checkpoints.
use super::*;

fn style_owner(page: &Page, id: crate::engine::dom::NodeId) -> Option<String> {
    page.dom.find_node(id).and_then(|node| {
        node.tag_name().map(|tag| {
            format!(
                "{tag}#{}.{}",
                node.attr("id").unwrap_or_default(),
                node.attr("class").unwrap_or_default()
            )
        })
    })
}

impl DocumentRuntime {
    pub(super) fn record_render_checkpoint(
        &mut self,
        outcome: &mut ScriptOutcome,
        style: &StyleRefreshStats,
        style_time: Duration,
        layout_time: Duration,
        canvas_repaint: bool,
        retained_style_repaint: bool,
    ) {
        if self.diagnostic_selectors.is_empty() {
            return;
        }
        if let Some(diagnostic) = self.page.take_resource_diagnostic() {
            outcome.diagnostics.push(diagnostic);
        }
        if canvas_repaint {
            outcome
                .diagnostics
                .push("Canvas pixel checkpoint reused retained style and layout".into());
        }
        if retained_style_repaint {
            outcome
                .diagnostics
                .push(if style.opacity_only && style.changed_styles > 0 {
                    "Style checkpoint updated retained opacity groups without layout".into()
                } else {
                    "Style checkpoint retained exactly unchanged geometry and paint".into()
                });
        }
        outcome.diagnostics.push(format!(
            "render checkpoint: style/resources {:.3} ms (elements {:.3}, pseudos {:.3}), layout {:.3} ms; styles {}/{} changed, full rebuild {}, dirty roots {} {:?}, class changes {}, removed styles {}, local removals {}, geometry changed {}, opacity only {}, first changed {:?}",
            style_time.as_secs_f64() * 1000.0,
            style.element_style_time.as_secs_f64() * 1000.0,
            style.pseudo_style_time.as_secs_f64() * 1000.0,
            layout_time.as_secs_f64() * 1000.0,
            style.changed_styles, style.recomputed_styles, style.full_rebuild,
            outcome.invalidation.roots.len(),
            outcome.invalidation.roots.iter().take(8).map(|id| style_owner(&self.page, *id)).collect::<Vec<_>>(),
            outcome.invalidation.class_changes.len(),
            style.removed_styles, outcome.invalidation.removals_are_local,
            style.layout_changed, style.opacity_only,
            style.first_changed_node.and_then(|id| style_owner(&self.page, id)),
        ));
    }
}
