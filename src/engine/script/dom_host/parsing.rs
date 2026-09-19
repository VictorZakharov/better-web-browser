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
    if input.len() > crate::limits::MAX_HTML_INPUT_BYTES {
        return Err(JsNativeError::range()
            .with_message("DOMParser input exceeds the document byte limit")
            .into());
    }
    let html = kind == "text/html";
    let (document, quirks) = if html {
        let dom = crate::engine::dom::parse(input);
        if dom
            .errors
            .borrow()
            .iter()
            .any(|error| error.starts_with("safety limit:"))
        {
            return Err(JsNativeError::range()
                .with_message("DOMParser input exceeds the DOM safety limits")
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
