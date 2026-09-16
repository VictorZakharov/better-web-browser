//! Per-realm accumulation of the minimum conservative rendering invalidation.

use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct PendingInvalidation {
    roots: Vec<NodeRef>,
    impact: InvalidationImpact,
    rebuild_style_rules: bool,
    removed_nodes: BTreeSet<NodeId>,
    removed_global_dependency: bool,
}

impl PendingInvalidation {
    pub(super) fn record(
        &mut self,
        document: &NodeRef,
        target: Option<&NodeRef>,
        kind: MutationKind<'_>,
    ) {
        let rebuild_rules = rebuilds_style_rules(target, kind);
        self.impact = self.impact.union(kind.impact());
        if rebuild_rules {
            self.impact = self.impact.union(MutationKind::Stylesheet.impact());
        }
        self.rebuild_style_rules |= rebuild_rules;
        let Some(target) = target else {
            return;
        };
        let root = match kind {
            MutationKind::Attribute(_)
            | MutationKind::CharacterData
            | MutationKind::State
            | MutationKind::PointerDesignation => target
                .shadow_including_parent()
                .unwrap_or_else(|| target.clone()),
            // Keep the DOM mutation's scope even when rules may need rebuilding. The style
            // consumer widens to the document only if the effective rule inputs changed.
            MutationKind::ChildList | MutationKind::Stylesheet => target.clone(),
            MutationKind::Viewport => document.clone(),
        };
        self.extend(document, &root);
    }

    pub(super) fn extend(&mut self, document: &NodeRef, target: &NodeRef) {
        if self.roots.iter().any(|root| is_descendant_of(target, root)) {
            return;
        }
        self.roots.retain(|root| !is_descendant_of(root, target));
        self.roots.push(target.clone());
        if self.roots.len() > crate::engine::invalidation::MAX_INVALIDATION_ROOTS {
            self.roots.clear();
            self.roots.push(document.clone());
        }
    }

    pub(super) fn record_removed_subtree(&mut self, root: &NodeRef) {
        for node in Node::shadow_including_descendants(root) {
            self.removed_nodes.insert(node.id());
            // SVG definitions, slot redistribution, base URLs, and top-layer content affect boxes outside
            // an otherwise hidden subtree. Retain this evidence before losing connectivity.
            self.removed_global_dependency |= node.is_fullscreen()
                || matches!(node.tag_name(), Some("base" | "slot"))
                || node
                    .namespace_uri()
                    .is_some_and(|ns| ns != "http://www.w3.org/1999/xhtml");
        }
    }

    pub(super) fn acknowledge_published_geometry(&mut self) {
        // The renderer has already laid out all content changes through this checkpoint.
        // Its geometry does not refresh the script snapshot's independent style cache:
        // retain dirty roots, removed styles, and rule-rebuild obligations for that cache.
        self.impact = self.impact.without_intrinsic_size();
    }

    pub(super) fn snapshot(&self, mutation_count: usize) -> RenderInvalidation {
        RenderInvalidation {
            roots: self.roots.iter().map(|root| root.id()).collect(),
            impact: self.impact,
            mutation_count,
            rebuild_style_rules: self.rebuild_style_rules,
            removed_nodes: self.removed_nodes.iter().copied().collect(),
            removals_are_local: !self.removed_nodes.is_empty() && !self.removed_global_dependency,
        }
    }

    pub(super) fn take(&mut self, mutation_count: usize) -> RenderInvalidation {
        let result = self.snapshot(mutation_count);
        *self = Self::default();
        result
    }
}

pub(super) fn rebuilds_style_rules(target: Option<&NodeRef>, kind: MutationKind<'_>) -> bool {
    matches!(kind, MutationKind::Stylesheet)
        || (matches!(
            kind,
            MutationKind::Attribute("href" | "rel" | "media" | "type" | "disabled" | "title")
        ) && target.is_some_and(|node| matches!(node.tag_name(), Some("style" | "link"))))
        || (matches!(kind, MutationKind::CharacterData | MutationKind::ChildList)
            && target.is_some_and(is_in_style_element))
}

