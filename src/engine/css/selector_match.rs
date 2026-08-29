//! Selector matching against DOM nodes.

use super::*;

pub(super) fn selector_matches(selector: &Selector, node: &NodeRef) -> bool {
    fn matches_at(selector: &Selector, index: usize, node: &NodeRef) -> bool {
        if !compound_matches(&selector.compounds[index], node) {
            return false;
        }
        if index == 0 {
            return true;
        }
        match selector.combinators[index - 1] {
            Combinator::Child => node
                .parent()
                .is_some_and(|parent| matches_at(selector, index - 1, &parent)),
            Combinator::Descendant => {
                let mut ancestor = node.parent();
                while let Some(candidate) = ancestor {
                    if matches_at(selector, index - 1, &candidate) {
                        return true;
                    }
                    ancestor = candidate.parent();
                }
                false
            }
            Combinator::AdjacentSibling => previous_element_siblings(node)
                .next()
                .is_some_and(|sibling| matches_at(selector, index - 1, &sibling)),
            Combinator::GeneralSibling => previous_element_siblings(node)
                .any(|sibling| matches_at(selector, index - 1, &sibling)),
        }
    }

    matches_at(selector, selector.compounds.len() - 1, node)
}

fn previous_element_siblings(node: &NodeRef) -> impl Iterator<Item = NodeRef> {
    let siblings = node
        .parent()
        .map(|parent| parent.children.borrow().clone())
        .unwrap_or_default();
    let index = siblings
        .iter()
        .position(|sibling| sibling.id() == node.id())
        .unwrap_or(0);
    siblings[..index]
        .iter()
        .rev()
        .filter(|sibling| sibling.element().is_some())
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
}

pub(super) fn compound_matches(selector: &CompoundSelector, node: &NodeRef) -> bool {
    if selector.never_matches || node.element().is_none() {
        return false;
    }
    if selector
        .tag
        .as_deref()
        .is_some_and(|tag| node.tag_name() != Some(tag))
    {
        return false;
    }
    if selector
        .id
        .as_deref()
        .is_some_and(|id| node.attr("id").as_deref() != Some(id))
    {
        return false;
    }
    if selector.classes.iter().any(|class| !node.has_class(class)) {
        return false;
    }
    if selector
        .attributes
        .iter()
        .any(|attribute| !attribute_matches(attribute, node))
    {
        return false;
    }
    if selector.requires_link && node.tag_name() != Some("a") {
        return false;
    }
    if selector.requires_root
        && !node
            .parent()
            .is_some_and(|parent| matches!(parent.data, super::dom::NodeData::Document))
    {
        return false;
    }
    if selector.requires_enabled && (!is_disableable(node) || is_disabled(node)) {
        return false;
    }
    if selector.requires_disabled && (!is_disableable(node) || !is_disabled(node)) {
        return false;
    }
    if selector.requires_fullscreen && !node.is_fullscreen() {
        return false;
    }
    if selector.requires_first_child {
        let Some(parent) = node.parent() else {
            return false;
        };
        let is_first = parent
            .children
            .borrow()
            .iter()
            .find(|child| child.element().is_some())
            .is_some_and(|child| child.id() == node.id());
        if !is_first {
            return false;
        }
    }
    if selector.any_of.iter().any(|choices| {
        !choices
            .iter()
            .any(|simple| simple_selector_matches(simple, node))
    }) {
        return false;
    }
    !selector.not.iter().any(|choices| {
        choices
            .iter()
            .any(|simple| simple_selector_matches(simple, node))
    })
}

fn is_disableable(node: &NodeRef) -> bool {
    matches!(
        node.tag_name(),
        Some("button" | "fieldset" | "input" | "optgroup" | "option" | "select" | "textarea")
    )
}

fn is_disabled(node: &NodeRef) -> bool {
    if node.attr("disabled").is_some() {
        return true;
    }
    let mut ancestor = node.parent();
    while let Some(candidate) = ancestor {
        if candidate.tag_name() == Some("fieldset") && candidate.attr("disabled").is_some() {
            return true;
        }
        ancestor = candidate.parent();
    }
    false
}

pub(super) fn simple_selector_matches(simple: &SimpleSelector, node: &NodeRef) -> bool {
    match simple {
        SimpleSelector::Tag(tag) => node.tag_name() == Some(tag),
        SimpleSelector::Id(id) => node.attr("id").as_deref() == Some(id),
        SimpleSelector::Class(class) => node.has_class(class),
    }
}

pub(super) fn attribute_matches(selector: &AttributeSelector, node: &NodeRef) -> bool {
    let Some(actual) = node.attr(&selector.name) else {
        return false;
    };
    if matches!(selector.operator, AttributeOperator::Exists) {
        return true;
    }

    let expected = selector.value.as_str();
    let compare = |left: &str, right: &str| {
        if selector.case_insensitive {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    };
    let normalized_actual;
    let normalized_expected;
    let (actual, expected) = if selector.case_insensitive {
        normalized_actual = actual.to_ascii_lowercase();
        normalized_expected = expected.to_ascii_lowercase();
        (normalized_actual.as_str(), normalized_expected.as_str())
    } else {
        (actual.as_str(), expected)
    };

    match selector.operator {
        AttributeOperator::Exists => true,
        AttributeOperator::Equals => compare(actual, expected),
        AttributeOperator::Includes => actual
            .split_ascii_whitespace()
            .any(|value| compare(value, expected)),
        AttributeOperator::DashMatch => {
            compare(actual, expected)
                || actual
                    .strip_prefix(expected)
                    .is_some_and(|suffix| suffix.starts_with('-'))
        }
        AttributeOperator::Prefix => actual.starts_with(expected),
        AttributeOperator::Suffix => actual.ends_with(expected),
        AttributeOperator::Substring => actual.contains(expected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_adjacent_and_general_element_siblings() {
        let dom = dom::parse("<i></i>text<b id='one'></b><b id='two'></b>");
        let two = dom
            .find_node(dom.elements_named("b").nth(1).unwrap().id())
            .unwrap();
        assert!(selector_matches(&parse_selector("b + b").unwrap(), &two));
        assert!(selector_matches(&parse_selector("i ~ b").unwrap(), &two));
        assert!(!selector_matches(&parse_selector("i + b").unwrap(), &two));
    }

    #[test]
    fn matches_enabled_and_disabled_form_controls() {
        let dom = dom::parse("<button id=on></button><button id=off disabled></button>");
        let buttons = dom.elements_named("button").collect::<Vec<_>>();
        assert!(selector_matches(
            &parse_selector("button:enabled").unwrap(),
            &buttons[0]
        ));
        assert!(selector_matches(
            &parse_selector("button:disabled").unwrap(),
            &buttons[1]
        ));
    }
}
