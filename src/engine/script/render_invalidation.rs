//! Per-realm accumulation of the minimum conservative rendering invalidation.

use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct PendingInvalidation {
    root: Option<NodeRef>,
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
            MutationKind::Attribute(_) | MutationKind::CharacterData => {
                target.parent().unwrap_or_else(|| target.clone())
            }
            MutationKind::ChildList => target.clone(),
            MutationKind::Stylesheet | MutationKind::Viewport => document.clone(),
        };
        self.extend(&root);
    }

    pub(super) fn extend(&mut self, target: &NodeRef) {
        self.root = Some(
            self.root
                .as_ref()
                .map_or_else(|| target.clone(), |root| common_ancestor(root, target)),
        );
    }

    pub(super) fn record_removed_subtree(&mut self, root: &NodeRef) {
        self.removed_nodes
            .extend(Node::descendants(root).map(|node| node.id()));
    }

    pub(super) fn snapshot(&self, mutation_count: usize) -> RenderInvalidation {
        RenderInvalidation {
            root: self.root.as_ref().map(|root| root.id()),
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
    std::iter::successors(Some(node.clone()), |current| current.parent())
        .any(|current| current.tag_name() == Some("style"))
}

fn common_ancestor(left: &NodeRef, right: &NodeRef) -> NodeRef {
    let left_ancestors = std::iter::successors(Some(left.clone()), |node| node.parent())
        .map(|node| (node.id(), node))
        .collect::<HashMap<_, _>>();
    std::iter::successors(Some(right.clone()), |node| node.parent())
        .find_map(|node| left_ancestors.get(&node.id()).cloned())
        .unwrap_or_else(|| left.clone())
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
        assert_eq!(invalidation.root, Some(main.id()));
        assert_eq!(invalidation.mutation_count, 2);
    }
}
