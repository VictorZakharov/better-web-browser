//! The subject state that structural and user-action pseudo-classes read.
//
// Universal structural rules such as `.content > :first-child` and `:focus` reach every
// element, so style sharing would revalidate each of them for every hit. When nothing else in
// a rule is left open, its result follows from these few facts about the subject alone, and
// comparing them once replaces evaluating every such rule.
use super::*;

/// Facts about an element's position among its element siblings and its hover and focus
/// state, or the subset of them a rule reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::engine::css::cascade) struct StateFacts(u8);

impl StateFacts {
    pub(super) const FIRST_CHILD: Self = Self(1);
    pub(super) const LAST_CHILD: Self = Self(1 << 1);
    pub(super) const FIRST_OF_TYPE: Self = Self(1 << 2);
    pub(super) const LAST_OF_TYPE: Self = Self(1 << 3);
    pub(super) const HOVER: Self = Self(1 << 4);
    pub(super) const FOCUS: Self = Self(1 << 5);
    const POSITION: u8 = 0b1111;

    pub(super) fn insert(&mut self, facts: Self) {
        self.0 |= facts.0;
    }

    pub(super) fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The facts in `read` that hold for `node`, matching `selector_match`'s structural and
    /// user-action pseudo-classes.
    pub(super) fn of(node: &NodeRef, read: Self) -> Self {
        let mut facts = 0;
        if read.0 & Self::POSITION != 0
            && let Some(parent) = node.parent()
        {
            let children = parent.children.borrow();
            let elements = || children.iter().filter(|child| child.element().is_some());
            let same_type = |child: &&NodeRef| {
                child.tag_name() == node.tag_name() && child.namespace_uri() == node.namespace_uri()
            };
            let is_node =
                |child: Option<&NodeRef>| child.is_some_and(|child| child.id() == node.id());
            let reads = |fact: Self| read.0 & fact.0 != 0;
            if reads(Self::FIRST_CHILD) && is_node(elements().next()) {
                facts |= Self::FIRST_CHILD.0;
            }
            if reads(Self::LAST_CHILD) && is_node(elements().next_back()) {
                facts |= Self::LAST_CHILD.0;
            }
            if reads(Self::FIRST_OF_TYPE) && is_node(elements().find(same_type)) {
                facts |= Self::FIRST_OF_TYPE.0;
            }
            if reads(Self::LAST_OF_TYPE) && is_node(elements().rfind(same_type)) {
                facts |= Self::LAST_OF_TYPE.0;
            }
        }
        if read.0 & Self::HOVER.0 != 0 && node.is_hovered() {
            facts |= Self::HOVER.0;
        }
        if read.0 & Self::FOCUS.0 != 0 && node.is_focused() {
            facts |= Self::FOCUS.0;
        }
        Self(facts)
    }
}

impl std::ops::BitOr for StateFacts {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// The facts the given candidate rules read, and their values for `node`: another element
/// with equal facts gets the same result from each [`RuleSharing::SubjectState`] rule.
pub(super) fn subject_state(
    node: &NodeRef,
    rules: impl IntoIterator<Item = RuleSharing>,
) -> (StateFacts, StateFacts) {
    let mut read = StateFacts::default();
    for rule in rules {
        if let RuleSharing::SubjectState(facts) = rule {
            read.insert(facts);
        }
    }
    (read, StateFacts::of(node, read))
}
