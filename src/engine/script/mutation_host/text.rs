//! Text replacement preserves DOM identity and budgets while carrying precise style evidence.
use super::*;

fn character_data(node: &NodeRef) -> bool {
    matches!(
        &node.data,
        NodeData::Text(_)
            | NodeData::Cdata(_)
            | NodeData::Comment(_)
            | NodeData::ProcessingInstruction { .. }
    )
}

pub(super) fn set(args: &[JsValue], state: &mut HostState) -> JsResult<JsValue> {
    let contents = argument_string(args, 2)?;
    let node = state.node(argument_id(args, 1));
    let removed = node
        .as_ref()
        .filter(|node| !character_data(node))
        .map(|node| node.children.borrow().clone())
        .unwrap_or_default();
    let mut kind = node.as_ref().map_or(MutationKind::CharacterData, |node| {
        if character_data(node) {
            MutationKind::CharacterData
        } else if node.element().is_some()
            && node
                .namespace_uri()
                .is_none_or(|ns| ns == "http://www.w3.org/1999/xhtml")
            && removed.iter().all(character_data)
        {
            // DOM replace-all still creates a new text node. Do not treat this as
            // CharacterData or reuse the removed node's computed style/identity.
            // https://dom.spec.whatwg.org/#concept-node-string-replace-all
            MutationKind::TextChildren
        } else {
            MutationKind::ChildList
        }
    });
    if removed.iter().any(subtree_contains_style) {
        kind = MutationKind::Stylesheet;
    }
    if matches!(
        kind,
        MutationKind::ChildList | MutationKind::TextChildren | MutationKind::Stylesheet
    ) {
        enforce_tree_budget(state)?;
    }
    if !contents.is_empty() && node.as_ref().is_some_and(|node| !character_data(node)) {
        state.ensure_node_capacity(1)?;
    }
    let changed = node.as_ref().is_some_and(|node| {
        Node::set_text_content(node, &contents);
        state.register_subtree(node);
        true
    });
    if changed {
        for removed in &removed {
            state.record_removed_subtree(removed);
        }
        state.record_mutation(node.as_ref(), kind);
        if let Some(node) = node {
            state.diagnose(format!("set textContent on {}", node_label(&node)));
        }
    }
    Ok(JsValue::from(changed))
}
