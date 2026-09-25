//! DOM identity, construction, and document-query host operations.

use super::binding_helpers::{argument_id, argument_string, join_node_ids, js_string};
use super::*;
mod construction;
mod equality;
mod metadata;
mod named;
mod parsing;
use construction::{create_document, create_html_document, subtree_size};
use metadata::{element_qualified_name, node_metadata, node_name, node_type};
pub(super) use named::NamedPropertyIndex;
use named::{named_property_candidates, named_property_names, named_property_nodes};
pub(super) use parsing::DocumentMetadata;

const HTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";

pub(super) fn dom_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "innerHtmlGet" => js_string(
            state
                .node(argument_id(args, 1))
                .map(|node| super::binding_helpers::serialize_children(&node))
                .unwrap_or_default(),
        ),
        "outerHtmlGet" => js_string(
            state
                .node(argument_id(args, 1))
                .map(|node| super::binding_helpers::serialize_html_node(&node))
                .unwrap_or_default(),
        ),
        "xmlSerialize" => {
            let node = state.node(argument_id(args, 1)).ok_or_else(|| {
                JsNativeError::typ().with_message("serializeToString requires a Node")
            })?;
            js_string(super::binding_helpers::serialize_xml_node(&node))
        }
        "nodesEqual" => JsValue::from(
            state
                .node(argument_id(args, 1))
                .zip(state.node(argument_id(args, 2)))
                .is_some_and(|(a, b)| equality::equal(state, &a, &b)),
        ),
        "documentUrl" => js_string(
            state
                .node(argument_id(args, 1))
                .and_then(|node| {
                    state
                        .documents
                        .borrow()
                        .document_metadata
                        .get(&node.id())
                        .map(|metadata| metadata.url.clone())
                })
                .unwrap_or_else(|| state.document_url.clone()),
        ),
        "parseDocument" => {
            let input = argument_string(args, 1)?;
            let kind = argument_string(args, 2)?;
            let response_url = args
                .get(3)
                .filter(|arg| !matches!(arg, JsValue::Undefined))
                .map(|_| argument_string(args, 3))
                .transpose()?;
            JsValue::from(parsing::parse_document(
                state,
                &input,
                &kind,
                response_url.as_deref(),
            )?)
        }
        "documentContentType" => js_string(state.node(argument_id(args, 1)).map_or_else(
            || "text/html".to_string(),
            |node| {
                state
                    .documents
                    .borrow()
                    .document_metadata
                    .get(&node.id())
                    .map_or_else(
                        || {
                            if state.is_html_document_for(&node) {
                                "text/html"
                            } else {
                                "application/xml"
                            }
                            .to_string()
                        },
                        |metadata| metadata.content_type.clone(),
                    )
            },
        )),
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
        "isHtmlDocument" => JsValue::from(
            state
                .node(argument_id(args, 1))
                .is_some_and(|node| state.is_html_document_for(&node)),
        ),
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
            let tag_name = argument_string(args, 2)?;
            state
                .ensure_node_capacity(1 + usize::from(tag_name.eq_ignore_ascii_case("template")))?;
            JsValue::from(
                owner
                    .map(|owner| {
                        Node::create_element_in_document(
                            &owner,
                            &tag_name,
                            state.is_html_document_for(&owner),
                        )
                    })
                    .map(|node| {
                        state.register_subtree(&node);
                        state.id_for(&node)
                    })
                    .unwrap_or_default(),
            )
        }
        "createElementNS" => {
            let owner = state.node(argument_id(args, 1));
            let namespace = argument_string(args, 2)?;
            let qualified_name = argument_string(args, 3)?;
            state.ensure_node_capacity(
                1 + usize::from(qualified_name.eq_ignore_ascii_case("template")),
            )?;
            JsValue::from(
                owner
                    .map(|owner| Node::create_element_ns_for(&owner, &namespace, &qualified_name))
                    .map(|node| {
                        state.register_subtree(&node);
                        state.id_for(&node)
                    })
                    .unwrap_or_default(),
            )
        }
        "createText" | "createComment" => {
            let owner = state.node(argument_id(args, 1));
            let contents = argument_string(args, 2)?;
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
                let is_document = state
                    .document_for(source)
                    .is_some_and(|owner| owner.id() == source.id());
                state.ensure_node_capacity(
                    (if deep { subtree_size(source) } else { 1 }) + usize::from(is_document),
                )?;
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
                    let metadata = state
                        .documents
                        .borrow()
                        .document_metadata
                        .get(&source.id())
                        .cloned();
                    if let Some(metadata) = metadata {
                        state
                            .documents
                            .borrow_mut()
                            .document_metadata
                            .insert(clone.id(), metadata);
                    }
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
            let namespace = argument_string(args, 1)?;
            let qualified_name = argument_string(args, 2)?;
            JsValue::from(create_document(state, &namespace, &qualified_name)?)
        }
        "createHtmlDocument" => {
            let title = argument_string(args, 1)?;
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
        "documentTypeMetadata" => js_string(
            state
                .node(argument_id(args, 1))
                .and_then(|node| match &node.data {
                    NodeData::Doctype {
                        public_id,
                        system_id,
                        ..
                    } => Some(format!("{public_id}\u{1f}{system_id}")),
                    _ => None,
                })
                .unwrap_or_default(),
        ),
        "byId" => {
            let root = state.node(argument_id(args, 1));
            let wanted = argument_string(args, 2)?;
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
        "namedPropertyCandidates" => {
            js_string(named_property_candidates(state, argument_id(args, 1)))
        }
        "namedProperty" => {
            let wanted = argument_string(args, 1)?;
            let nodes = named_property_nodes(state, &wanted);
            js_string(join_node_ids(state, &nodes, false))
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
