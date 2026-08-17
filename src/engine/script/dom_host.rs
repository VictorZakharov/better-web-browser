//! DOM identity, construction, and document-query host operations.

use super::binding_helpers::{argument_id, argument_string, join_node_ids, js_string};
use super::*;

const HTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";

pub(super) fn dom_host_call(
    operation: &str,
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "document" => {
            let document = state.document.clone();
            JsValue::from(state.id_for(&document))
        }
        "nodeType" => JsValue::from(node_type(state, state.node(argument_id(args, 1)).as_ref())),
        "nodeName" => js_string(
            state
                .node(argument_id(args, 1))
                .map(|node| node_name(state, &node))
                .unwrap_or_default(),
        ),
        "nodeMetadata" => js_string(
            state
                .node(argument_id(args, 1))
                .map(|node| node_metadata(state, &node))
                .unwrap_or_default(),
        ),
        "tagName" => js_string(
            state
                .node(argument_id(args, 1))
                .map(|node| element_qualified_name(state, &node))
                .unwrap_or_default(),
        ),
        "localName" => state
            .node(argument_id(args, 1))
            .and_then(|node| node.tag_name().map(str::to_string))
            .map_or_else(JsValue::null, js_string),
        "prefix" => state
            .node(argument_id(args, 1))
            .and_then(|node| {
                node.element()
                    .and_then(|element| element.name.prefix.as_ref().map(ToString::to_string))
            })
            .map_or_else(JsValue::null, js_string),
        "namespaceUri" => state
            .node(argument_id(args, 1))
            .and_then(|node| node.namespace_uri().map(str::to_string))
            .map_or_else(JsValue::null, js_string),
        "ownerDocument" => {
            let owner = state.node(argument_id(args, 1)).and_then(|node| {
                state
                    .document_for(&node)
                    .filter(|document| document.id() != node.id())
            });
            JsValue::from(
                owner
                    .map(|document| state.id_for(&document))
                    .unwrap_or_default(),
            )
        }
        "templateContent" => {
            let contents = state.node(argument_id(args, 1)).and_then(|node| {
                node.element()
                    .and_then(|element| element.template_contents.borrow().clone())
            });
            JsValue::from(contents.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "createElement" => {
            let owner = state.node(argument_id(args, 1));
            let tag_name = argument_string(args, 2, context)?;
            state
                .ensure_node_capacity(1 + usize::from(tag_name.eq_ignore_ascii_case("template")))?;
            JsValue::from(
                owner
                    .map(|owner| Node::create_element_for(&owner, &tag_name))
                    .map(|node| state.id_for(&node))
                    .unwrap_or_default(),
            )
        }
        "createElementNS" => {
            let owner = state.node(argument_id(args, 1));
            let namespace = argument_string(args, 2, context)?;
            let qualified_name = argument_string(args, 3, context)?;
            state.ensure_node_capacity(
                1 + usize::from(qualified_name.eq_ignore_ascii_case("template")),
            )?;
            JsValue::from(
                owner
                    .map(|owner| Node::create_element_ns_for(&owner, &namespace, &qualified_name))
                    .map(|node| state.id_for(&node))
                    .unwrap_or_default(),
            )
        }
        "createText" | "createComment" => {
            let owner = state.node(argument_id(args, 1));
            let contents = argument_string(args, 2, context)?;
            state.ensure_node_capacity(1)?;
            JsValue::from(
                owner
                    .map(|owner| {
                        if operation == "createText" {
                            Node::create_text_for(&owner, &contents)
                        } else {
                            Node::create_comment_for(&owner, &contents)
                        }
                    })
                    .map(|node| state.id_for(&node))
                    .unwrap_or_default(),
            )
        }
        "createDocumentFragment" => {
            let owner = state.node(argument_id(args, 1));
            state.ensure_node_capacity(1)?;
            JsValue::from(
                owner
                    .map(|owner| Node::create_document_fragment_for(&owner))
                    .map(|node| state.id_for(&node))
                    .unwrap_or_default(),
            )
        }
        "cloneNode" => {
            let source = state.node(argument_id(args, 1));
            let deep = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
            if let Some(source) = source.as_ref() {
                state.ensure_node_capacity(if deep { subtree_size(source) } else { 1 })?;
            }
            let clone = source.map(|source| {
                let owner = state.document_for(&source);
                let is_document = owner
                    .as_ref()
                    .is_some_and(|document| document.id() == source.id());
                if is_document {
                    let html = state.is_html_document_for(&source);
                    let clone = Node::clone_document(&source, deep);
                    state.register_document(clone.clone(), html);
                    clone
                } else {
                    let owner = owner.unwrap_or_else(|| source.clone());
                    let clone = Node::clone_for(&owner, &source, deep);
                    state.register_subtree(&clone);
                    clone
                }
            });
            JsValue::from(clone.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "importNode" => {
            let owner = state.node(argument_id(args, 1));
            let source = state.node(argument_id(args, 2));
            let deep = args.get(3).and_then(JsValue::as_boolean).unwrap_or(false);
            if let Some(source) = source.as_ref() {
                state.ensure_node_capacity(if deep { subtree_size(source) } else { 1 })?;
            }
            let clone = owner.zip(source).and_then(|(owner, source)| {
                if matches!(source.data, NodeData::Document)
                    && state
                        .document_for(&source)
                        .is_some_and(|document| document.id() == source.id())
                {
                    return None;
                }
                Some(Node::clone_for(&owner, &source, deep))
            });
            if let Some(clone) = clone.as_ref() {
                state.register_subtree(clone);
            }
            JsValue::from(clone.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "createDocument" => {
            let namespace = argument_string(args, 1, context)?;
            let qualified_name = argument_string(args, 2, context)?;
            JsValue::from(create_document(state, &namespace, &qualified_name)?)
        }
        "createHtmlDocument" => {
            let title = argument_string(args, 1, context)?;
            JsValue::from(create_html_document(state, &title)?)
        }
        "isPrimaryDocument" => {
            let primary = state
                .node(argument_id(args, 1))
                .is_some_and(|node| node.id() == state.document.id());
            JsValue::from(primary)
        }
        "documentCharacterSet" => {
            let primary = state
                .node(argument_id(args, 1))
                .is_some_and(|node| node.id() == state.document.id());
            js_string(if primary {
                state.document_character_set.clone()
            } else {
                "UTF-8".to_string()
            })
        }
        "doctype" => {
            let doctype = state.node(argument_id(args, 1)).and_then(|document| {
                document
                    .children
                    .borrow()
                    .iter()
                    .find(|node| matches!(node.data, NodeData::Doctype { .. }))
                    .cloned()
            });
            JsValue::from(doctype.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "byId" => {
            let root = state.node(argument_id(args, 1));
            let wanted = argument_string(args, 2, context)?;
            let node = (!wanted.is_empty())
                .then_some(root)
                .flatten()
                .and_then(|root| {
                    Node::descendants(&root)
                        .find(|node| node.attr("id").as_deref() == Some(wanted.as_str()))
                });
            JsValue::from(node.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "namedPropertyNames" => js_string(named_property_names(state)),
        "namedProperty" => {
            let wanted = argument_string(args, 1, context)?;
            let nodes = named_property_nodes(state, &wanted);
            js_string(join_node_ids(state, &nodes, false))
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn create_document(state: &mut HostState, namespace: &str, qualified_name: &str) -> JsResult<u32> {
    state.ensure_node_capacity(1 + usize::from(!qualified_name.is_empty()))?;
    let document = Node::create_document();
    if !qualified_name.is_empty() {
        let root = Node::create_element_ns_for(&document, namespace, qualified_name);
        Node::append_child(&document, root);
    }
    Ok(state.register_document(document, false))
}

fn create_html_document(state: &mut HostState, title: &str) -> JsResult<u32> {
    state.ensure_node_capacity(if title.is_empty() { 4 } else { 6 })?;
    let document = Node::create_document();
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
    Node::append_child(&document, html);
    Ok(state.register_document(document, true))
}

fn subtree_size(root: &NodeRef) -> usize {
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

fn node_type(state: &HostState, node: Option<&NodeRef>) -> u8 {
    node.map_or(0, |node| match node.data {
        NodeData::Element(_) => 1,
        NodeData::Text(_) => 3,
        NodeData::Comment(_) => 8,
        NodeData::Document if is_document_root(state, node) => 9,
        NodeData::Document => 11,
        NodeData::Doctype { .. } => 10,
        NodeData::ProcessingInstruction { .. } => 7,
    })
}

fn node_name(state: &HostState, node: &NodeRef) -> String {
    match &node.data {
        NodeData::Element(_) => element_qualified_name(state, node),
        NodeData::Text(_) => "#text".to_string(),
        NodeData::Comment(_) => "#comment".to_string(),
        NodeData::Document if is_document_root(state, node) => "#document".to_string(),
        NodeData::Document => "#document-fragment".to_string(),
        NodeData::Doctype { name, .. } => name.clone(),
        NodeData::ProcessingInstruction { target, .. } => target.clone(),
    }
}

fn node_metadata(state: &HostState, node: &NodeRef) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        node_type(state, Some(node)),
        node_name(state, node),
        node.tag_name().unwrap_or_default(),
        node.namespace_uri().unwrap_or_default(),
    )
}

fn is_document_root(state: &HostState, node: &NodeRef) -> bool {
    state
        .document_for(node)
        .is_some_and(|document| document.id() == node.id())
}

fn element_qualified_name(state: &HostState, node: &NodeRef) -> String {
    let name = node.qualified_name().unwrap_or_default();
    if node.namespace_uri() == Some(HTML_NAMESPACE) && state.is_html_document_for(node) {
        name.to_ascii_uppercase()
    } else {
        name
    }
}

fn named_property_names(state: &HostState) -> String {
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for node in Node::descendants(&state.document) {
        for name in named_values(&node) {
            if seen.insert(name.clone()) {
                names.push(name);
            }
        }
    }
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string())
}

fn named_property_nodes(state: &HostState, wanted: &str) -> Vec<NodeRef> {
    let mut seen = HashSet::new();
    Node::descendants(&state.document)
        .filter(|node| {
            named_values(node).iter().any(|name| name == wanted) && seen.insert(node.id())
        })
        .collect()
}

fn named_values(node: &NodeRef) -> Vec<String> {
    if node.namespace_uri() != Some(HTML_NAMESPACE) {
        return Vec::new();
    }
    let mut names = Vec::with_capacity(2);
    if let Some(id) = node.attr("id").filter(|id| !id.is_empty()) {
        names.push(id);
    }
    if matches!(node.tag_name(), Some("embed" | "form" | "img" | "object"))
        && let Some(name) = node.attr("name").filter(|name| !name.is_empty())
    {
        names.push(name);
    }
    names
}
