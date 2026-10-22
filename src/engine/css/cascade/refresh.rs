//! Incremental computed-style refresh over bounded, independent dirty subtrees.

use super::*;
use crate::engine::invalidation::InvalidationImpact;

#[cfg(test)]
mod deferred_paint_tests;
#[cfg(test)]
mod effect_roots_tests;
mod element_roots;
#[cfg(test)]
mod element_roots_tests;

impl StyleSet {
    pub(crate) fn refresh_subtrees(
        &mut self,
        document: &NodeRef,
        requested_roots: &[NodeRef],
        removed_nodes: &[NodeId],
    ) -> StyleRefreshStats {
        self.refresh_subtrees_after_invalidation(
            document,
            requested_roots,
            &[],
            removed_nodes,
            InvalidationImpact::default(),
        )
    }

    pub(crate) fn refresh_subtrees_after_invalidation(
        &mut self,
        document: &NodeRef,
        requested_roots: &[NodeRef],
        element_roots: &[NodeRef],
        removed_nodes: &[NodeId],
        impact: InvalidationImpact,
    ) -> StyleRefreshStats {
        // The new text/graphics proof is limited to ordinary document rules.
        // Part, slot, host and scope conditions have separate selector graphs;
        // do not infer their dependencies from the ordinary selector alone.
        let unsupported_local_scope = self.compiled.cross_tree_or_scoped
            && impact.is_local_content_only()
            && !impact.is_style_only();
        let mut changed_has_results = Vec::new();
        let roots = if unsupported_local_scope {
            vec![document.clone()]
        } else if self
            .compiled
            .nonlocal_dependencies
            .needs_document_refresh(impact)
        {
            // A local equality check cannot conceal a changed ancestor or
            // sibling :has() result. Widen only when this mutation can affect
            // the compiled relative-selector dependencies, and then only to
            // the anchors whose results can change when that is provable.
            let changed = requested_roots
                .iter()
                .chain(element_roots)
                .cloned()
                .collect::<Vec<_>>();
            match self
                .compiled
                .nonlocal_dependencies
                .relational_roots(&changed, &self.has_results)
                .filter(|_| !self.compiled.cross_tree_or_scoped)
            {
                Some(relational) => {
                    changed_has_results = relational.results;
                    let mut roots = relational.roots;
                    roots.extend_from_slice(requested_roots);
                    normalized_roots(document, &roots, element_roots.is_empty())
                }
                None => vec![document.clone()],
            }
        } else {
            normalized_roots(document, requested_roots, element_roots.is_empty())
        };
        let mut stats = StyleRefreshStats {
            opacity_only: true,
            ..StyleRefreshStats::default()
        };
        self.refresh_roots(&roots, element_roots, &mut stats);

        // A node may be removed and reinserted before the rendering checkpoint. Its identifier
        // remains in the removal log, but its newly recomputed style must remain available. Sweep
        // connectivity once for the entire root set rather than once per component. Without
        // removals no node left the document, and without fullscreen elements there are no
        // fullscreen roots to find, so the sweep is skipped.
        let mut fullscreen = Vec::new();
        let connected_nodes = if !removed_nodes.is_empty() || Node::any_fullscreen_elements() {
            Node::shadow_including_descendants(document)
                .map(|node| {
                    if node.is_fullscreen() {
                        fullscreen.push(node.clone());
                    }
                    node_id(&node)
                })
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        };
        let disconnected = removed_nodes
            .iter()
            .filter(|node| !connected_nodes.contains(node))
            .copied()
            .collect::<HashSet<_>>();
        stats.removed_styles += disconnected
            .iter()
            .filter(|node| self.styles.remove(node).is_some())
            .count();
        // Disconnected nodes' former ancestors carry new DOM versions; their generations go.
        for node in &disconnected {
            self.layout_generations.forget(*node);
        }
        // Anchors evaluated while choosing roots supersede what their recalculation recorded.
        self.has_results
            .retain(|node, _| !disconnected.contains(node));
        self.has_results.extend(changed_has_results);
        let removed_pseudos = self.remove_generated_pseudos(&disconnected);
        stats.non_deferable_paint_changes |= removed_pseudos;
        self.refresh_deferred_fullscreen_roots(&fullscreen, &mut stats);
        stats.non_deferable_paint_changes |= stats.removed_styles != 0;
        stats.opacity_only &=
            !stats.layout_changed && stats.removed_styles == 0 && !removed_pseudos;
        stats.total_styles = self.styles.len();
        stats
    }

