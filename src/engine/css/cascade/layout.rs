//! Layout-only style snapshots may defer subtrees which generate no boxes.
use super::*;

#[cfg(test)]
mod regressions;

impl StyleSet {
    pub(super) fn compute_independent_fullscreen_roots(&mut self, document: &NodeRef) {
        let fullscreen = Node::shadow_including_descendants(document)
            .filter(|node| node.is_fullscreen())
            .collect::<Vec<_>>();
        self.refresh_deferred_fullscreen_roots(&fullscreen, &mut StyleRefreshStats::default());
    }

    pub(super) fn refresh_deferred_fullscreen_roots(
        &mut self,
        roots: &[NodeRef],
        stats: &mut StyleRefreshStats,
    ) {
        let mut current = HashSet::new();
        for root in roots {
            let uncomposed = uncomposed_ancestor_ids(root);
            if uncomposed.is_empty() && !self.has_deferred_ancestor(root) {
                continue;
            }
            // Top-layer suppression follows shadow-including ancestry, not assigned slots.
            // Hidden slots and unassigned light DOM can therefore omit an eligible subtree.
            // https://www.w3.org/TR/css-position-4/#top-layer-styling
            stats.removed_styles += self.forget_deferred_styles(root, true);
            // Normal composed-tree refresh cannot update unassigned inherited ancestors.
            // Evict only that missing chain before resolving the explicit fullscreen root.
            for id in &uncomposed {
                stats.removed_styles += usize::from(self.styles.remove(id).is_some());
            }
            self.remove_generated_pseudos(&uncomposed);
            let ancestors =
                std::iter::successors(Some(root.clone()), |node| node.shadow_including_parent())
                    .filter(|node| node.element().is_some())
                    .collect::<Vec<_>>();
            let suppressed = ancestors.iter().rev().any(|ancestor| {
                self.computed_style_for_node(ancestor)
                    .is_none_or(|style| style.display == Display::None)
            });
            if suppressed {
                continue;
            }
            // Hydration installs the root before the subtree comparison; independent
            // roots must conservatively relayout, including mutations after initial entry.
            stats.layout_changed = true;
            let parent = Node::composed_parent(root)
                .and_then(|parent| self.styles.get(&parent.id()).cloned());
            self.recompute_subtree(root, parent.as_ref(), stats);
            current.insert(root.id());
        }
        // Leaving an independently rendered top layer also changes geometry even if its
        // hidden slot and every normally traversed computed style remain unchanged.
        stats.layout_changed |= self.deferred_fullscreen_roots != current;
        self.deferred_fullscreen_roots = current;
    }

    pub(super) fn has_deferred_ancestor(&self, node: &NodeRef) -> bool {
        self.defer_nonrendered_descendants
            && std::iter::successors(Node::composed_parent(node), Node::composed_parent).any(
                |ancestor| {
                    self.styles
                        .get(&ancestor.id())
                        .is_some_and(|style| style.display == Display::None)
                },
            )
    }

    pub(super) fn forget_deferred_styles(&mut self, node: &NodeRef, include_root: bool) -> usize {
        let ids = Node::composed_descendants(node)
            .skip(usize::from(!include_root))
            .map(|node| node.id())
            .collect::<HashSet<_>>();
        let count = self.styles.len();
        for id in &ids {
            self.styles.remove(id);
        }
        self.remove_generated_pseudos(&ids);
        count - self.styles.len()
    }

    pub(crate) fn from_sources_for_layout(
        dom: &Dom,
        base_url: &str,
        sheets: &[(String, String)],
        environment: MediaEnvironment,
    ) -> Self {
        let mut styles = Self::for_computed_style_for_media_environment(
            &dom.document,
            base_url,
            sheets,
            environment,
        );
        styles.defer_nonrendered_descendants = true;
        styles.compute_subtree(&dom.document, None);
        styles.compute_independent_fullscreen_roots(&dom.document);
        styles
    }
}

// composed_parent falls back to DOM ancestry for unassigned nodes. Verify the actual
// child links along explicit fullscreen ancestry instead of mistaking cached styles
// for participation in the composed traversal. Nodes below the last broken link need
// fresh inherited styles, even if a previous fullscreen pass already cached them.
fn uncomposed_ancestor_ids(node: &NodeRef) -> HashSet<NodeId> {
    let mut chain = Vec::new();
    let mut missing_chain_length = 0;
    let mut current = node.clone();
    while let Some(parent) = Node::composed_parent(&current) {
        chain.push(current.id());
        if !Node::composed_children(&parent)
            .iter()
            .any(|child| child.id() == current.id())
        {
            missing_chain_length = chain.len();
        }
        current = parent;
    }
    chain.truncate(missing_chain_length);
    chain.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_descendants_are_lazy_but_explicit_computed_style_and_reveal_are_correct() {
        let dom = dom::parse(
            "<style>.hidden{display:none;color:red} span{width:20px}</style><div class=hidden><span>text</span></div>",
        );
        let parent = dom.elements_named("div").next().unwrap();
        let child = dom.elements_named("span").next().unwrap();
        let mut styles = StyleSet::from_sources_for_layout(
            &dom,
            "",
            &[],
            MediaEnvironment::new(800.0, 600.0, 1.0, false),
        );
        assert!(!styles.styles.contains_key(&child.id()));
        assert_eq!(
            styles.computed_style_for_node(&child).unwrap().color,
            Color::rgb(255, 0, 0)
        );
        child.set_attr("style", "color:blue");
        let hidden_update =
            styles.refresh_subtrees(&dom.document, std::slice::from_ref(&child), &[]);
        assert_eq!(hidden_update.recomputed_styles, 0);
        assert!(!styles.styles.contains_key(&child.id()));
        assert_eq!(
            styles.computed_style_for_node(&child).unwrap().color,
            Color::rgb(0, 0, 255)
        );
        parent.set_attr("class", "visible");
        let stats = styles.refresh_subtrees(&dom.document, std::slice::from_ref(&parent), &[]);
        assert!(stats.layout_changed);
        let fresh = StyleSet::from_dom(&dom, &[], 800.0);
        assert_eq!(styles.get(&child), fresh.get(&child));
        parent.set_attr("class", "hidden");
        styles.refresh_subtrees(&dom.document, std::slice::from_ref(&parent), &[]);
        assert!(
            !styles.styles.contains_key(&child.id()),
            "hide must evict previously resolved descendants"
        );
    }
}
