//! DOM traversal, selector matching, serialization, and host-call argument helpers.

use super::*;
mod serialization;
pub(super) use serialization::{serialize_children, serialize_html_node, serialize_xml_node};

pub(super) fn argument_string(arguments: &[JsValue], index: usize) -> JsResult<String> {
    match arguments.get(index) {
        Some(value) => Ok(value.string_value()),
        None => Ok(String::new()),
    }
}

pub(super) fn argument_id(arguments: &[JsValue], index: usize) -> u32 {
    arguments
        .get(index)
        .and_then(JsValue::as_number)
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= f64::from(u32::MAX))
        .map(|value| value as u32)
        .unwrap_or_default()
}

pub(super) fn argument_duration(arguments: &[JsValue], index: usize) -> Duration {
    let milliseconds = arguments
        .get(index)
        .and_then(JsValue::as_number)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(0.0);
    Duration::try_from_secs_f64(milliseconds / 1_000.0).unwrap_or(Duration::MAX)
}

pub(super) fn js_string(value: String) -> JsValue {
    JsValue::from(value)
}

pub(super) fn node_label(node: &NodeRef) -> String {
    match &node.data {
        NodeData::Document => "#document".into(),
        NodeData::Text(_) => "#text".into(),
        NodeData::Cdata(_) => "#cdata-section".into(),
        NodeData::Comment(_) => "#comment".into(),
        _ => node
            .tag_name()
            .map(|tag| format!("<{tag}>"))
            .unwrap_or_else(|| "#node".into()),
    }
}

pub(super) fn append_html_fragment(target: &NodeRef, html: &str) {
    let children = Node::parse_html_fragment(target, html, true);
    for child in children {
        Node::append_child(target, child);
    }
}

pub(super) fn join_node_ids(
    state: &mut HostState,
    nodes: &[NodeRef],
    elements_only: bool,
) -> String {
    nodes
        .iter()
        .filter(|node| !elements_only || node.element().is_some())
        .map(|node| state.id_for(node).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn sibling_id(state: &mut HostState, arguments: &[JsValue], next: bool) -> u32 {
    let Some(node) = state.node(argument_id(arguments, 1)) else {
        return 0;
    };
    let Some(parent) = node.parent() else {
        return 0;
    };
    let children = parent.children.borrow();
    let Some(index) = children.iter().position(|child| child.id() == node.id()) else {
        return 0;
    };
    let sibling = if next {
        children.get(index + 1)
    } else {
        index.checked_sub(1).and_then(|index| children.get(index))
    }
    .cloned();
    drop(children);
    sibling.map(|node| state.id_for(&node)).unwrap_or_default()
}

pub(super) fn query_selector(root: &NodeRef, selector: &str) -> Option<NodeRef> {
    let selector = crate::engine::css::compile_selector_list(selector)?;
    Node::descendants(root)
        .skip(1)
        .find(|node| node.element().is_some() && selector.matches_with_scope(node, root))
}

pub(super) fn query_selector_all(root: &NodeRef, selector: &str) -> Vec<NodeRef> {
    let Some(selector) = crate::engine::css::compile_selector_list(selector) else {
        return Vec::new();
    };
    Node::descendants(root)
        .skip(1)
        .filter(|node| node.element().is_some() && selector.matches_with_scope(node, root))
        .collect()
}

pub(super) fn matches_selector_list(node: &NodeRef, selector: &str) -> bool {
    node.element().is_some()
        && crate::engine::css::compile_selector_list(selector)
            .is_some_and(|selector| selector.matches_with_scope(node, node))
}

pub(super) fn closest_matching_element(node: &NodeRef, selector: &str) -> Option<NodeRef> {
    let selector = crate::engine::css::compile_selector_list(selector)?;
    let scope = node.clone();
    let mut candidate = Some(node.clone());
    while let Some(node) = candidate {
        candidate = node.parent();
        if node.element().is_some() && selector.matches_with_scope(&node, &scope) {
            return Some(node);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_stops_at_the_first_match_and_selector_lists_do_not_duplicate_nodes() {
        let dom = crate::engine::dom::parse(
            "<main><div id='first' class='match'></div><div class='match'></div></main>",
        );
        let first = query_selector(&dom.document, "div").unwrap();
        let all = query_selector_all(&dom.document, "div, .match");

        assert_eq!(first.attr("id").as_deref(), Some("first"));
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id(), first.id());
    }

    #[test]
    fn matches_and_closest_test_candidates_without_rescanning_subtrees() {
        let dom = crate::engine::dom::parse(
            "<main class='shell'><section><a id='target' class='link'></a></section></main>",
        );
        let target = query_selector(&dom.document, "#target").unwrap();

        assert!(matches_selector_list(&target, "main .link, button"));
        assert!(!matches_selector_list(&target, "button, .missing"));
        assert_eq!(
            closest_matching_element(&target, ".shell")
                .and_then(|node| node.tag_name().map(str::to_string)),
            Some("main".to_string())
        );
        assert_eq!(
            closest_matching_element(&target, ".link").map(|node| node.id()),
            Some(target.id())
        );
    }

    #[test]
    fn scope_matches_the_query_root_and_original_closest_element() {
        let dom = crate::engine::dom::parse(
            "<main id=outer><section id=scope><p id=child></p></section></main>",
        );
        let section = dom.elements_named("section").next().unwrap();
        let child = dom.elements_named("p").next().unwrap();
        assert_eq!(
            query_selector(&section, ":scope > p").unwrap().id(),
            child.id()
        );
        assert!(matches_selector_list(&section, ":scope"));
        assert_eq!(
            closest_matching_element(&child, ":scope").unwrap().id(),
            child.id()
        );
    }
}
