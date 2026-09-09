//! Buffered document writing and parser-insertion capture.
use super::*;
use std::path::Path;
pub(in crate::engine::script) fn flush_document_write(state: &mut HostState) -> bool {
    // The retained parser consumes captured writes at its insertion point after this script
    // returns. Synchronous document.write re-entry (including nested evaluation before return)
    // remains a separate contract; the old fragment helper is not a conforming replacement.
    if state.capture_parser_writes || state.pending_document_write.is_empty() {
        return false;
    }
    let html = std::mem::take(&mut state.pending_document_write);
    let document = state.document.clone();
    let target = Node::descendants(&document)
        .find(|node| node.tag_name() == Some("body"))
        .or_else(|| {
            document
                .children
                .borrow()
                .iter()
                .find(|node| node.element().is_some())
                .cloned()
        })
        .unwrap_or_else(|| document.clone());
    append_html_fragment(&document, &target, &html);
    state.register_subtree(&target);
    let kind = if contains_ascii_tag(&html, "style") {
        MutationKind::Stylesheet
    } else {
        MutationKind::ChildList
    };
    state.record_mutation(Some(&target), kind);
    state.diagnose(format!(
        "append buffered document.write markup to {}",
        node_label(&target)
    ));
    true
}

/// Coalesces writes from one classic script so the fragment tokenizer sees one continuous input
/// stream, including entity and start-tag state, instead of reparsing every write independently.
pub(in crate::engine::script) fn eval_with_writes(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    source: &str,
    source_url: &str,
) -> JsResult<JsValue> {
    let result = context.eval(Source::from_bytes(source).with_path(Path::new(source_url)));
    flush_document_write(&mut host.borrow_mut());
    result
}

pub(super) fn queue_document_write(args: &[JsValue], state: &mut HostState) -> JsResult<JsValue> {
    let html = argument_string(args, 1)?;
    if state.pending_document_write.len() > MAX_DOCUMENT_WRITE_BYTES.saturating_sub(html.len()) {
        return Err(JsNativeError::range()
            .with_message("document.write output exceeds the page limit")
            .into());
    }
    state.ensure_node_capacity(
        estimated_markup_nodes(&state.pending_document_write)
            .saturating_add(estimated_markup_nodes(&html)),
    )?;
    state.pending_document_write.push_str(&html);
    Ok(JsValue::undefined())
}
