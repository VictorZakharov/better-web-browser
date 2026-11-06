//! Restyle scopes of class or attribute uses followed by a sibling combinator.
//
// A selector reaches its subject from the compound that uses a changed token by moving down
// (child and descendant combinators) or right (sibling combinators). Moving right from a node
// inside the element's subtree stays inside it, and moving right from a later sibling reaches
// only later siblings. So when the first combinator after the use is a sibling combinator, only
// the element's later siblings (and, once the selector also moves down, their subtrees) can
// match differently; when it moves down first, only the element's own subtree can. Either way
// only elements that can match the rule's subject compound are affected, and they restyle as
// element roots. The parent's whole subtree, the earlier fallback, is for a collapsible table in
// a long article most of the document. These are Blink's sibling invalidation sets.
// https://chromium.googlesource.com/chromium/src/+/HEAD/third_party/blink/renderer/core/css/style-invalidation.md#sibling-invalidation
use crate::engine::css::selector_model::Combinator;
use crate::engine::dom::NodeRef;

/// Where the elements a use followed by a sibling combinator can restyle lie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SiblingPath {
    /// Below the element: the selector moves down before moving right.
    Down,
    /// Among the element's later siblings, and below them when the selector also moves down.
    Later { descends: bool },
}

impl SiblingPath {
    /// The path of a use whose compound is followed by `combinators`, or `None` when none of
    /// them is a sibling combinator.
    pub(super) fn of(combinators: &[Combinator]) -> Option<Self> {
        let sibling = |combinator: &Combinator| {
            matches!(
                combinator,
                Combinator::AdjacentSibling | Combinator::GeneralSibling
            )
        };
        if !combinators.iter().any(sibling) {
            return None;
        }
        Some(if combinators.first().is_some_and(sibling) {
            Self::Later {
                descends: !combinators.iter().all(sibling),
            }
        } else {
            Self::Down
        })
    }
}

/// The element siblings after `node` in its parent's children.
pub(super) fn later_element_siblings(node: &NodeRef) -> Vec<NodeRef> {
    let Some(parent) = node.parent() else {
        return Vec::new();
    };
    parent
        .children
        .borrow()
        .iter()
        .skip_while(|child| child.id() != node.id())
        .skip(1)
        .filter(|child| child.element().is_some())
        .cloned()
        .collect()
}
