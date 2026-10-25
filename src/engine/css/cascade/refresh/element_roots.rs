//! Element roots: recalculate one element and continue into its subtree only on change.
//
// A selector input that changed for exactly this element (for example one of its own class
// tokens) cannot change a descendant except through inheritance or generated content, both of
// which flow from the element's own computed style. Descendants whose selectors read the
// changed input are separate roots, so an unchanged element ends the walk here. The same holds
// below a changed element: a child's style depends only on its own inputs and its parent's style
// (the root font size travels down as part of it), so the walk continues only below children
// whose style changed in turn. A property change that nothing inherits (`scroll-padding` on the
// root) then restyles the root and its children, not the document.
use super::*;

impl StyleSet {
    /// Moves the invalidation's element roots (`RenderInvalidation::element_roots`) from
    /// `subtrees` to `elements`. Run it after the child-list resolution, which finds its parents
    /// among `subtrees` too; a root no longer there was already resolved or is covered.
    pub(crate) fn resolve_element_roots(
        &self,
        ids: &[NodeId],
        subtrees: &mut Vec<NodeRef>,
        elements: &mut Vec<NodeRef>,
    ) {
        for id in ids {
            if let Some(position) = subtrees.iter().position(|root| root.id() == *id) {
                elements.push(subtrees.remove(position));
            }
        }
    }

    /// Recomputes `subtrees` whole and `elements` with an early cutoff, top-down, so that a
    /// node already recomputed in this pass (inside a subtree root, or reached from an element
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
        let subtree_ids = subtrees.iter().map(node_id).collect::<HashSet<_>>();
        // Subtrees recomputed whole, and elements a cutoff walk already reached.
        let mut covered = HashSet::new();
        let mut reached = HashSet::new();
        for (ancestors, element, node) in roots {
            if ancestors.iter().any(|ancestor| covered.contains(ancestor))
                || (element && reached.contains(&node_id(node)))
            {
                continue;
            }
            if self.has_deferred_ancestor(node) {
                if !element {
                    stats.removed_styles += self.forget_deferred_styles(node, true);
                }
                continue;
            }
            if element {
                let walk = CutoffWalk {
                    subtree_roots: &subtree_ids,
                    covered: &mut covered,
                    reached: &mut reached,
                };
                self.recompute_element_with_cutoff(node, walk, stats);
            } else {
                let parent_style = Node::composed_parent(node)
                    .and_then(|parent| self.styles.get(&node_id(&parent)).cloned());
                self.recompute_subtree(node, parent_style.as_deref(), stats);
                covered.insert(node_id(node));
            }
        }
    }

    /// Recomputes the element and, when its style changed, the descendants it reaches: children
    /// of changed elements, and whole subtrees from each subtree root of the pass it meets.
    fn recompute_element_with_cutoff(
        &mut self,
        node: &NodeRef,
        walk: CutoffWalk<'_>,
        stats: &mut StyleRefreshStats,
    ) {
        let id = node_id(node);
        let parent_style = Node::composed_parent(node)
            .and_then(|parent| self.styles.get(&node_id(&parent)).cloned());
        let started = std::time::Instant::now();
        let style = self.compute_style(node, parent_style.as_deref());
        stats.element_style_time += started.elapsed();
        if self.styles.get(&id).map(Arc::as_ref) != Some(&style) {
            let CutoffWalk {
                subtree_roots,
                covered,
                reached,
            } = walk;
            let mut whole = HashSet::new();
            self.recompute_tree(node, parent_style.as_deref(), stats, |node, changed| {
                let id = node_id(node);
                reached.insert(id);
                let inside_whole = !whole.is_empty()
                    && Node::composed_parent(node)
                        .is_some_and(|parent| whole.contains(&node_id(&parent)));
                if subtree_roots.contains(&id) {
                    covered.insert(id);
                }
                if inside_whole || subtree_roots.contains(&id) {
                    whole.insert(id);
                    return true;
                }
                changed
            });
            return;
        }
        stats.invalidated_nodes += 1;
        stats.recomputed_styles += 1;
        self.remember_has_results(node, true);
        let started = std::time::Instant::now();
        let pseudo_changes = self.sync_generated_pseudos(node);
        stats.pseudo_style_time += started.elapsed();
        stats.layout_changed |= pseudo_changes.layout_changed;
        stats.non_deferable_paint_changes |= pseudo_changes.paint_changed;
        stats.opacity_only &= !pseudo_changes.paint_changed;
        stats.text_color_only &= !pseudo_changes.paint_changed;
        if pseudo_changes.paint_changed {
            stats.first_changed_node.get_or_insert(node.id());
        }
    }
}

/// What a cutoff walk shares with the rest of a `refresh_roots` pass.
struct CutoffWalk<'a> {
    subtree_roots: &'a HashSet<NodeId>,
    covered: &'a mut HashSet<NodeId>,
    reached: &'a mut HashSet<NodeId>,
}