    pub(super) fn compute_subtree(&mut self, node: &NodeRef, parent: Option<&ComputedStyle>) {
        let root = node_id(node);
        let mut pending = vec![node.clone()];
        while let Some(node) = pending.pop() {
            let style = if node_id(&node) == root {
                self.compute_style(&node, parent)
            } else {
                let parent = Node::composed_parent(&node)
                    .expect("connected composed-tree child has a parent");
                let parent_style = self
                    .styles
                    .get(&node_id(&parent))
                    .expect("parent style is computed before its children");
                self.compute_style(&node, Some(parent_style))
            };
            self.remember_has_results(&node, node_id(&node) == root);
            let displayed = style.display != Display::None;
            self.styles.insert(node_id(&node), style);
            self.style_log.record(node_id(&node));
            self.layout_generations.mark(&node);
            self.sync_generated_pseudos(&node);
            if !self.defer_nonrendered_descendants || displayed {
                pending.extend(Node::composed_children(&node).into_iter().rev());
            }
        }
    }

    pub(super) fn recompute_subtree(
        &mut self,
        node: &NodeRef,
        parent: Option<&ComputedStyle>,
        stats: &mut StyleRefreshStats,
    ) {
        let root = node_id(node);
        let mut pending = vec![node.clone()];
        while let Some(node) = pending.pop() {
            let style_started = std::time::Instant::now();
            let style = if node_id(&node) == root {
                self.compute_style(&node, parent)
            } else {
                let parent = Node::composed_parent(&node)
                    .expect("connected composed-tree child has a parent");
                let parent_style = self
                    .styles
                    .get(&node_id(&parent))
                    .expect("parent style is recomputed before its children");
                self.compute_style(&node, Some(parent_style))
            };
            stats.element_style_time += style_started.elapsed();
            stats.invalidated_nodes += 1;
            stats.recomputed_styles += 1;
            let mut changed = true;
            match self.styles.get(&node_id(&node)) {
                Some(previous) if previous != &style => {
                    stats.first_changed_node.get_or_insert(node.id());
                    stats.changed_styles += 1;
                    stats.layout_changed |= !previous.layout_equivalent(&style);
                    stats.non_deferable_paint_changes |=
                        !previous.deferred_paint_equivalent(&style);
                    stats.opacity_only &= previous.retained_opacity_equivalent(&style);
                }
                None => {
                    stats.first_changed_node.get_or_insert(node.id());
                    stats.changed_styles += 1;
                    stats.layout_changed = true;
                    stats.non_deferable_paint_changes = true;
                    stats.opacity_only = false;
                }
                _ => changed = false,
            }
            self.remember_has_results(&node, node_id(&node) == root);
            let displayed = style.display != Display::None;
            self.styles.insert(node_id(&node), style);
            self.style_log.record(node_id(&node));
            // Attribute-backed generated content and pseudo-only declarations can change box
            // geometry without changing the originating element's computed style.
            let pseudo_started = std::time::Instant::now();
            let pseudo_changes = self.sync_generated_pseudos(&node);
            if changed || pseudo_changes.layout_changed || pseudo_changes.paint_changed {
                self.layout_generations.mark(&node);
            }
            stats.layout_changed |= pseudo_changes.layout_changed;
            stats.non_deferable_paint_changes |= pseudo_changes.paint_changed;
            stats.opacity_only &= !pseudo_changes.paint_changed;
            if pseudo_changes.paint_changed {
                stats.first_changed_node.get_or_insert(node.id());
            }
            stats.pseudo_style_time += pseudo_started.elapsed();
            if !self.defer_nonrendered_descendants || displayed {
                pending.extend(Node::composed_children(&node).into_iter().rev());
            } else {
                stats.removed_styles += self.forget_deferred_styles(&node, false);
            }
        }
    }
}

impl StyleSet {
    /// Records the `:has()` results that `node`'s dependents are being styled with. A traversal
    /// root's later siblings are outside the traversal, so a root whose results later siblings
    /// may consult is forgotten instead, keeping the next mutation conservative.
    fn remember_has_results(&mut self, node: &NodeRef, traversal_root: bool) {
        let id = node_id(node);
        match self
            .compiled
            .nonlocal_dependencies
            .current_has_results(node)
        {
            Some((results, siblings)) if !(siblings && traversal_root) => {
                self.has_results.insert(id, results);
            }
            _ => {
                self.has_results.remove(&id);
            }
        }
    }
}

/// `default_document` makes an empty request mean the whole document; element roots alone
/// request no subtree.
fn normalized_roots(
    document: &NodeRef,
    requested: &[NodeRef],
    default_document: bool,
) -> Vec<NodeRef> {
    let mut roots = Vec::<NodeRef>::new();
    for requested in requested {
        let requested = requested.shadow_host().unwrap_or_else(|| requested.clone());
        let root = if is_descendant_of(&requested, document) {
            requested
        } else {
            document.clone()
        };
        if roots.iter().any(|known| is_descendant_of(&root, known)) {
            continue;
        }
        roots.retain(|known| !is_descendant_of(known, &root));
        roots.push(root);
    }
    if roots.is_empty() && default_document {
        roots.push(document.clone());
    }
    roots
}

fn is_descendant_of(node: &NodeRef, ancestor: &NodeRef) -> bool {
    std::iter::successors(Some(node.clone()), |current| {
        current.shadow_including_parent()
    })
    .any(|current| current.id() == ancestor.id())
}
