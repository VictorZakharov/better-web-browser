//! HTML Standard getHTML() options, separate from legacy innerHTML serialization.

use super::binding_helpers::{argument_id, js_string, serialize_children_with_shadow_roots};
use super::*;

pub(super) fn get_html_host_call(args: &[JsValue], state: &mut HostState) -> JsResult<JsValue> {
    let Some(node) = state.node(argument_id(args, 1)) else {
        return Err(JsNativeError::typ()
            .with_message("Invalid getHTML receiver")
            .into());
    };
    if !matches!(node.data, NodeData::Element(_) | NodeData::ShadowRoot(_)) {
        return Err(JsNativeError::typ()
            .with_message("Invalid getHTML receiver")
            .into());
    }
    let serializable = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
    let mut explicit_roots = HashSet::new();
    for index in 3..args.len() {
        let Some(root) = state.node(argument_id(args, index)) else {
            return Err(JsNativeError::typ()
                .with_message("Expected a ShadowRoot")
                .into());
        };
        if !matches!(root.data, NodeData::ShadowRoot(_)) {
            return Err(JsNativeError::typ()
                .with_message("Expected a ShadowRoot")
                .into());
        }
        explicit_roots.insert(root.id());
    }
    let document_registry_kind = |root: &NodeRef| {
        let owner = state.document_for(root);
        let global = owner
            .as_ref()
            .is_some_and(|document| document.id() == state.document.id());
        let scoped = owner.as_ref().is_some_and(|document| {
            state
                .documents
                .borrow()
                .scoped_custom_element_documents
                .contains(&document.id().document())
        });
        (global, !global && !scoped)
    };
    Ok(js_string(serialize_children_with_shadow_roots(
        &node,
        serializable,
        &explicit_roots,
        &document_registry_kind,
    )))
}
