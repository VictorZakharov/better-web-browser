//! Incremental computed-style refresh over bounded, independent dirty subtrees.

use super::*;

impl StyleSet {
    pub(crate) fn refresh_subtrees(
        &mut self,
        document: &NodeRef,
        requested_roots: &[NodeRef],
        removed_nodes: &[NodeId],
    ) -> StyleRefreshStats {
        let roots = normalized_roots(document, requested_roots);
        let mut stats = StyleRefreshStats::default();
        for root in roots {
            if self.has_deferred_ancestor(&root) {
                stats.removed_styles += self.forget_deferred_styles(&root, true);
                continue;
            }
            let parent_style = Node::composed_parent(&root)
                .and_then(|parent| self.styles.get(&node_id(&parent)).cloned());
            self.recompute_subtree(&root, parent_style.as_ref(), &mut stats);
        }

        // A node may be removed and reinserted before the rendering checkpoint. Its identifier
        // remains in the removal log, but its newly recomputed style must remain available. Sweep
        // connectivity once for the entire root set rather than once per component.
        let mut fullscreen = Vec::new();
        let connected_nodes = Node::shadow_including_descendants(document)
            .map(|node| {
                if node.is_fullscreen() {
                    fullscreen.push(node.clone());
                }
                node_id(&node)
            })
            .collect::<HashSet<_>>();
        stats.removed_styles += removed_nodes
            .iter()
            .filter(|node| !connected_nodes.contains(node))
            .filter(|node| self.styles.remove(node).is_some())
            .count();
        let removed_origins = removed_nodes
            .iter()
            .filter(|node| !connected_nodes.contains(node))
            .copied()
            .collect::<HashSet<_>>();
        self.remove_generated_pseudos(&removed_origins);
        self.refresh_deferred_fullscreen_roots(&fullscreen, &mut stats);
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
            self.styles.insert(node_id(&node), style.clone());
            self.sync_generated_pseudos(&node, &style);
            if !self.defer_nonrendered_descendants || style.display != Display::None {
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
            match self.styles.get(&node_id(&node)) {
                Some(previous) if previous != &style => {
                    stats.changed_styles += 1;
                    stats.layout_changed |= !previous.layout_equivalent(&style);
                }
                None => {
                    stats.changed_styles += 1;
                    stats.layout_changed = true;
                }
                _ => {}
            }
            self.styles.insert(node_id(&node), style.clone());
            // Attribute-backed generated content and pseudo-only declarations can change box
            // geometry without changing the originating element's computed style.
            let pseudo_started = std::time::Instant::now();
            if self.sync_generated_pseudos(&node, &style) {
                stats.layout_changed = true;
            }
            stats.pseudo_style_time += pseudo_started.elapsed();
            if !self.defer_nonrendered_descendants || style.display != Display::None {
                pending.extend(Node::composed_children(&node).into_iter().rev());
            } else {
                stats.removed_styles += self.forget_deferred_styles(&node, false);
            }
        }
    }
}

fn normalized_roots(document: &NodeRef, requested: &[NodeRef]) -> Vec<NodeRef> {
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
    if roots.is_empty() {
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
