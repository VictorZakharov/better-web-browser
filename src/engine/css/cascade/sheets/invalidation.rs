//! Rule invalidation is a request to check inputs, not proof that every rule changed.
use super::*;
use crate::engine::invalidation::RenderInvalidation;
#[cfg(test)]
mod tests;

impl StyleSet {
    pub(crate) fn refresh_rules_after_invalidation(
        &mut self,
        dom: &Dom,
        base_url: &str,
        sources: &[StylesheetSource],
        environment: MediaEnvironment,
        invalidation: &RenderInvalidation,
    ) -> StyleRefreshStats {
        let compiled = collect(&dom.document, base_url, sources, environment);
        // Sharing requires exact source text, source order, URLs, shadow scope and media
        // environment. Inline declarations also resolve URLs against the document base.
        // An identical rule set does not make DOM changes a no-op: refresh every dirty
        // subtree, including inheritance, structural selectors and generated content.
        if Rc::ptr_eq(&compiled, &self.compiled)
            && self.document_base_url == base_url
            && !invalidation.roots.is_empty()
            && !invalidation.roots.contains(&dom.document.id())
        {
            let roots = invalidation
                .roots
                .iter()
                .filter_map(|id| dom.find_node(*id))
                .collect::<Vec<_>>();
            let (removed_styles, removed_generated) = self.prune_uncomposed_styles(dom);
            let mut stats =
                self.refresh_subtrees(&dom.document, &roots, &invalidation.removed_nodes);
            stats.removed_styles += removed_styles;
            stats.layout_changed |= removed_styles != 0 || removed_generated;
            return stats;
        }
        self.rebuild_rules_for_media_environment(
            dom,
            base_url,
            sources,
            environment,
            &invalidation.removed_nodes,
        )
    }

    pub(super) fn prune_uncomposed_styles(&mut self, dom: &Dom) -> (usize, bool) {
        // Attaching a shadow tree can leave the rule inputs identical while changing which
        // nodes participate in the composed tree. Drop stale light-DOM/pseudo entries even
        // without an explicit removal log, so later CSSOM queries hydrate them afresh.
        let composed = Node::composed_descendants(&dom.document)
            .map(|node| node.id())
            .collect::<HashSet<_>>();
        let previous_count = self.styles.len();
        let previous_generated_count = self.generated_nodes.len();
        self.styles.retain(|node, _| composed.contains(node));
        self.pseudo_styles
            .retain(|(origin, _), _| composed.contains(origin));
        self.generated_nodes
            .retain(|(origin, _), _| composed.contains(origin));
        let generated = self
            .generated_nodes
            .values()
            .flat_map(Node::descendants)
            .map(|node| node.id())
            .collect::<HashSet<_>>();
        self.generated_styles
            .retain(|node, _| generated.contains(node));
        (
            previous_count - self.styles.len(),
            previous_generated_count != self.generated_nodes.len(),
        )
    }
}
