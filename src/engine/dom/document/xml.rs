//! Bounded, namespace-aware XML document construction. No external entities are fetched.
use super::super::{Node, NodeData, NodeRef};
use crate::limits::{MAX_DOM_DEPTH, MAX_DOM_NODES, MAX_HTML_INPUT_BYTES};
use ::xml::common::Position;
use ::xml::reader::{ParserConfig, XmlEvent};
use html5ever::{Attribute, QualName};
use std::cell::RefCell;
use std::rc::Rc;
mod namespaces;

pub(crate) fn error_document(error: &str) -> NodeRef {
    let document = Node::create_document();
    let root = Node::create_element_ns_for(
        &document,
        "http://www.mozilla.org/newlayout/xml/parsererror.xml",
        "parsererror",
    );
    Node::append_child(&root, Node::create_text_for(&document, error));
    Node::append_child(&document, root);
    document
}

pub(crate) fn parse(input: &str) -> Result<NodeRef, String> {
    // DOMParser receives an already-decoded string. Force UTF-8, but consume its
    // optional leading BOM before the reader's encoding override disables sniffing.
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut reader = ParserConfig::new()
        .ignore_comments(false)
        .allow_multiple_root_elements(false)
        .ignore_root_level_whitespace(true)
        .override_encoding(Some(::xml::Encoding::Utf8))
        .ignore_invalid_encoding_declarations(true)
        .max_entity_expansion_length(MAX_HTML_INPUT_BYTES)
        .max_entity_expansion_depth(16)
        .max_data_length(MAX_HTML_INPUT_BYTES)
        .create_reader(input.as_bytes());
    let document = Node::create_document();
    let mut declarations = namespaces::Declarations::new(input);
    let mut stack = vec![document.clone()];
    let mut namespaces = vec![::xml::namespace::Namespace::empty()];
    let mut count = 1usize;
    let mut expanded_bytes = 0usize;
    loop {
        let event = reader.next().map_err(|error| error.to_string())?;
        let bytes = match &event {
            XmlEvent::Characters(text)
            | XmlEvent::Whitespace(text)
            | XmlEvent::CData(text)
            | XmlEvent::Comment(text) => text.len(),
            XmlEvent::StartElement { attributes, .. } => {
                attributes.iter().map(|attr| attr.value.len()).sum()
            }
            _ => 0,
        };
        expanded_bytes = expanded_bytes.saturating_add(bytes);
        if expanded_bytes > MAX_HTML_INPUT_BYTES {
            return Err("XML expanded text limit exceeded".into());
        }
        let parent = stack.last().unwrap();
        let child = match event {
            XmlEvent::StartDocument { .. } => continue,
            XmlEvent::EndDocument => break,
            XmlEvent::StartElement {
                name,
                attributes,
                namespace,
            } => {
                if stack.len() >= MAX_DOM_DEPTH {
                    return Err("XML depth limit exceeded".into());
                }
                let qualified = name.prefix.as_ref().map_or_else(
                    || name.local_name.clone(),
                    |prefix| format!("{prefix}:{}", name.local_name),
                );
                let node = Node::create_element_ns_for(
                    &document,
                    name.namespace.as_deref().unwrap_or_default(),
                    &qualified,
                );
                let mut expanded = std::collections::HashSet::new();
                let mut attrs = Vec::new();
                for attr in attributes {
                    if !expanded.insert((attr.name.namespace.clone(), attr.name.local_name.clone()))
                    {
                        return Err("Duplicate expanded attribute name".into());
                    }
                    attrs.push(Attribute {
                        name: QualName::new(
                            attr.name.prefix.map(Into::into),
                            attr.name.namespace.unwrap_or_default().into(),
                            attr.name.local_name.into(),
                        ),
                        value: attr.value.into(),
                    });
                }
                let declared = declarations.at(reader.position()).unwrap_or_else(|| {
                    namespace
                        .iter()
                        .filter(|(prefix, uri)| {
                            *prefix != "xml"
                                && *prefix != "xmlns"
                                && namespaces.last().unwrap().get(prefix) != Some(uri)
                        })
                        .map(|(prefix, _)| prefix.to_string())
                        .collect()
                });
                for prefix in declared {
                    let uri = namespace.get(&prefix).unwrap_or_default();
                    attrs.push(Attribute {
                        name: QualName::new(
                            (!prefix.is_empty()).then(|| "xmlns".into()),
                            "http://www.w3.org/2000/xmlns/".into(),
                            if prefix.is_empty() { "xmlns" } else { &prefix }.into(),
                        ),
                        value: uri.into(),
                    });
                }
                *node.element().unwrap().attrs.borrow_mut() = attrs;
                Node::append_child(parent, node.clone());
                stack.push(node);
                namespaces.push(namespace);
                count += 1;
                if count >= MAX_DOM_NODES {
                    return Err("XML node limit exceeded".into());
                }
                continue;
            }
            XmlEvent::EndElement { .. } => {
                stack.pop();
                namespaces.pop();
                continue;
            }
            XmlEvent::Characters(text) | XmlEvent::Whitespace(text) => {
                Node::create_text_for(&document, &text)
            }
            XmlEvent::CData(text) => Node::new_in(
                Rc::clone(&document.identity),
                NodeData::Cdata(RefCell::new(text)),
            ),
            XmlEvent::Comment(text) => Node::create_comment_for(&document, &text),
            XmlEvent::ProcessingInstruction { name, data } => Node::new_in(
                Rc::clone(&document.identity),
                NodeData::ProcessingInstruction {
                    target: name,
                    contents: RefCell::new(data.unwrap_or_default()),
                },
            ),
            XmlEvent::Doctype { .. } => {
                let ids = reader.doctype_ids().ok_or("Missing doctype identifiers")?;
                Node::create_doctype_for(
                    &document,
                    ids.name(),
                    ids.public_id().unwrap_or_default(),
                    ids.system_id().unwrap_or_default(),
                )
            }
        };
        count += 1;
        if count >= MAX_DOM_NODES {
            return Err("XML node limit exceeded".into());
        }
        Node::append_child(parent, child);
    }
    Ok(document)
}
