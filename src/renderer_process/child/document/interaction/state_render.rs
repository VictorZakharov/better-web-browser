//! Targeted state invalidation for authoritative scriptless control edits.
use super::*;

/// Requests a targeted state-invalidation render after scriptless control
/// edits: the control's own subtree root plus form-owner/fieldset aggregation
/// roots (HTML `:valid` / `:invalid` on `form` / `fieldset`), radio-group
/// peers, and their aggregates. Never the whole document.
pub(super) fn request_state_render(control: &NodeRef, outcome: &mut ScriptOutcome) {
    use crate::engine::invalidation::validation_aggregation_roots;
    let mut roots = Vec::new();
    let mut push_with_aggregates = |node: &NodeRef| {
        // A changed peer's siblings can match :checked + .label too, even
        // when that radio lives under a different parent from the target.
        roots.push(
            node.shadow_including_parent()
                .unwrap_or_else(|| node.clone())
                .id(),
        );
        roots.extend(
            validation_aggregation_roots(node)
                .iter()
                .map(|root| root.id()),
        );
    };
    push_with_aggregates(control);
    if control.is_radio() {
        for peer in control.radio_group() {
            if peer.id() != control.id() {
                push_with_aggregates(&peer);
            }
        }
    }
    roots.sort_unstable();
    roots.dedup();
    outcome.request_full_render();
    outcome.invalidation = crate::engine::invalidation::RenderInvalidation {
        roots,
        impact: crate::engine::invalidation::MutationKind::State.impact(),
        mutation_count: 0,
        rebuild_style_rules: false,
        removed_nodes: Vec::new(),
        removals_are_local: false,
    };
}
