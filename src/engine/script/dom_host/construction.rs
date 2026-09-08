//! Document construction and node-budget preflight for cloning.

use super::*;

pub(super) fn create_document(
    state: &mut HostState,
    namespace: &str,
    qualified_name: &str,
) -> JsResult<u32> {
    state.ensure_node_capacity(2 + usize::from(!qualified_name.is_empty()))?;
    let document = Node::create_document();
    if !qualified_name.is_empty() {
        let root = Node::create_element_ns_for(&document, namespace, qualified_name);
        Node::append_child(&document, root);
    }
    Ok(state.register_document(document, false))
}

pub(super) fn create_html_document(state: &mut HostState, title: &str) -> JsResult<u32> {
    state.ensure_node_capacity(if title.is_empty() { 6 } else { 8 })?;
    let document = Node::create_document();
    let doctype = Node::create_doctype_for(&document, "html", "", "");
    let html = Node::create_element_ns_for(&document, HTML_NAMESPACE, "html");
    let head = Node::create_element_ns_for(&document, HTML_NAMESPACE, "head");
    if !title.is_empty() {
        let title_element = Node::create_element_ns_for(&document, HTML_NAMESPACE, "title");
        Node::append_child(&title_element, Node::create_text_for(&document, title));
        Node::append_child(&head, title_element);
    }
    let body = Node::create_element_ns_for(&document, HTML_NAMESPACE, "body");
    Node::append_child(&html, head);
    Node::append_child(&html, body);
    Node::append_child(&document, doctype);
    Node::append_child(&document, html);
    Ok(state.register_document(document, true))
}

pub(super) fn subtree_size(root: &NodeRef) -> usize {
    let mut count = 0_usize;
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        count = count.saturating_add(1);
        if count > MAX_DOM_NODES {
            return count;
        }
        stack.extend(node.children.borrow().iter().rev().cloned());
        if let Some(template) = node
            .element()
            .and_then(|element| element.template_contents.borrow().clone())
        {
            stack.push(template);
        }
    }
    count
}
