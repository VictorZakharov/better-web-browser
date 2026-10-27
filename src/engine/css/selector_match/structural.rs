//! Structural pseudo-class matching against the DOM tree.
use super::*;

pub(super) fn compound_structural_matches(
    selector: &CompoundSelector,
    node: &NodeRef,
    scope: Option<NodeId>,
) -> bool {
    if selector.requires_empty && !matches_empty(node) {
        return false;
    }
    if !(selector.requires_first_child
        || selector.requires_last_child
        || selector.requires_first_of_type
        || selector.requires_last_of_type
        || selector.requires_only_child
        || selector.requires_only_of_type
        || !selector.nth.is_empty())
    {
        return true;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    let children = parent.children.borrow();
    let elements = || children.iter().filter(|child| child.element().is_some());
    let same_type = |child: &NodeRef| {
        child.tag_name() == node.tag_name() && child.namespace_uri() == node.namespace_uri()
    };
    // The 1-based position of `node` among the siblings that pass `include`, and their count.
    // Counting in place keeps these checks free of per-element sibling lists.
    let position = |include: &dyn Fn(&NodeRef) -> bool| {
        let (mut position, mut count) = (None, 0);
        for child in elements().filter(|child| include(child)) {
            count += 1;
            if child.id() == node.id() {
                position = Some(count);
            }
        }
        position.map(|position| (position, count))
    };
    // An element is first (last) when no element sibling precedes (follows) it.
    let first = || {
        elements()
            .next()
            .is_some_and(|child| child.id() == node.id())
    };
    let last = || {
        elements()
            .next_back()
            .is_some_and(|child| child.id() == node.id())
    };
    if (selector.requires_first_child && !first())
        || (selector.requires_last_child && !last())
        || (selector.requires_only_child && !(first() && last()))
    {
        return false;
    }
    if selector.requires_first_of_type
        || selector.requires_last_of_type
        || selector.requires_only_of_type
    {
        let Some((index, count)) = position(&same_type) else {
            return false;
        };
        if (selector.requires_first_of_type && index != 1)
            || (selector.requires_last_of_type && index != count)
            || (selector.requires_only_of_type && count != 1)
        {
            return false;
        }
    }
    selector.nth.iter().all(|nth| {
        let include = |child: &NodeRef| {
            if nth.of_type {
                same_type(child)
            } else {
                nth.filter.is_empty()
                    || nth
                        .filter
                        .iter()
                        .any(|filter| selector_matches_with_scope(filter, child, scope))
            }
        };
        let Some((position, count)) = position(&include) else {
            return false;
        };
        let one_based = if nth.from_end {
            count + 1 - position
        } else {
            position
        };
        matches_an_plus_b(one_based, nth.a, nth.b)
    })
}

/// Any element or non-empty text child, even white space only, defeats `:empty`. The
/// Selectors 4 draft's white-space allowance is implemented by no shipping engine, so this
/// follows the interoperable Selectors 3 definition.
/// https://www.w3.org/TR/selectors-3/#empty-pseudo
/// https://drafts.csswg.org/selectors-4/#the-empty-pseudo
fn matches_empty(node: &NodeRef) -> bool {
    node.children
        .borrow()
        .iter()
        .all(|child| match &child.data {
            dom::NodeData::Element(_) => false,
            dom::NodeData::Text(text) | dom::NodeData::Cdata(text) => text.borrow().is_empty(),
            _ => true,
        })
}

fn matches_an_plus_b(index: usize, a: i32, b: i32) -> bool {
    let delta = index as i64 - i64::from(b);
    let a = i64::from(a);
    if a == 0 {
        delta == 0
    } else {
        delta % a == 0 && delta / a >= 0
    }
}
