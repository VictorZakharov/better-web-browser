//! Bounded restyle scopes for child-list changes.
//
// Inserting or removing children of an element changes which elements exist below it, its own
// `:empty` state and the sibling positions of its other children. Restyling the parent's whole
// subtree covers all of that, but for <body> or <html> it is the whole document. Instead the
// parent is restyled as an element root, new children (and children whose subtrees were removed
// or moved in the same checkpoint) are restyled with their subtrees, and the rest of the subtree
// only where a rule can observe the new sibling positions:
//
// - a rule whose positional conditions are all in its rightmost sibling chain (`li:first-child`,
//   `.a + .b`, `.list > :nth-child(odd)`) matches differently only for the parent's children, so
//   they are restyled as element roots;
// - a rule with a positional condition further left (`tr:first-child td`, `.a + .b .c`) can match
//   differently only below a child of the parent, since only those changed position. When each
//   such condition is in a compound with a tag, ID or class, only the children that may match
//   one are searched: their descendants that may match the subject key are restyled as element
//   roots, or their whole subtrees when the subject has no key. Otherwise the elements that may
//   match the subject key are searched below every child, and when the subject has no key either
//   the parent's subtree is restyled. Compounds left of the condition that are joined to it by a
//   descendant or child combinator match the parent or its ancestors, so a rule whose keys there
//   match none of them (`table.x :first-child td` below <body>) is skipped.
//
// Element roots refresh descendants only when their own style changes, which carries inheritance.
// `:has()` anchors are found from the changed elements (`has_anchors`); when a `:has()` argument
// searches siblings, the parent's children are passed along as changed elements as well. Scoped or
// cross-tree rules, shadow hosts, slots and nodes inside shadow trees keep the subtree scope. When
// `:empty` appears outside a subject compound, the parent's parent subtree is restyled instead,
// since the parent's `:empty` state reaches its siblings.
// https://drafts.csswg.org/selectors-4/#structural-pseudos
use super::subject_keys::{KeyIndex, SubjectKey};
use super::*;
use selectors::{Position, empty_reaches_siblings, outer_keys, position, positional_ancestor_keys};
use std::collections::HashSet;

mod selectors;
#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
pub(super) struct ChildListInvalidation {
    /// Some rule's positional conditions are confined to its rightmost sibling chain.
    children: bool,
    /// Rules with a positional condition left of their rightmost sibling chain.
    ancestor_rules: Vec<AncestorRule>,
    /// `:empty` appears outside a subject compound, so the parent's own `:empty` state can
    /// reach its siblings (`:empty + p`) and its siblings' subtrees.
    empty_siblings: bool,
    /// The rules are scoped or cross-tree.
    unbounded: bool,
}

/// What a rule with a positional condition left of its subject's sibling chain can reach.
#[derive(Debug, PartialEq)]
struct AncestorRule {
    /// Keys that the changed parent or its ancestors must match (`outer_keys`).
    outer: Vec<SubjectKey>,
    /// Keys of the compounds a child of the parent must match to change what the rule matches
    /// below it, or `None` when such a compound has no key.
    anchors: Option<Vec<SubjectKey>>,
    /// The subject key, or `None` for any element.
    subject: Option<SubjectKey>,
}

impl ChildListInvalidation {
    pub(super) fn for_rules(rules: &[Rule], cross_tree_or_scoped: bool) -> Self {
        let mut index = Self {
            unbounded: cross_tree_or_scoped,
            ..Self::default()
        };
        for rule in rules {
            let selector = &rule.selector;
            index.empty_siblings |= empty_reaches_siblings(selector);
            match position(selector) {
                Position::None => {}
                Position::Siblings => index.children = true,
                Position::Ancestors => {
                    let subject = SubjectKey::of(selector.compounds.last().expect("compound"));
                    let entry = AncestorRule {
                        outer: outer_keys(selector),
                        anchors: positional_ancestor_keys(selector),
                        subject: (!subject.matches_any()).then_some(subject),
                    };
                    if !index.ancestor_rules.contains(&entry) {
                        index.ancestor_rules.push(entry);
                    }
                }
            }
        }
        index
    }
}

