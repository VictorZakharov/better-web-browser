//! Rule invalidation is a request to check inputs, not proof that every rule changed.
use super::*;
use crate::engine::dom::node::NodeSet;
use crate::engine::invalidation::RenderInvalidation;
mod rule_diff;
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
            && !invalidation.is_empty()
            && !invalidation.roots.contains(&dom.document.id())
        {
            let mut roots = invalidation
                .roots
                .iter()
                .filter_map(|id| dom.find_node(*id))
                .collect::<Vec<_>>();
            let mut elements = Vec::new();
            let fallbacks = self.resolve_attribute_changes(
                dom,
                &invalidation.attribute_changes,
                &mut roots,
                &mut elements,
            );
            self.resolve_child_list_changes(
                dom,
                &invalidation.child_list_roots,
                &invalidation.removed_nodes,
                &mut roots,
                &mut elements,
            );
            self.resolve_element_roots(&invalidation.element_roots, &mut roots, &mut elements);
            let (removed_styles, removed_generated) = self.prune_uncomposed_styles(dom);
            let mut stats = self.refresh_subtrees_after_invalidation(
                &dom.document,
                &roots,
                &elements,
                &invalidation.removed_nodes,
                invalidation.impact,
            );
            stats.removed_styles += removed_styles;
            stats.layout_changed |= removed_styles != 0 || removed_generated;
            stats.non_deferable_paint_changes |= removed_styles != 0 || removed_generated;
            stats.invalidation_fallbacks = fallbacks;
            return stats;
        }
        // A changed rule set whose difference is whole, keyed rules restyles only the subjects
        // of those rules plus the mutation roots. A document root still means everything.
        if self.document_base_url == base_url
            && !invalidation.roots.contains(&dom.document.id())
            && let Some(diff) =
                rule_diff::changed_rule_subjects(&self.compiled, &compiled, &dom.document)
        {
            // Resolve class changes against the outgoing rules too: a removed rule may have
            // matched through a token that changed in the same checkpoint.
            let mut roots = invalidation
                .roots
                .iter()
                .filter_map(|id| dom.find_node(*id))
                .collect::<Vec<_>>();
            let mut elements = Vec::new();
            let outgoing = self.resolve_attribute_changes(
                dom,
                &invalidation.attribute_changes,
                &mut roots,
                &mut elements,
            );
            self.compiled = compiled;
            let mut fallbacks = outgoing.saturating_add(self.resolve_attribute_changes(
                dom,
                &invalidation.attribute_changes,
                &mut roots,
                &mut elements,
            ));
            fallbacks.rule_diff_subtrees += diff.subtrees.len() as u64;
            // Unchanged rules are all in the new set, so the new rules bound what an inserted
            // or removed child can restyle; elements a changed rule may match are subjects.
            self.resolve_child_list_changes(
                dom,
                &invalidation.child_list_roots,
                &invalidation.removed_nodes,
                &mut roots,
                &mut elements,
            );
            self.resolve_element_roots(&invalidation.element_roots, &mut roots, &mut elements);
            roots.extend(diff.subtrees);
            // Remembered :has() results follow the anchor order of the rules with :has(), so
            // relational changes also keep every key-matched subject.
            if diff.relational {
                self.has_results.clear();
                elements.extend(diff.subjects);
            } else {
                elements.extend(self.subjects_matched_by(&diff.changed, diff.subjects));
            }
            let (removed_styles, removed_generated) = self.prune_uncomposed_styles(dom);
            let mut stats = self.refresh_subtrees_after_invalidation(
                &dom.document,
                &roots,
                &elements,
                &invalidation.removed_nodes,
                invalidation.impact,
            );
            stats.removed_styles += removed_styles;
            stats.layout_changed |= removed_styles != 0 || removed_generated;
            stats.non_deferable_paint_changes |= removed_styles != 0 || removed_generated;
            stats.invalidation_fallbacks = fallbacks;
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
            .collect::<NodeSet<_>>();
        let previous_count = self.styles.len();
        let previous_generated_count = self.generated_nodes.len();
        self.styles.retain(|node, _| composed.contains(node));
        self.layout_generations
            .retain(|node| composed.contains(node));
        self.pseudo_styles
            .retain(|(origin, _), _| composed.contains(origin));
        self.generated_nodes
            .retain(|(origin, _), _| composed.contains(origin));
        let generated = self
            .generated_nodes
            .values()
            .flat_map(Node::descendants)
            .map(|node| node.id())
            .collect::<NodeSet<_>>();
        self.generated_styles
            .retain(|node, _| generated.contains(node));
        (
            previous_count - self.styles.len(),
            previous_generated_count != self.generated_nodes.len(),
        )
    }
}
