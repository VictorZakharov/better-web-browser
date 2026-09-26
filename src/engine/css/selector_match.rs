//! Selector matching against DOM nodes.
mod ancestor_filter;
mod attributes;
mod linguistic;
#[path = "selector_editing.rs"]
mod selector_editing;
mod structural;
pub(super) use ancestor_filter::{AncestorFilter, AncestorFilterCache};

use super::selector_validity::{
    matches_in_range, matches_invalid, matches_optional, matches_out_of_range, matches_required,
    matches_valid,
};
use super::*;
use attributes::attribute_matches;
use selector_editing::matches_read_write;

pub(crate) struct CompiledSelectorList {
    selectors: Vec<Selector>,
}

impl CompiledSelectorList {
    pub(crate) fn matches(&self, node: &NodeRef) -> bool {
        self.selectors
            .iter()
            .any(|selector| selector_matches(selector, node))
    }

    pub(crate) fn matches_with_scope(&self, node: &NodeRef, scope: &NodeRef) -> bool {
        self.selectors
            .iter()
            .any(|selector| selector_matches_with_scope(selector, node, Some(scope.id())))
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
    selector_matches_with_scope(selector, node, None)
}

fn selector_matches_with_scope(selector: &Selector, node: &NodeRef, scope: Option<NodeId>) -> bool {
    fn matches_at(
        selector: &Selector,
        index: usize,
        node: &NodeRef,
        scope: Option<NodeId>,
    ) -> bool {
        if !compound_matches(&selector.compounds[index], node, scope) {
            return false;
        }
        if index == 0 {
            return true;
        }
        match selector.combinators[index - 1] {
            Combinator::Child => node
                .parent()
                .is_some_and(|parent| matches_at(selector, index - 1, &parent, scope)),
            Combinator::Descendant => {
                let mut ancestor = node.parent();
                while let Some(candidate) = ancestor {
                    if matches_at(selector, index - 1, &candidate, scope) {
                        return true;
                    }
                    ancestor = candidate.parent();
                }
                false
            }
            Combinator::AdjacentSibling => previous_element_siblings(node)
                .next()
                .is_some_and(|sibling| matches_at(selector, index - 1, &sibling, scope)),
            Combinator::GeneralSibling => previous_element_siblings(node)
                .any(|sibling| matches_at(selector, index - 1, &sibling, scope)),
        }
    }

    matches_at(selector, selector.compounds.len() - 1, node, scope)
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

pub(super) fn compound_matches(
    selector: &CompoundSelector,
    node: &NodeRef,
    scope: Option<NodeId>,
) -> bool {
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
    if selector.requires_scope {
        let matches_scope = scope.map_or_else(
            || {
                node.parent()
                    .is_some_and(|parent| matches!(parent.data, super::dom::NodeData::Document))
            },
            |scope| scope == node.id(),
        );
        if !matches_scope {
            return false;
        }
    }
    if selector.requires_enabled && (!is_disableable(node) || is_disabled(node)) {
        return false;
    }
    if selector.requires_disabled && (!is_disableable(node) || !is_disabled(node)) {
        return false;
    }
    if selector.requires_read_write && !matches_read_write(node) {
        return false;
    }
    if selector.requires_read_only && matches_read_write(node) {
        return false;
    }
    if selector.requires_hover && !node.is_hovered() {
        return false;
    }
    if selector.requires_focus && !node.is_focused() {
        return false;
    }
    if selector.requires_focus_within && !node.has_focus_within() {
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
    if !structural::compound_structural_matches(selector, node, scope) {
        return false;
    }
    if !linguistic::matches_languages(&selector.languages, node) {
        return false;
    }
    if !linguistic::matches_directions(&selector.directions, node) {
        return false;
    }
    if selector.has.iter().any(|alternatives| {
        !alternatives
            .iter()
            .any(|relative| relative_selector_matches(relative, node))
    }) {
        return false;
    }
    selector.functional.iter().all(|function| {
        let matches = function
            .selectors
            .iter()
            .any(|candidate| selector_matches_with_scope(candidate, node, scope));
        match function.kind {
            FunctionalSelectorKind::Is | FunctionalSelectorKind::Where => matches,
            FunctionalSelectorKind::Not => !matches,
        }
    })
}

fn relative_selector_matches(relative: &RelativeSelector, anchor: &NodeRef) -> bool {
    let mut candidates = match relative.search {
        RelativeSearch::Descendants => vec![anchor.clone()],
        RelativeSearch::Children => anchor.children.borrow().iter().cloned().collect(),
        RelativeSearch::FollowingSiblings
        | RelativeSearch::NextSibling
        | RelativeSearch::LaterSiblings => {
            let Some(parent) = anchor.parent() else {
                return false;
            };
            let siblings = parent.children.borrow();
            let Some(index) = siblings
                .iter()
                .position(|sibling| sibling.id() == anchor.id())
            else {
                return false;
            };
            let mut following = siblings[index + 1..]
                .iter()
                .filter(|sibling| sibling.element().is_some())
                .cloned()
                .collect::<Vec<_>>();
            if matches!(relative.search, RelativeSearch::NextSibling) {
                following.truncate(1);
            }
            following
        }
    };
    let descend = matches!(
        relative.search,
        RelativeSearch::Descendants | RelativeSearch::FollowingSiblings
    );
    while let Some(candidate) = candidates.pop() {
        if candidate.element().is_some()
            && selector_matches_with_scope(&relative.selector, &candidate, Some(anchor.id()))
        {
            return true;
        }
        if descend {
            candidates.extend(candidate.children.borrow().iter().cloned());
        }
    }
    false
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

#[cfg(test)]
#[path = "selector_match_tests.rs"]
mod tests;
