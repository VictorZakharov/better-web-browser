use super::*;

pub(super) fn node_type(state: &HostState, node: Option<&NodeRef>) -> u8 {
    node.map_or(0, |node| match node.data {
        NodeData::Element(_) => 1,
        NodeData::Text(_) => 3,
        NodeData::Cdata(_) => 4,
        NodeData::Comment(_) => 8,
        NodeData::ShadowRoot(_) => 11,
        NodeData::Document if is_document_root(state, node) => 9,
        NodeData::Document => 11,
        NodeData::Doctype { .. } => 10,
        NodeData::ProcessingInstruction { .. } => 7,
    })
}

pub(super) fn node_name(state: &HostState, node: &NodeRef) -> String {
    match &node.data {
        NodeData::Element(_) => element_qualified_name(state, node),
        NodeData::Text(_) => "#text".to_string(),
        NodeData::Cdata(_) => "#cdata-section".to_string(),
        NodeData::Comment(_) => "#comment".to_string(),
        NodeData::ShadowRoot(_) => "#document-fragment".to_string(),
        NodeData::Document if is_document_root(state, node) => "#document".to_string(),
        NodeData::Document => "#document-fragment".to_string(),
        NodeData::Doctype { name, .. } => name.clone(),
        NodeData::ProcessingInstruction { target, .. } => target.clone(),
    }
}

pub(super) fn node_metadata(state: &HostState, node: &NodeRef) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        node_type(state, Some(node)),
        node_name(state, node),
        node.tag_name().unwrap_or_default(),
        node.namespace_uri().unwrap_or_default(),
        if matches!(node.data, NodeData::ShadowRoot(_)) {
            "shadow"
        } else {
            ""
        },
        if node.element().is_some_and(|element| {
            element.attrs.borrow().iter().any(|attribute| {
                attribute.name.ns.as_ref().is_empty()
                    && attribute.name.local.as_ref().starts_with("on")
            })
        }) {
            "handlers"
        } else {
            ""
        },
    )
}

fn is_document_root(state: &HostState, node: &NodeRef) -> bool {
    state
        .document_for(node)
        .is_some_and(|document| document.id() == node.id())
}

pub(super) fn element_qualified_name(state: &HostState, node: &NodeRef) -> String {
    let name = node.qualified_name().unwrap_or_default();
    if node.namespace_uri() == Some(HTML_NAMESPACE) && state.is_html_document_for(node) {
        name.to_ascii_uppercase()
    } else {
        name
    }
}