impl StyleSet {
    /// Replaces child-list parents in `subtrees` with bounded roots: the parent and the elements
    /// whose matching can observe the change as element roots, and changed children as subtrees.
    pub(crate) fn resolve_child_list_changes(
        &self,
        dom: &Dom,
        parents: &[NodeId],
        removed_nodes: &[NodeId],
        subtrees: &mut Vec<NodeRef>,
        elements: &mut Vec<NodeRef>,
    ) {
        let index = &self.compiled.child_list_invalidation;
        if index.empty_siblings {
            for &id in parents {
                if let Some(position) = subtrees.iter().position(|root| root.id() == id)
                    && let Some(grandparent) = subtrees[position].parent()
                {
                    subtrees[position] = grandparent;
                }
            }
            return;
        }
        if index.unbounded || parents.is_empty() {
            return;
        }
        let children =
            index.children || self.compiled.nonlocal_dependencies.sibling_has_relations();
        let removed = removed_nodes.iter().copied().collect::<HashSet<_>>();
        for &id in parents {
            // Child-list roots are also subtree roots; finding them there avoids a document walk.
            let Some(parent) = subtrees.iter().find(|root| root.id() == id).cloned() else {
                continue;
            };
            if Node::tree_root(&parent).id() != dom.document.id()
                || parent.shadow_root().is_some()
                || parent.tag_name() == Some("slot")
            {
                continue;
            }
            let ancestry = std::iter::successors(Some(parent.clone()), |node| node.parent())
                .collect::<Vec<_>>();
            let rules = index
                .ancestor_rules
                .iter()
                .filter(|rule| {
                    rule.outer
                        .iter()
                        .all(|key| ancestry.iter().any(|node| key.may_match(node)))
                })
                .collect::<Vec<_>>();
            if rules
                .iter()
                .any(|rule| rule.anchors.is_none() && rule.subject.is_none())
            {
                // Any element below any child may match anew; keep the parent's subtree.
                continue;
            }
            let unanchored_subjects = rules
                .iter()
                .filter(|rule| rule.anchors.is_none())
                .filter_map(|rule| rule.subject.as_ref())
                .collect::<HashSet<_>>();
            let unanchored = KeyIndex::new(unanchored_subjects.clone());
            subtrees.retain(|root| root.id() != id);
            elements.push(parent.clone());
            for child in parent.children.borrow().iter() {
                if !self.styles.contains_key(&child.id()) || removed.contains(&child.id()) {
                    subtrees.push(child.clone());
                    continue;
                }
                let mut subjects = HashSet::new();
                let mut whole = false;
                for rule in &rules {
                    let Some(anchors) = &rule.anchors else {
                        continue;
                    };
                    if anchors.iter().any(|anchor| anchor.may_match(child)) {
                        match &rule.subject {
                            Some(subject) => {
                                subjects.insert(subject);
                            }
                            None => whole = true,
                        }
                    }
                }
                if whole {
                    subtrees.push(child.clone());
                    continue;
                }
                if children && child.element().is_some() {
                    elements.push(child.clone());
                }
                if subjects.is_empty() {
                    if !unanchored_subjects.is_empty() {
                        push_matches(child, &unanchored, elements);
                    }
                } else {
                    subjects.extend(&unanchored_subjects);
                    push_matches(child, &KeyIndex::new(subjects), elements);
                }
            }
        }
    }
}

/// Pushes `root` and its descendants that may match one of `keys`.
fn push_matches(root: &NodeRef, keys: &KeyIndex, elements: &mut Vec<NodeRef>) {
    let mut pending = vec![root.clone()];
    while let Some(node) = pending.pop() {
        if keys.may_match(&node) {
            elements.push(node.clone());
        }
        pending.extend(node.children.borrow().iter().cloned());
    }
}
