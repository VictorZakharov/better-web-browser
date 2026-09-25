//! HTML fragment serialization for innerHTML and outerHTML.

use super::super::*;

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

pub(in crate::engine::script) fn serialize_children(node: &NodeRef) -> String {
    let mut output = String::new();
    let target = contents(node);
    let raw_text = node
        .element()
        .is_some_and(|element| is_raw_text(element.name.ns.as_ref(), element.name.local.as_ref()));
    for child in target.children.borrow().iter() {
        append_node(child, &mut output, raw_text);
    }
    output
}

pub(in crate::engine::script) fn serialize_html_node(node: &NodeRef) -> String {
    let mut output = String::new();
    append_node(node, &mut output, false);
    output
}

fn contents(node: &NodeRef) -> NodeRef {
    node.element()
        .and_then(|element| element.template_contents.borrow().clone())
        .unwrap_or_else(|| node.clone())
}

fn append_node(node: &NodeRef, output: &mut String, raw_text: bool) {
    match &node.data {
        NodeData::Element(element) => {
            let namespace = element.name.ns.as_ref();
            let tag = element.name.local.as_ref();
            output.push('<');
            if namespace != HTML_NS
                && let Some(prefix) = &element.name.prefix
            {
                output.push_str(prefix.as_ref());
                output.push(':');
            }
            output.push_str(tag);
            for attribute in element.attrs.borrow().iter() {
                output.push(' ');
                let namespace = attribute.name.ns.as_ref();
                let prefix = match namespace {
                    XML_NS => Some("xml"),
                    XMLNS_NS if attribute.name.local.as_ref() != "xmlns" => Some("xmlns"),
                    XLINK_NS => Some("xlink"),
                    _ => attribute.name.prefix.as_ref().map(|prefix| prefix.as_ref()),
                };
                if let Some(prefix) = prefix {
                    output.push_str(prefix);
                    output.push(':');
                }
                output.push_str(attribute.name.local.as_ref());
                output.push_str("=\"");
                escape_html(&attribute.value, output, true);
                output.push('"');
            }
            output.push('>');
            if namespace == HTML_NS && is_void(tag) {
                return;
            }
            let target = contents(node);
            let raw_children = is_raw_text(namespace, tag);
            for child in target.children.borrow().iter() {
                append_node(child, output, raw_children);
            }
            output.push_str("</");
            if namespace != HTML_NS
                && let Some(prefix) = &element.name.prefix
            {
                output.push_str(prefix.as_ref());
                output.push(':');
            }
            output.push_str(tag);
            output.push('>');
        }
        NodeData::Text(text) | NodeData::Cdata(text) => {
            let text = text.borrow();
            if raw_text {
                output.push_str(&text);
            } else {
                escape_html(&text, output, false);
            }
        }
        NodeData::Comment(comment) => {
            output.push_str("<!--");
            output.push_str(&comment.borrow());
            output.push_str("-->");
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
        NodeData::ProcessingInstruction { target, contents } => {
            output.push_str("<?");
            output.push_str(target);
            if !contents.borrow().is_empty() {
                output.push(' ');
                output.push_str(&contents.borrow());
            }
            output.push('>');
        }
        NodeData::Document | NodeData::ShadowRoot(_) => {
            for child in node.children.borrow().iter() {
                append_node(child, output, false);
            }
        }
    }
}

fn is_void(tag: &str) -> bool {
    matches!(
        tag,
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

fn is_raw_text(namespace: &str, tag: &str) -> bool {
    namespace == HTML_NS
        && matches!(
            tag,
            "style" | "script" | "xmp" | "iframe" | "noembed" | "noframes" | "plaintext"
        )
}

fn escape_html(value: &str, output: &mut String, attribute: bool) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '\u{a0}' => output.push_str("&nbsp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' if attribute => output.push_str("&quot;"),
            character => output.push(character),
        }
    }
}
