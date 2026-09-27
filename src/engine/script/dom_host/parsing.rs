//! Detached document parsing and metadata, separate from the active parser lifecycle.
use super::*;

#[derive(Clone)]
pub(in crate::engine::script) struct DocumentMetadata {
    pub(in crate::engine::script) url: String,
    pub(in crate::engine::script) content_type: String,
    pub(in crate::engine::script) quirks: bool,
}

pub(super) fn parse_document(
    state: &mut HostState,
    input: &str,
    kind: &str,
    response_url: Option<&str>,
) -> JsResult<u32> {
    parse_document_with_options(state, input, kind, response_url, false)
}

pub(super) fn parse_html_unsafe_document(state: &mut HostState, input: &str) -> JsResult<u32> {
    // HTML Standard: this detached Document is about:blank, UTF-8, scripting
    // disabled, and explicitly permits declarative shadow roots.
    parse_document_with_options(state, input, "text/html", Some("about:blank"), true)
}

fn parse_document_with_options(
    state: &mut HostState,
    input: &str,
    kind: &str,
    response_url: Option<&str>,
    allow_declarative_shadow_roots: bool,
) -> JsResult<u32> {
    let operation = if allow_declarative_shadow_roots {
        "Document.parseHTMLUnsafe"
    } else {
        "DOMParser"
    };
    if input.len() > crate::limits::MAX_HTML_INPUT_BYTES {
        return Err(JsNativeError::range()
            .with_message(format!("{operation} input exceeds the document byte limit"))
            .into());
    }
    let html = kind == "text/html";
    let (document, quirks) = if html {
        let dom = if allow_declarative_shadow_roots {
            crate::engine::dom::document::parse_detached_html_with_shadow_roots(input)
        } else {
            crate::engine::dom::document::parse_detached_html(input)
        };
        if dom
            .errors
            .borrow()
            .iter()
            .any(|error| error.starts_with("safety limit:"))
        {
            return Err(JsNativeError::range()
                .with_message(format!("{operation} input exceeds the DOM safety limits"))
                .into());
        }
        (
            dom.document,
            dom.quirks_mode.get() == html5ever::tree_builder::QuirksMode::Quirks,
        )
    } else {
        let document = match crate::engine::dom::document::xml::parse(input) {
            Ok(document) => document,
            Err(_) if response_url.is_some() => return Ok(0),
            Err(error) => crate::engine::dom::document::xml::error_document(&error),
        };
        (document, false)
    };
    state.ensure_node_capacity(subtree_size(&document) + 1)?;
    // Scripts parsed without a browsing context stay inert even after adoption/cloning.
    let mut stack = vec![document.clone()];
    while let Some(node) = stack.pop() {
        if let Some(element) = node.element() {
            if node.tag_name() == Some("script") {
                element.script_started.set(true);
            }
            stack.extend(element.template_contents.borrow().iter().cloned());
        }
        if let Some(shadow) = node.shadow_root() {
            stack.push(shadow);
        }
        stack.extend(node.children.borrow().iter().cloned());
    }
    state.documents.borrow_mut().document_metadata.insert(
        document.id(),
        DocumentMetadata {
            url: response_url.unwrap_or(&state.document_url).to_string(),
            content_type: kind.to_string(),
            quirks,
        },
    );
    Ok(state.register_document(document, html))
}