fn is_in_style_element(node: &NodeRef) -> bool {
    std::iter::successors(Some(node.clone()), |current| {
        current.shadow_including_parent()
    })
    .any(|current| current.tag_name() == Some("style"))
}

fn is_descendant_of(node: &NodeRef, ancestor: &NodeRef) -> bool {
    std::iter::successors(Some(node.clone()), |node| node.shadow_including_parent())
        .any(|node| node.id() == ancestor.id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom;

    #[test]
    fn published_geometry_preserves_pending_style_and_removal_obligations() {
        let dom = dom::parse("<style>p{color:red}</style><main><p>text</p></main>");
        let sheet = dom.elements_named("style").next().unwrap();
        let paragraph = dom.elements_named("p").next().unwrap();
        let mut pending = PendingInvalidation::default();
        pending.record(&dom.document, Some(&sheet), MutationKind::Stylesheet);
        pending.record_removed_subtree(&paragraph);
        let before = pending.snapshot(2);

        pending.acknowledge_published_geometry();
        let after = pending.snapshot(2);
        assert!(before.impact.affects_intrinsic_size());
        assert!(!after.impact.affects_intrinsic_size());
        assert!(after.impact.affects_style() && after.impact.affects_layout());
        assert!(after.impact.affects_paint() && after.rebuild_style_rules);
        assert_eq!(after.roots, before.roots);
        assert_eq!(after.removed_nodes, before.removed_nodes);
        assert_eq!(after.mutation_count, before.mutation_count);

        pending.record(&dom.document, Some(&paragraph), MutationKind::CharacterData);
        assert!(pending.snapshot(3).impact.affects_intrinsic_size());
    }

    #[test]
    fn coalesces_sibling_mutations_at_their_parent() {
        let document = dom::parse("<main><p id=a></p><p id=b></p></main>");
        let main = document.elements_named("main").next().unwrap();
        let children = main.children.borrow().clone();
        let mut pending = PendingInvalidation::default();
        pending.record(
            &document.document,
            Some(&children[0]),
            MutationKind::Attribute("class"),
        );
        pending.record(
            &document.document,
            Some(&children[1]),
            MutationKind::Attribute("hidden"),
        );

        let invalidation = pending.snapshot(2);
        assert_eq!(invalidation.roots, vec![main.id()]);
        assert_eq!(invalidation.mutation_count, 2);
    }

    #[test]
    fn removed_subtrees_retain_nonlocal_dependencies_until_consumed() {
        for (source, local) in [
            ("<section><script></script></section>", true),
            ("<section><svg><defs/></svg></section>", false),
            ("<section><base href='/other/'></section>", false),
            ("<section><slot></slot></section>", false),
        ] {
            let dom = dom::parse(source);
            let root = dom.elements_named("section").next().unwrap();
            let mut pending = PendingInvalidation::default();
            pending.record_removed_subtree(&root);
            pending.acknowledge_published_geometry();
            assert_eq!(pending.take(1).removals_are_local, local, "{source}");
            assert!(!pending.snapshot(0).removals_are_local);
        }
        let dom = dom::parse("<main><p>fullscreen</p></main>");
        let root = dom.elements_named("main").next().unwrap();
        dom.elements_named("p").next().unwrap().set_fullscreen(true);
        let mut pending = PendingInvalidation::default();
        pending.record_removed_subtree(&root);
        assert!(!pending.snapshot(1).removals_are_local);
    }

    #[test]
    fn retains_disjoint_component_roots() {
        let document = dom::parse("<main><section><p></p></section><aside><p></p></aside></main>");
        let section = document.elements_named("section").next().unwrap();
        let aside = document.elements_named("aside").next().unwrap();
        let left = section.children.borrow()[0].clone();
        let right = aside.children.borrow()[0].clone();
        let mut pending = PendingInvalidation::default();
        pending.record(
            &document.document,
            Some(&left),
            MutationKind::Attribute("class"),
        );
        pending.record(
            &document.document,
            Some(&right),
            MutationKind::Attribute("class"),
        );

        let mut roots = pending.snapshot(2).roots;
        roots.sort_unstable();
        let mut expected = vec![section.id(), aside.id()];
        expected.sort_unstable();
        assert_eq!(roots, expected);
    }
}
