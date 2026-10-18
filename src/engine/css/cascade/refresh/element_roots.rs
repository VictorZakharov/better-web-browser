//! Element roots: recalculate one element and continue into its subtree only on change.
//
// A selector input that changed for exactly this element (for example one of its own class
// tokens) cannot change a descendant except through inheritance or generated content, both of
// which flow from the element's own computed style. Descendants whose selectors read the
// changed input are separate roots, so an unchanged element ends the walk here.
use super::*;

impl StyleSet {
    pub(super) fn refresh_element_roots(
        &mut self,
        elements: &[NodeRef],
        subtree_roots: &[NodeRef],
        stats: &mut StyleRefreshStats,
    ) {
        let covered = subtree_roots.iter().map(node_id).collect::<HashSet<_>>();
        for node in elements {
            let inside_subtree_root =
                std::iter::successors(Some(node.clone()), |node| node.shadow_including_parent())
                    .any(|ancestor| covered.contains(&node_id(&ancestor)));
            if inside_subtree_root || self.has_deferred_ancestor(node) {
                continue;
            }
            self.recompute_element_with_cutoff(node, stats);
        }
    }

    fn recompute_element_with_cutoff(&mut self, node: &NodeRef, stats: &mut StyleRefreshStats) {
        let id = node_id(node);
        let parent_style = Node::composed_parent(node)
            .and_then(|parent| self.styles.get(&node_id(&parent)).cloned());
        let started = std::time::Instant::now();
        let style = self.compute_style(node, parent_style.as_ref());
        stats.element_style_time += started.elapsed();
        if self.styles.get(&id) != Some(&style) {
            self.recompute_subtree(node, parent_style.as_ref(), stats);
            return;
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
    }
}
