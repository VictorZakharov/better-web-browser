//! XML serialization with namespace fixup for live DOM trees.
//!
//! A prefix is only emitted when it is bound to the node's namespace in that
//! element's scope. Attributes cannot use a default namespace, so unprefixed
//! namespaced attributes receive a stable generated prefix.

use super::super::*;
use std::collections::{HashMap, HashSet};

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";

#[derive(Clone)]
struct Namespaces {
    default: String,
    prefixes: HashMap<String, String>,
    next: usize,
}

impl Default for Namespaces {
    fn default() -> Self {
        Self {
            default: String::new(),
            prefixes: HashMap::from([("xml".into(), XML_NS.into())]),
            next: 1,
        }
    }
}

impl Namespaces {
    fn generated_prefix(&mut self) -> String {
        loop {
            let prefix = format!("ns{}", self.next);
            self.next += 1;
            if !self.prefixes.contains_key(&prefix) {
                return prefix;
            }
        }
    }

    fn prefix_for(&self, namespace: &str) -> Option<&str> {
        self.prefixes
            .iter()
            .filter(|(prefix, value)| prefix.as_str() != "xmlns" && value.as_str() == namespace)
            .map(|(prefix, _)| prefix.as_str())
            .min()
    }
}

pub(in crate::engine::script) fn serialize_xml_node(node: &NodeRef) -> String {
    let mut result = String::new();
    append_node(node, &Namespaces::default(), &mut result);
    result
}

fn append_node(node: &NodeRef, inherited: &Namespaces, output: &mut String) {
    match &node.data {
        NodeData::Element(element) => append_element(node, element, inherited, output),
        NodeData::Text(text) => escape_text(&text.borrow(), output),
        NodeData::Cdata(text) => {
            output.push_str("<![CDATA[");
            output.push_str(&text.borrow().replace("]]>", "]]]]><![CDATA[>"));
            output.push_str("]]>");
        }
        NodeData::Comment(comment) => {
            output.push_str("<!--");
            output.push_str(&comment.borrow());
            output.push_str("-->");
        }
        NodeData::ProcessingInstruction { target, contents } => {
            output.push_str("<?");
            output.push_str(target);
            let contents = contents.borrow();
            if !contents.is_empty() {
                output.push(' ');
                output.push_str(&contents);
            }
            output.push_str("?>");
        }
        NodeData::Doctype {
            name,
            public_id,
            system_id,
        } => {
            output.push_str("<!DOCTYPE ");
            output.push_str(name);
            if !public_id.is_empty() {
                output.push_str(" PUBLIC \"");
                output.push_str(public_id);
                output.push_str("\" \"");
                output.push_str(system_id);
                output.push('"');
            } else if !system_id.is_empty() {
                output.push_str(" SYSTEM \"");
                output.push_str(system_id);
                output.push('"');
            }
            output.push('>');
        }
        NodeData::Document | NodeData::ShadowRoot(_) => {
            for child in node.children.borrow().iter() {
                append_node(child, inherited, output);
            }
        }
    }
}

