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
//   match the subject key are searched below every child.
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
use crate::engine::css::selector_model::{Combinator, CompoundSelector};
use std::collections::HashSet;

#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
pub(super) struct ChildListInvalidation {
    /// Some rule's positional conditions are confined to its rightmost sibling chain.
    children: bool,
    /// Subject keys of rules with a positional condition left of their rightmost sibling chain
    /// in a compound without a key.
    descendant_subjects: Vec<SubjectKey>,
    /// Rules whose positional conditions left of their rightmost sibling chain are all in keyed
    /// compounds, as (key of such a compound, subject key or `None` for any element).
    anchored: Vec<(SubjectKey, Option<SubjectKey>)>,
    /// `:empty` appears outside a subject compound, so the parent's own `:empty` state can
    /// reach its siblings (`:empty + p`) and its siblings' subtrees.
    empty_siblings: bool,
    /// A rule with an unkeyed subject has an unkeyed positional compound, or the rules are
    /// scoped or cross-tree.
    unbounded: bool,
}

/// Where a selector observes sibling positions.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Position {
    None,
    /// Only in the sibling chain of the matched element itself.
    Siblings,
    /// Also in an ancestor's sibling chain.
    Ancestors,
}

impl ChildListInvalidation {
    pub(super) fn for_rules(rules: &[Rule], cross_tree_or_scoped: bool) -> Self {
        let mut index = Self {
            unbounded: cross_tree_or_scoped,
            ..Self::default()
        };
        for rule in rules {
            index.empty_siblings |= empty_reaches_siblings(&rule.selector);
            match position(&rule.selector) {
                Position::None => {}
                Position::Siblings => index.children = true,
                Position::Ancestors => {
                    let subject = SubjectKey::of(rule.selector.compounds.last().expect("compound"));
                    let subject = (!subject.matches_any()).then_some(subject);
                    match (positional_ancestor_keys(&rule.selector), subject) {
                        (Some(anchors), subject) => {
                            for anchor in anchors {
                                let entry = (anchor, subject.clone());
                                if !index.anchored.contains(&entry) {
                                    index.anchored.push(entry);
                                }
                            }
                        }
                        (None, Some(subject)) => {
                            if !index.descendant_subjects.contains(&subject) {
                                index.descendant_subjects.push(subject);
                            }
                        }
                        (None, None) => index.unbounded = true,
                    }
                }
            }
        }
        index
    }
}

/// Whether an `:empty` condition is followed by a sibling combinator, so the parent's own
/// `:empty` state can reach its siblings and their subtrees.
fn empty_reaches_siblings(selector: &Selector) -> bool {
    selector
        .compounds
        .iter()
        .enumerate()
        .any(|(index, compound)| {
            ((compound.requires_empty || nested(compound).any(contains_empty))
                && selector.combinators[index..].iter().any(sibling))
                || nested(compound).any(empty_reaches_siblings)
        })
}

fn contains_empty(selector: &Selector) -> bool {
    selector
        .compounds
        .iter()
        .any(|compound| compound.requires_empty || nested(compound).any(contains_empty))
}

/// The selector arguments of `compound`'s functional pseudo-classes and `of S` filters.
fn nested(compound: &CompoundSelector) -> impl Iterator<Item = &Selector> {
    compound
        .functional
        .iter()
        .flat_map(|functional| &functional.selectors)
        .chain(compound.nth.iter().flat_map(|nth| &nth.filter))
}

fn sibling(combinator: &Combinator) -> bool {
    matches!(
        combinator,
        Combinator::AdjacentSibling | Combinator::GeneralSibling
    )
}

/// The index of the first compound of the subject's sibling chain; compounds before it match
/// ancestors of the subject or their siblings.
fn sibling_chain_start(selector: &Selector) -> usize {
    selector
        .combinators
        .iter()
        .rposition(|combinator| matches!(combinator, Combinator::Descendant | Combinator::Child))
        .map_or(0, |index| index + 1)
}

/// Keys of the compounds left of the subject's sibling chain that observe sibling positions,
/// or `None` when one of them can match any element or observes positions further inside.
fn positional_ancestor_keys(selector: &Selector) -> Option<Vec<SubjectKey>> {
    let chain = sibling_chain_start(selector);
    let mut keys = Vec::new();
    for (index, compound) in selector.compounds[..chain].iter().enumerate() {
        let after_sibling = index > 0 && sibling(&selector.combinators[index - 1]);
        match compound_position(compound) {
            Position::Ancestors => return None,
            Position::None if !after_sibling => continue,
            Position::None | Position::Siblings => {}
        }
        let key = SubjectKey::of(compound);
        if key.matches_any() {
            return None;
        }
        keys.push(key);
    }
    Some(keys)
}

fn position(selector: &Selector) -> Position {
    let chain = sibling_chain_start(selector);
    let mut result = if selector.combinators[..chain.saturating_sub(1)]
        .iter()
        .any(sibling)
    {
        Position::Ancestors
    } else if selector.combinators[chain..].iter().any(sibling) {
        Position::Siblings
    } else {
        Position::None
    };
    for (index, compound) in selector.compounds.iter().enumerate() {
        let compound = compound_position(compound);
        result = result.max(if index < chain && compound != Position::None {
            Position::Ancestors
        } else {
            compound
        });
    }
    result
}

fn compound_position(compound: &CompoundSelector) -> Position {
    let own = compound.requires_first_child
        || compound.requires_first_of_type
        || compound.requires_last_child
        || compound.requires_last_of_type
        || compound.requires_only_child
        || compound.requires_only_of_type
        || compound.requires_empty
        || !compound.nth.is_empty();
    let nested = nested(compound)
        .map(position)
        .max()
        .unwrap_or(Position::None);
    nested.max(if own {
        Position::Siblings
    } else {
        Position::None
    })
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
        let unanchored = KeyIndex::new(index.descendant_subjects.iter().collect());
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
            subtrees.retain(|root| root.id() != id);
            elements.push(parent.clone());
            for child in parent.children.borrow().iter() {
                if !self.styles.contains_key(&child.id()) || removed.contains(&child.id()) {
                    subtrees.push(child.clone());
                    continue;
                }
                let mut subjects = HashSet::new();
                let mut whole = false;
                for (anchor, subject) in &index.anchored {
                    if anchor.may_match(child) {
                        match subject {
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
                    if !index.descendant_subjects.is_empty() {
                        push_matches(child, &unanchored, elements);
                    }
                } else {
                    subjects.extend(&index.descendant_subjects);
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
