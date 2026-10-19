//! What a selector observes about sibling positions, for bounding child-list restyles.
use super::super::subject_keys::SubjectKey;
use crate::engine::css::selector_model::{Combinator, CompoundSelector, Selector};

/// Where a selector observes sibling positions.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Position {
    None,
    /// Only in the sibling chain of the matched element itself.
    Siblings,
    /// Also in an ancestor's sibling chain.
    Ancestors,
}

/// Keys of the compounds that must match the changed parent or its ancestors for the rule to
/// match differently below it: compounds left of the first one whose element can be a child of
/// the parent, joined to it through a descendant or child combinator. Empty when unknown.
pub(super) fn outer_keys(selector: &Selector) -> Vec<SubjectKey> {
    let chain = sibling_chain_start(selector);
    let Some(first) = (0..chain).find(|&index| {
        compound_position(&selector.compounds[index]) != Position::None
            || (index > 0 && sibling(&selector.combinators[index - 1]))
    }) else {
        return Vec::new();
    };
    (0..first)
        .filter(|&index| {
            selector.combinators[index..first]
                .iter()
                .any(|combinator| matches!(combinator, Combinator::Descendant | Combinator::Child))
        })
        .map(|index| SubjectKey::of(&selector.compounds[index]))
        .filter(|key| !key.matches_any())
        .collect()
}

/// Whether an `:empty` condition is followed by a sibling combinator, so the parent's own
/// `:empty` state can reach its siblings and their subtrees.
pub(super) fn empty_reaches_siblings(selector: &Selector) -> bool {
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
pub(super) fn positional_ancestor_keys(selector: &Selector) -> Option<Vec<SubjectKey>> {
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

pub(super) fn position(selector: &Selector) -> Position {
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