fn append_element(
    node: &NodeRef,
    element: &crate::engine::dom::ElementData,
    inherited: &Namespaces,
    output: &mut String,
) {
    let mut scope = inherited.clone();
    let mut declarations = Vec::<(String, String)>::new();
    let mut declared_here = HashSet::<String>::new();
    let attributes = element.attrs.borrow();
    // Apply explicit namespace declarations before choosing the element's name.
    for attribute in attributes
        .iter()
        .filter(|attr| attr.name.ns.as_ref() == XMLNS_NS)
    {
        let local = attribute.name.local.as_ref();
        let value = attribute.value.to_string();
        if local == "xmlns" {
            scope.default.clone_from(&value);
            declarations.push(("xmlns".into(), value));
            declared_here.insert(String::new());
        } else {
            scope.prefixes.insert(local.to_string(), value.clone());
            declarations.push((format!("xmlns:{local}"), value));
            declared_here.insert(local.to_string());
        }
    }

    let namespace = element.name.ns.as_ref();
    let local_name = element.name.local.as_ref();
    let element_prefix = if namespace == XML_NS {
        Some("xml".to_string())
    } else if let Some(prefix) = &element.name.prefix {
        let requested = prefix.as_ref();
        if scope
            .prefixes
            .get(requested)
            .is_some_and(|uri| uri == namespace)
        {
            Some(requested.to_string())
        } else if declared_here.contains(requested) {
            let generated = scope.generated_prefix();
            declare_prefix(&mut scope, &mut declarations, &generated, namespace);
            Some(generated)
        } else {
            declare_prefix(&mut scope, &mut declarations, requested, namespace);
            Some(requested.to_string())
        }
    } else if scope.default == namespace {
        None
    } else if declared_here.contains("") {
        let generated = scope.generated_prefix();
        declare_prefix(&mut scope, &mut declarations, &generated, namespace);
        Some(generated)
    } else {
        scope.default = namespace.to_string();
        declarations.push(("xmlns".into(), namespace.to_string()));
        None
    };

    let mut serialized_attributes = Vec::new();
    for attribute in attributes
        .iter()
        .filter(|attr| attr.name.ns.as_ref() != XMLNS_NS)
    {
        let namespace = attribute.name.ns.as_ref();
        let local = attribute.name.local.as_ref();
        let qualified = if namespace.is_empty() {
            local.to_string()
        } else if namespace == XML_NS {
            format!("xml:{local}")
        } else {
            let requested = attribute.name.prefix.as_ref().map(|prefix| prefix.as_ref());
            let prefix = requested
                .filter(|prefix| {
                    scope
                        .prefixes
                        .get(*prefix)
                        .is_some_and(|uri| uri == namespace)
                })
                .map(str::to_string)
                .or_else(|| scope.prefix_for(namespace).map(str::to_string))
                .unwrap_or_else(|| {
                    let candidate = requested
                        .filter(|prefix| {
                            !scope.prefixes.contains_key(*prefix)
                                && !declared_here.contains(*prefix)
                        })
                        .map(str::to_string)
                        .unwrap_or_else(|| scope.generated_prefix());
                    declare_prefix(&mut scope, &mut declarations, &candidate, namespace);
                    candidate
                });
            format!("{prefix}:{local}")
        };
        serialized_attributes.push((qualified, attribute.value.to_string()));
    }

    output.push('<');
    if let Some(prefix) = &element_prefix {
        output.push_str(prefix);
        output.push(':');
    }
    output.push_str(local_name);
    for (name, value) in declarations.into_iter().chain(serialized_attributes) {
        output.push(' ');
        output.push_str(&name);
        output.push_str("=\"");
        escape_attribute(&value, output);
        output.push('"');
    }
    let target = element
        .template_contents
        .borrow()
        .clone()
        .unwrap_or_else(|| node.clone());
    if target.children.borrow().is_empty() {
        if namespace == HTML_NS && is_html_void(local_name) {
            output.push_str(" />");
            return;
        }
        if namespace != HTML_NS {
            output.push_str("/>");
            return;
        }
        // DOM Parsing §5.2.1.1: non-void HTML elements need both tags even in
        // XML serialization, so reparsing as HTML does not swallow siblings.
    }
    output.push('>');
    for child in target.children.borrow().iter() {
        append_node(child, &scope, output);
    }
    output.push_str("</");
    if let Some(prefix) = &element_prefix {
        output.push_str(prefix);
        output.push(':');
    }
    output.push_str(local_name);
    output.push('>');
}

fn declare_prefix(
    scope: &mut Namespaces,
    declarations: &mut Vec<(String, String)>,
    prefix: &str,
    namespace: &str,
) {
    scope
        .prefixes
        .insert(prefix.to_string(), namespace.to_string());
    declarations.push((format!("xmlns:{prefix}"), namespace.to_string()));
}

fn is_html_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "basefont"
            | "bgsound"
            | "br"
            | "col"
            | "embed"
            | "frame"
            | "hr"
            | "img"
            | "input"
            | "keygen"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn escape_text(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '\r' => output.push_str("&#xD;"),
            character => output.push(character),
        }
    }
}

fn escape_attribute(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '"' => output.push_str("&quot;"),
            '\t' => output.push_str("&#x9;"),
            '\n' => output.push_str("&#xA;"),
            '\r' => output.push_str("&#xD;"),
            character => output.push(character),
        }
    }
}
