//! Declarative shadow-root attachment for HTML's `template` start-tag algorithm.
//! https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inhead

use super::super::shadow::DeclarativeShadowOptions;
use super::super::{Node, NodeRef, ShadowRootMode};
use super::Dom;
use html5ever::Attribute;
#[cfg(test)]
#[path = "document/declarative_shadow_tests.rs"]
mod tests;

pub(super) fn attach(
    dom: &Dom,
    location: &NodeRef,
    template: &NodeRef,
    attrs: &[Attribute],
) -> bool {
    let host = dom.resolve_parser_node(location);
    let host = host.as_ref();
    if !valid_shadow_host(host) || host.shadow_root().is_some() {
        return false;
    }
    let mode = match attribute(attrs, "shadowrootmode") {
        Some("open") => ShadowRootMode::Open,
        Some("closed") => ShadowRootMode::Closed,
        _ => return false,
    };
    let Some(contents) = template.element().map(|element| &element.template_contents) else {
        return false;
    };
    let keep_registry_null = attribute(attrs, "shadowrootcustomelementregistry").is_some();
    let Some(root) = Node::attach_declarative_shadow(
        host,
        DeclarativeShadowOptions {
            mode,
            delegates_focus: attribute(attrs, "shadowrootdelegatesfocus").is_some(),
            serializable: attribute(attrs, "shadowrootserializable").is_some(),
            clonable: attribute(attrs, "shadowrootclonable").is_some(),
            manual_slot_assignment: attribute(attrs, "shadowrootslotassignment")
                .is_some_and(|value| value.eq_ignore_ascii_case("manual")),
            registry_is_null: dom.document_registry_is_null || keep_registry_null,
            keep_registry_null,
        },
    ) else {
        return false;
    };
    // html5ever keeps the detached template on its open-element stack. Redirect its
    // contents handle, so subsequent tokens are constructed directly in the shadow tree.
    *contents.borrow_mut() = Some(root);
    true
}

fn attribute<'a>(attrs: &'a [Attribute], name: &str) -> Option<&'a str> {
    attrs.iter().find_map(|attr| {
        (attr.name.ns.as_ref().is_empty() && attr.name.local.as_ref() == name)
            .then_some(attr.value.as_ref())
    })
}

// DOM's attachShadow host restriction applies equally to parser-created roots.
// https://dom.spec.whatwg.org/#dom-element-attachshadow
fn valid_shadow_host(node: &NodeRef) -> bool {
    if node.namespace_uri() != Some("http://www.w3.org/1999/xhtml") {
        return false;
    }
    let Some(name) = node.tag_name() else {
        return false;
    };
    matches!(
        name,
        "article"
            | "aside"
            | "blockquote"
            | "body"
            | "div"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "main"
            | "nav"
            | "p"
            | "section"
            | "span"
    ) || valid_custom_element_name(name)
}

fn valid_custom_element_name(name: &str) -> bool {
    let mut chars = name.chars();
    if !chars.next().is_some_and(|first| first.is_ascii_lowercase())
        || !name.contains('-')
        || matches!(
            name,
            "annotation-xml"
                | "color-profile"
                | "font-face"
                | "font-face-src"
                | "font-face-uri"
                | "font-face-format"
                | "font-face-name"
                | "missing-glyph"
        )
    {
        return false;
    }
    // DOM's ASCII-alpha local-name branch permits other punctuation too. It
    // excludes exactly ASCII whitespace, NUL, slash, and ">"; HTML also
    // excludes ASCII uppercase from custom-element names.
    chars.all(|c| {
        !c.is_ascii_uppercase()
            && !matches!(c, '\0' | '\t' | '\n' | '\x0c' | '\r' | ' ' | '/' | '>')
    })
}
