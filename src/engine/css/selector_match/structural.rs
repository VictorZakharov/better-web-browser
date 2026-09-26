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
    let elements = children
        .iter()
        .filter(|child| child.element().is_some())
        .collect::<Vec<_>>();
    let Some(index) = elements.iter().position(|child| child.id() == node.id()) else {
        return false;
    };
    let same_type = |child: &&NodeRef| {
        child.tag_name() == node.tag_name() && child.namespace_uri() == node.namespace_uri()
    };
    let of_type = elements
        .iter()
        .copied()
        .filter(same_type)
        .collect::<Vec<_>>();
    let Some(type_index) = of_type.iter().position(|child| child.id() == node.id()) else {
        return false;
    };
    if (selector.requires_first_child && index != 0)
        || (selector.requires_last_child && index + 1 != elements.len())
        || (selector.requires_first_of_type && type_index != 0)
        || (selector.requires_last_of_type && type_index + 1 != of_type.len())
        || (selector.requires_only_child && elements.len() != 1)
        || (selector.requires_only_of_type && of_type.len() != 1)
    {
        return false;
    }
    selector.nth.iter().all(|nth| {
        let matching = if nth.of_type {
            of_type.clone()
        } else if nth.filter.is_empty() {
            elements.clone()
        } else {
            elements
                .iter()
                .copied()
                .filter(|child| {
                    nth.filter
                        .iter()
                        .any(|filter| selector_matches_with_scope(filter, child, scope))
                })
                .collect::<Vec<_>>()
        };
        let Some(position) = matching.iter().position(|child| child.id() == node.id()) else {
            return false;
        };
        let one_based = if nth.from_end {
            matching.len() - position
        } else {
            position + 1
        };
        matches_an_plus_b(one_based, nth.a, nth.b)
    })
}

fn matches_empty(node: &NodeRef) -> bool {
    node.children
        .borrow()
        .iter()
        .all(|child| match &child.data {
            dom::NodeData::Element(_) => false,
            dom::NodeData::Text(text) | dom::NodeData::Cdata(text) => text
                .borrow()
                .chars()
                .all(|character| matches!(character, '\t' | '\n' | '\x0c' | '\r' | ' ')),
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
