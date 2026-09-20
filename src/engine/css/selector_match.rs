//! Selector matching against DOM nodes.
mod ancestor_filter;
pub(super) use ancestor_filter::{AncestorFilter, AncestorFilterCache};

use super::selector_validity::{
    matches_in_range, matches_invalid, matches_optional, matches_out_of_range, matches_required,
    matches_valid,
};
use super::*;

pub(crate) struct CompiledSelectorList {
    selectors: Vec<Selector>,
}

impl CompiledSelectorList {
    pub(crate) fn matches(&self, node: &NodeRef) -> bool {
        self.selectors
            .iter()
            .any(|selector| selector_matches(selector, node))
    }
}

pub(crate) fn compile_selector_list(input: &str) -> Option<CompiledSelectorList> {
    let selectors = split_css_top_level(input, ',')
        .map(str::trim)
        .map(parse_selector)
        .collect::<Option<Vec<_>>>()?;
    (!selectors.is_empty()).then_some(CompiledSelectorList { selectors })
}

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
        .is_some_and(|id| node.attr_ref("id").as_deref() != Some(id))
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
    if selector.requires_hover && !node.is_hovered() {
        return false;
    }
    if selector.requires_checked && !matches_checked(node) {
        return false;
    }
    if selector.requires_indeterminate && !matches_indeterminate(node) {
        return false;
    }
    if selector.requires_valid && !matches_valid(node) {
        return false;
    }
    if selector.requires_invalid && !matches_invalid(node) {
        return false;
    }
    if selector.requires_required && !matches_required(node) {
        return false;
    }
    if selector.requires_optional && !matches_optional(node) {
        return false;
    }
    if selector.requires_in_range && !matches_in_range(node) {
        return false;
    }
    if selector.requires_out_of_range && !matches_out_of_range(node) {
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
    if selector.requires_first_of_type {
        let Some(parent) = node.parent() else {
            return false;
        };
        let tag_name = node.tag_name();
        let namespace = node.namespace_uri();
        let is_first_of_type = parent
            .children
            .borrow()
            .iter()
            .filter(|child| child.element().is_some())
            .find(|child| child.tag_name() == tag_name && child.namespace_uri() == namespace)
            .is_some_and(|child| child.id() == node.id());
        if !is_first_of_type {
            return false;
        }
    }
    if selector.requires_last_child {
        let Some(parent) = node.parent() else {
            return false;
        };
        let is_last = parent
            .children
            .borrow()
            .iter()
            .rev()
            .find(|child| child.element().is_some())
            .is_some_and(|child| child.id() == node.id());
        if !is_last {
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

pub(crate) fn is_disabled(node: &NodeRef) -> bool {
    if node.attr_ref("disabled").is_some() {
        return true;
    }
    if node.tag_name() == Some("option") {
        return node.parent().is_some_and(|parent| {
            parent.tag_name() == Some("optgroup") && parent.attr_ref("disabled").is_some()
        });
    }
    if node.tag_name() == Some("optgroup") {
        return false;
    }
    let mut ancestor = node.parent();
    while let Some(candidate) = ancestor {
        if candidate.tag_name() == Some("fieldset") && candidate.attr_ref("disabled").is_some() {
            let first_legend = candidate
                .children
                .borrow()
                .iter()
                .find(|child| child.tag_name() == Some("legend"))
                .cloned();
            let inside_legend = first_legend.is_some_and(|legend| {
                let mut parent = node.parent();
                while let Some(current) = parent {
                    if current.id() == legend.id() {
                        return true;
                    }
                    parent = current.parent();
                }
                false
            });
            if !inside_legend {
                return true;
            }
        }
        ancestor = candidate.parent();
    }
    false
}

pub(super) fn simple_selector_matches(simple: &SimpleSelector, node: &NodeRef) -> bool {
    match simple {
        SimpleSelector::State(name) => match name.as_str() {
            "checked" => matches_checked(node),
            "indeterminate" => matches_indeterminate(node),
            "disabled" => is_disableable(node) && is_disabled(node),
            "enabled" => is_disableable(node) && !is_disabled(node),
            "valid" => matches_valid(node),
            "invalid" => matches_invalid(node),
            "required" => matches_required(node),
            "optional" => matches_optional(node),
            "in-range" => matches_in_range(node),
            "out-of-range" => matches_out_of_range(node),
            _ => false,
        },
        SimpleSelector::Tag(tag) => node.tag_name() == Some(tag),
        SimpleSelector::Id(id) => node.attr_ref("id").as_deref() == Some(id),
        SimpleSelector::Class(class) => node.has_class(class),
        SimpleSelector::Attribute(attribute) => attribute_matches(attribute, node),
    }
}

fn matches_checked(node: &NodeRef) -> bool {
    // Options use authoritative selectedness: the `selected` attribute is the
    // default, while user picks and `selected` writes live in control state.
    (node.is_checkable() && node.checked())
        || (node.tag_name() == Some("option") && node.control_state_snapshot().selectedness)
}

fn matches_indeterminate(node: &NodeRef) -> bool {
    if node.is_radio() {
        return !node.checked() && node.radio_group().iter().all(|other| !other.checked());
    }
    (node.is_checkable() && node.indeterminate())
        || (node.tag_name() == Some("progress") && node.attr("value").is_none())
}

pub(super) fn attribute_matches(selector: &AttributeSelector, node: &NodeRef) -> bool {
    let Some(actual) = node.attr_ref(&selector.name) else {
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
        (&*actual, expected)
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

    #[test]
    fn matches_first_element_of_the_same_expanded_type() {
        let dom = dom::parse("<i></i>text<b id='first'></b><em></em><b id='second'></b>");
        let nodes = dom.elements_named("b").collect::<Vec<_>>();
        let selector = parse_selector("b:first-of-type").unwrap();

        assert!(selector_matches(&selector, &nodes[0]));
        assert!(!selector_matches(&selector, &nodes[1]));
    }

    #[test]
    fn functional_pseudo_classes_match_attribute_selectors() {
        let dom = dom::parse(
            "<main is-two-columns_ force-default-style></main><main id='single'></main>",
        );
        let nodes = dom.elements_named("main").collect::<Vec<_>>();

        assert!(!selector_matches(
            &parse_selector("main:not([is-two-columns_])").unwrap(),
            &nodes[0]
        ));
        assert!(selector_matches(
            &parse_selector("main:not([is-two-columns_])").unwrap(),
            &nodes[1]
        ));
        assert!(selector_matches(
            &parse_selector("main:is([force-default-style], .fallback)").unwrap(),
            &nodes[0]
        ));
    }
}
