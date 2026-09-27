//! HTML fragment serialization for innerHTML, outerHTML, and getHTML().

use super::super::*;
use std::collections::HashSet;

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
type RegistryKindLookup<'a> = dyn Fn(&NodeRef) -> (bool, bool) + 'a;

pub(in crate::engine::script) fn serialize_children(node: &NodeRef) -> String {
    serialize_fragment(node, &HtmlSerializationOptions::default(), false)
}

pub(in crate::engine::script) fn serialize_children_with_shadow_roots(
    node: &NodeRef,
    serializable_shadow_roots: bool,
    shadow_roots: &HashSet<NodeId>,
    document_registry_kind: &RegistryKindLookup<'_>,
) -> String {
    let options = HtmlSerializationOptions {
        serializable_shadow_roots,
        shadow_roots: Some(shadow_roots),
        document_registry_kind: Some(document_registry_kind),
    };
    serialize_fragment(node, &options, true)
}

#[derive(Default)]
struct HtmlSerializationOptions<'a> {
    serializable_shadow_roots: bool,
    shadow_roots: Option<&'a HashSet<NodeId>>,
    document_registry_kind: Option<&'a RegistryKindLookup<'a>>,
}

fn serialize_fragment(
    node: &NodeRef,
    options: &HtmlSerializationOptions<'_>,
    void_is_empty: bool,
) -> String {
    if void_is_empty
        && node.element().is_some_and(|element| {
            element.name.ns.as_ref() == HTML_NS && is_void(element.name.local.as_ref())
        })
    {
        return String::new();
    }
    let mut output = String::new();
    let target = contents(node);
    let raw_text = node
        .element()
        .is_some_and(|element| is_raw_text(element.name.ns.as_ref(), element.name.local.as_ref()));
    append_selected_shadow_root(&target, &mut output, options);
    for child in target.children.borrow().iter() {
        append_node(child, &mut output, raw_text, options);
    }
    output
}

pub(in crate::engine::script) fn serialize_html_node(node: &NodeRef) -> String {
    let mut output = String::new();
    append_node(
        node,
        &mut output,
        false,
        &HtmlSerializationOptions::default(),
    );
    output
}

fn contents(node: &NodeRef) -> NodeRef {
    node.element()
        .and_then(|element| element.template_contents.borrow().clone())
        .unwrap_or_else(|| node.clone())
}

fn append_selected_shadow_root(
    node: &NodeRef,
    output: &mut String,
    options: &HtmlSerializationOptions<'_>,
) {
    let Some(root) = node.shadow_root() else {
        return;
    };
    let NodeData::ShadowRoot(shadow) = &root.data else {
        return;
    };
    let selected = (options.serializable_shadow_roots && shadow.serializable)
        || options
            .shadow_roots
            .is_some_and(|roots| roots.contains(&root.id()));
    if !selected {
        return;
    }

    // HTML Standard: fragment serialization emits a shadow host's selected root
    // as a declarative template before its light-DOM children.
    output.push_str("<template shadowrootmode=\"");
    output.push_str(shadow.mode.as_str());
    output.push('"');
    if shadow.delegates_focus {
        output.push_str(" shadowrootdelegatesfocus=\"\"");
    }
    if shadow.serializable {
        output.push_str(" shadowrootserializable=\"\"");
    }
    if shadow.manual_slot_assignment {
        output.push_str(" shadowrootslotassignment=\"manual\"");
    }
    if shadow.clonable {
        output.push_str(" shadowrootclonable=\"\"");
    }
    // HTML omits this marker only when both registries are null or both are
    // global. A scoped registry object itself cannot be serialized in markup.
    let (document_is_global, document_is_null) = options
        .document_registry_kind
        .map_or((true, false), |kind| kind(&root));
    let both_global = document_is_global && shadow.registry_is_global.get();
    let both_null = document_is_null && shadow.registry_is_null.get();
    if !both_global && !both_null {
        output.push_str(" shadowrootcustomelementregistry=\"\"");
    }
    output.push('>');
    for child in root.children.borrow().iter() {
        append_node(child, output, false, options);
    }
    output.push_str("</template>");
}

fn append_node(
    node: &NodeRef,
    output: &mut String,
    raw_text: bool,
    options: &HtmlSerializationOptions<'_>,
) {
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
            append_selected_shadow_root(&target, output, options);
            for child in target.children.borrow().iter() {
                append_node(child, output, raw_children, options);
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
                append_node(child, output, false, options);
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
