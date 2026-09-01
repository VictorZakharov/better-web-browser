//! Per-realm accumulation of the minimum conservative rendering invalidation.

use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct PendingInvalidation {
    roots: Vec<NodeRef>,
    impact: InvalidationImpact,
    rebuild_style_rules: bool,
    removed_nodes: BTreeSet<NodeId>,
}

impl PendingInvalidation {
    pub(super) fn record(
        &mut self,
        document: &NodeRef,
        target: Option<&NodeRef>,
        mut kind: MutationKind<'_>,
    ) {
        if target.is_some_and(is_in_style_element)
            && matches!(kind, MutationKind::CharacterData | MutationKind::ChildList)
        {
            kind = MutationKind::Stylesheet;
        }
        self.impact = self.impact.union(kind.impact());
        self.rebuild_style_rules |= matches!(kind, MutationKind::Stylesheet);
        let Some(target) = target else {
            return;
        };
        let root = match kind {
            MutationKind::Attribute(_) | MutationKind::CharacterData => target
                .shadow_including_parent()
                .unwrap_or_else(|| target.clone()),
            MutationKind::ChildList => target.clone(),
            MutationKind::Stylesheet | MutationKind::Viewport => document.clone(),
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
        self.removed_nodes
            .extend(Node::shadow_including_descendants(root).map(|node| node.id()));
    }

    pub(super) fn snapshot(&self, mutation_count: usize) -> RenderInvalidation {
        RenderInvalidation {
            roots: self.roots.iter().map(|root| root.id()).collect(),
            impact: self.impact,
            mutation_count,
            rebuild_style_rules: self.rebuild_style_rules,
            removed_nodes: self.removed_nodes.iter().copied().collect(),
        }
    }

    pub(super) fn take(&mut self, mutation_count: usize) -> RenderInvalidation {
        let result = self.snapshot(mutation_count);
        *self = Self::default();
        result
    }
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
