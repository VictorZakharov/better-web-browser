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
mod unchanged;

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
            text_color_only: true,
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
        stats.text_color_only &=
            !stats.layout_changed && stats.removed_styles == 0 && !removed_pseudos;
        stats.total_styles = self.styles.len();
        stats
    }

    pub(super) fn compute_subtree(&mut self, node: &NodeRef, parent: Option<&ComputedStyle>) {
        let root = node_id(node);
        let mut cache = sharing::SharingCache::default();
        let mut pending = vec![(node.clone(), cache.new_group())];
        while let Some((node, parent_group)) = pending.pop() {
            let (style, group, source) = if node_id(&node) == root {
                self.compute_style_shared(&mut cache, &node, parent, parent_group)
            } else {
                let parent = Node::composed_parent(&node)
                    .expect("connected composed-tree child has a parent");
                let parent_style = self
                    .styles
                    .get(&node_id(&parent))
                    .expect("parent style is computed before its children");
                if node.element().is_none() {
                    (
                        self.shared_text_style(&mut cache, &node, parent_style),
                        parent_group,
                        None,
                    )
                } else {
                    self.compute_style_shared(&mut cache, &node, Some(parent_style), parent_group)
                }
            };
            self.remember_has_results(&node, node_id(&node) == root);
            let displayed = style.display != Display::None;
            self.styles.insert(node_id(&node), style);
            self.style_log.record(node_id(&node));
            self.layout_generations.mark(&node);
            self.sync_generated_pseudos_shared(&node, source.as_ref().map(|s| (&mut cache, s)));
            if !self.defer_nonrendered_descendants || displayed {
                let children = Node::composed_children(&node).into_iter().rev();
                pending.extend(children.map(|child| (child, group)));
            }
        }
        #[cfg(test)]
        sharing::LAST_STATS.with(|stats| stats.set(cache.stats));
    }

    pub(super) fn recompute_subtree(
        &mut self,
        node: &NodeRef,
        parent: Option<&ComputedStyle>,
        stats: &mut StyleRefreshStats,
    ) {
        self.recompute_tree(node, parent, stats, |_, _| true);
    }

    /// Recomputes `node` and the descendants below each node for which `descend(node,
    /// changed)` holds; `changed` says whether that node's computed style changed.
    pub(super) fn recompute_tree(
        &mut self,
        node: &NodeRef,
        parent: Option<&ComputedStyle>,
        stats: &mut StyleRefreshStats,
        mut descend: impl FnMut(&NodeRef, bool) -> bool,
    ) {
        let root = node_id(node);
        // Every node is restyled after its parent in this traversal, so share groups hold as
        // in `compute_subtree`: equal siblings and cousins reuse one computed style.
        let mut cache = sharing::SharingCache::default();
        let mut unchanged = unchanged::UnchangedStyles::default();
        let mut pending = vec![(node.clone(), cache.new_group())];
        while let Some((node, parent_group)) = pending.pop() {
            let style_started = std::time::Instant::now();
            let (style, group, source) = if node_id(&node) == root {
                self.compute_style_shared(&mut cache, &node, parent, parent_group)
            } else {
                let parent = Node::composed_parent(&node)
                    .expect("connected composed-tree child has a parent");
                let parent_style = self
                    .styles
                    .get(&node_id(&parent))
                    .expect("parent style is recomputed before its children");
                if node.element().is_none() {
                    (
                        self.shared_text_style(&mut cache, &node, parent_style),
                        parent_group,
                        None,
                    )
                } else {
                    self.compute_style_shared(&mut cache, &node, Some(parent_style), parent_group)
                }
            };
            stats.element_style_time += style_started.elapsed();
            stats.invalidated_nodes += 1;
            stats.recomputed_styles += 1;
            let mut changed = true;
            match self.styles.get(&node_id(&node)) {
                Some(previous) if !unchanged.equal(previous, &style) => {
                    stats.first_changed_node.get_or_insert(node.id());
                    stats.changed_styles += 1;
                    // Each comparison walks much of a ~3 KB style. A restyle after a stylesheet
                    // arrives changes most styles, so skip the ones whose flag is already settled.
                    if !stats.layout_changed {
                        stats.layout_changed = !previous.layout_equivalent(&style);
                    }
                    if !stats.non_deferable_paint_changes {
                        stats.non_deferable_paint_changes =
                            !previous.deferred_paint_equivalent(&style);
                    }
                    if stats.opacity_only {
                        stats.opacity_only = previous.retained_opacity_equivalent(&style);
                    }
                    if stats.text_color_only {
                        stats.text_color_only = previous.text_color_equivalent(&style)
                            && !paints_color_outside_text(&node);
                    }
                }
                None => {
                    stats.first_changed_node.get_or_insert(node.id());
                    stats.changed_styles += 1;
                    stats.layout_changed = true;
                    stats.non_deferable_paint_changes = true;
                    stats.opacity_only = false;
                    stats.text_color_only = false;
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
            let pseudo_changes =
                self.sync_generated_pseudos_shared(&node, source.as_ref().map(|s| (&mut cache, s)));
            if changed || pseudo_changes.layout_changed || pseudo_changes.paint_changed {
                self.layout_generations.mark(&node);
            }
            stats.layout_changed |= pseudo_changes.layout_changed;
            stats.non_deferable_paint_changes |= pseudo_changes.paint_changed;
            stats.opacity_only &= !pseudo_changes.paint_changed;
            stats.text_color_only &= !pseudo_changes.paint_changed;
            if pseudo_changes.paint_changed {
                stats.first_changed_node.get_or_insert(node.id());
            }
            stats.pseudo_style_time += pseudo_started.elapsed();
            if !self.defer_nonrendered_descendants || displayed {
                if descend(&node, changed) {
                    let children = Node::composed_children(&node).into_iter().rev();
                    pending.extend(children.map(|child| (child, group)));
                }
            } else {
                stats.removed_styles += self.forget_deferred_styles(&node, false);
            }
        }
        #[cfg(test)]
        sharing::LAST_STATS.with(|stats| stats.set(cache.stats));
    }
}

impl StyleSet {
    /// Records the `:has()` results that `node`'s dependents are being styled with. A traversal
    /// root's later siblings are outside the traversal, so a root whose results later siblings
    /// may consult is forgotten instead, keeping the next mutation conservative.
    fn remember_has_results(&mut self, node: &NodeRef, traversal_root: bool) {
        // Only elements are anchors; nothing is ever remembered for other nodes.
        if node.element().is_none() {
            return;
        }
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
    let candidates = requested
        .iter()
        .map(|requested| {
            let requested = requested.shadow_host().unwrap_or_else(|| requested.clone());
            if is_descendant_of(&requested, document) {
                requested
            } else {
                document.clone()
            }
        })
        .collect::<Vec<_>>();
    // Keep each root whose ancestors were not requested too, once, in request order. A pass
    // over every pair of roots is quadratic for the thousands a streamed chunk can bring.
    let ids = candidates
        .iter()
        .map(|root| root.id())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut roots = candidates
        .into_iter()
        .filter(|root| {
            std::iter::successors(root.shadow_including_parent(), |node| {
                node.shadow_including_parent()
            })
            .all(|ancestor| !ids.contains(&ancestor.id()))
                && seen.insert(root.id())
        })
        .collect::<Vec<_>>();
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

/// Elements that paint their own `color` outside text runs: replaced content (SVG
/// `currentColor`, image tints), form controls and their accent colors, and anything in the SVG
/// namespace.
fn paints_color_outside_text(node: &NodeRef) -> bool {
    node.namespace_uri()
        .is_some_and(|namespace| namespace != "http://www.w3.org/1999/xhtml")
        || matches!(
            node.tag_name(),
            Some(
                "img"
                    | "image"
                    | "video"
                    | "canvas"
                    | "iframe"
                    | "embed"
                    | "object"
                    | "svg"
                    | "input"
                    | "textarea"
                    | "select"
                    | "button"
                    | "option"
                    | "optgroup"
                    | "meter"
                    | "progress"
                    | "math"
            )
        )
}
