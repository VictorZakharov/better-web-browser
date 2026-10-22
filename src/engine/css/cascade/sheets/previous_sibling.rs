//! Which previous last children an appended sibling can restyle beyond themselves.
//
// Appending an element makes the previous last element child (or last of its type) lose
// `:last-child` (`:last-of-type`, `:only-*`). A rule whose condition sits in its subject compound,
// or before a sibling combinator in the subject's sibling chain, can then match differently only
// for that element itself and for later siblings, which are the new nodes. A condition on a
// compound left of a descendant or child combinator (`li:last-child ul`) also reaches the
// element's descendants, but only for an element that may match that compound's keys; one whose
// element the keys cannot name reaches any element's descendants. Conditions inside `:has()` are
// left to the relational invalidation, which rechecks the anchors around every changed element,
// element roots included (`has_anchors`).
// https://drafts.csswg.org/selectors-4/#structural-pseudos
use super::subject_keys::SubjectKey;
use crate::engine::css::selector_model::{Combinator, CompoundSelector, Selector};
use crate::engine::dom::NodeRef;

#[cfg(test)]
mod tests;

/// The previous last children whose descendants an appended sibling can restyle.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct PreviousSubtrees {
    /// Some condition's element cannot be named by keys.
    any: bool,
    keys: Vec<SubjectKey>,
}

/// The element a compound is matched against, relative to the element losing its position.
#[derive(Clone)]
enum Reach {
    /// The rule's subject, or an earlier sibling in its sibling chain.
    Subject,
    /// An element with descendants that the rule can reach, matching this key.
    Ancestor(SubjectKey),
    Unknown,
}

impl PreviousSubtrees {
    pub(super) fn include(&mut self, selector: &Selector) {
        self.visit(selector, &Reach::Subject);
    }

    /// Whether appending after `previous` can restyle elements below it.
    pub(super) fn reaches(&self, previous: &NodeRef) -> bool {
        self.any || self.keys.iter().any(|key| key.may_match(previous))
    }

    fn visit(&mut self, selector: &Selector, subject: &Reach) {
        let chain = selector
            .combinators
            .iter()
            .rposition(|combinator| {
                matches!(combinator, Combinator::Descendant | Combinator::Child)
            })
            .map_or(0, |index| index + 1);
        let last = selector.compounds.len() - 1;
        for (index, compound) in selector.compounds.iter().enumerate() {
            let reach = match subject {
                _ if index == last => subject.clone(),
                Reach::Subject if index >= chain => Reach::Subject,
                Reach::Subject => Reach::Ancestor(SubjectKey::of(compound)),
                Reach::Ancestor(_) | Reach::Unknown => Reach::Unknown,
            };
            if loses_position(compound) {
                match &reach {
                    Reach::Subject => {}
                    Reach::Ancestor(key) if !key.matches_any() => {
                        if !self.keys.contains(key) {
                            self.keys.push(key.clone());
                        }
                    }
                    Reach::Ancestor(_) | Reach::Unknown => self.any = true,
                }
            }
            for functional in &compound.functional {
                for nested in &functional.selectors {
                    self.visit(nested, &reach);
                }
            }
            for nth in &compound.nth {
                for nested in &nth.filter {
                    self.visit(nested, &reach);
                }
            }
        }
    }
}

fn loses_position(compound: &CompoundSelector) -> bool {
    compound.requires_last_child
        || compound.requires_only_child
        || compound.requires_last_of_type
        || compound.requires_only_of_type
}
