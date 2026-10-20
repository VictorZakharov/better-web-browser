//! Element roots: recalculate one element and continue into its subtree only on change.
//
// A selector input that changed for exactly this element (for example one of its own class
// tokens) cannot change a descendant except through inheritance or generated content, both of
// which flow from the element's own computed style. Descendants whose selectors read the
// changed input are separate roots, so an unchanged element ends the walk here.
use super::*;

impl StyleSet {
    /// Recomputes `subtrees` whole and `elements` with an early cutoff, top-down, so that a
    /// node inside a subtree already recomputed in this pass (a subtree root, or an element
    /// root whose style changed) is not recomputed again.
    pub(super) fn refresh_roots(
        &mut self,
        subtrees: &[NodeRef],
        elements: &[NodeRef],
        stats: &mut StyleRefreshStats,
    ) {
        let mut roots = subtrees
            .iter()
            .map(|node| (node, true))
            .chain(elements.iter().map(|node| (node, false)))
            .map(|(node, subtree)| {
                let ancestors = std::iter::successors(Some(node.clone()), |node| {
                    node.shadow_including_parent()
                })
                .map(|ancestor| node_id(&ancestor))
                .collect::<Vec<_>>();
                (ancestors, !subtree, node)
            })
            .collect::<Vec<_>>();
        // Ancestors first; at equal depth a subtree root goes before an element root.
        roots.sort_by_key(|(ancestors, element, _)| (ancestors.len(), *element));
        let mut covered = HashSet::new();
        for (ancestors, element, node) in roots {
            if ancestors.iter().any(|ancestor| covered.contains(ancestor)) {
                continue;
            }
            if self.has_deferred_ancestor(node) {
                if !element {
                    stats.removed_styles += self.forget_deferred_styles(node, true);
                }
                continue;
            }
            if element {
                if self.recompute_element_with_cutoff(node, stats) {
                    covered.insert(node_id(node));
                }
            } else {
                let parent_style = Node::composed_parent(node)
                    .and_then(|parent| self.styles.get(&node_id(&parent)).cloned());
                self.recompute_subtree(node, parent_style.as_ref(), stats);
                covered.insert(node_id(node));
            }
        }
    }

    /// Returns whether the element's style changed, in which case its subtree was recomputed.
    fn recompute_element_with_cutoff(
        &mut self,
        node: &NodeRef,
        stats: &mut StyleRefreshStats,
    ) -> bool {
        let id = node_id(node);
        let parent_style = Node::composed_parent(node)
            .and_then(|parent| self.styles.get(&node_id(&parent)).cloned());
        let started = std::time::Instant::now();
        let style = self.compute_style(node, parent_style.as_ref());
        stats.element_style_time += started.elapsed();
        if self.styles.get(&id) != Some(&style) {
            self.recompute_subtree(node, parent_style.as_ref(), stats);
            return true;
        }
        stats.invalidated_nodes += 1;
        stats.recomputed_styles += 1;
        self.remember_has_results(node, true);
        let started = std::time::Instant::now();
        let pseudo_changes = self.sync_generated_pseudos(node, &style);
        stats.pseudo_style_time += started.elapsed();
        stats.layout_changed |= pseudo_changes.layout_changed;
        stats.non_deferable_paint_changes |= pseudo_changes.paint_changed;
        stats.opacity_only &= !pseudo_changes.paint_changed;
        if pseudo_changes.paint_changed {
            stats.first_changed_node.get_or_insert(node.id());
        }
        false
    }
}
