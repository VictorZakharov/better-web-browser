//! DOM tree and attribute mutation operations exposed through `__hostCall`.

use super::binding_helpers::{append_html_fragment, argument_id, argument_string, node_label};
use super::*;
use crate::limits::{MAX_DOCUMENT_WRITE_BYTES, MAX_DOM_MUTATIONS_PER_TASK};

pub(super) fn mutation_host_call(
    operation: &str,
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if state.task_mutation_count >= MAX_DOM_MUTATIONS_PER_TASK {
        return Err(JsNativeError::range()
            .with_message("DOM mutation task budget exceeded")
            .into());
    }
    let value = match operation {
        "appendChild" => append_child(args, state),
        "insertBefore" => insert_before(args, state),
        "removeChild" => remove_child(args, state),
        "remove" => remove(args, state),
        "textSet" => set_text(args, context, state)?,
        "attrSet" => super::attribute_host::set_attribute(args, context, state)?,
        "attrSetNs" => super::attribute_host::set_attribute_ns(args, context, state, false)?,
        "attrReplaceNs" => super::attribute_host::set_attribute_ns(args, context, state, true)?,
        "attrRemove" => super::attribute_host::remove_attribute(args, context, state)?,
        "attrRemoveNs" => super::attribute_host::remove_attribute_ns(args, context, state)?,
        "innerHtmlSet" => set_inner_html(args, context, state)?,
        "innerHtmlAppend" => append_inner_html(args, context, state)?,
        "documentWrite" => queue_document_write(args, context, state)?,
        _ => return Ok(None),
    };
    Ok(Some(value))
}

pub(super) fn flush_document_write(state: &mut HostState) -> bool {
    if state.pending_document_write.is_empty() {
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
pub(super) fn eval_with_writes(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    source: &str,
) -> JsResult<JsValue> {
    let result = context.eval(Source::from_bytes(source));
    flush_document_write(&mut host.borrow_mut());
    result
}

fn queue_document_write(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<JsValue> {
    let html = argument_string(args, 1, context)?;
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

fn append_child(args: &[JsValue], state: &mut HostState) -> JsValue {
    let parent = state.node(argument_id(args, 1));
    let child = state.node(argument_id(args, 2));
    let previous_parent = child.as_ref().and_then(|child| child.parent());
    let changed = parent
        .as_ref()
        .zip(child.clone())
        .is_some_and(|(parent, child)| Node::append_child(parent, child));
    if changed {
        if let (Some(parent), Some(child)) = (parent.as_ref(), child.as_ref()) {
            state.adopt_subtree(parent, child);
        }
        let kind = child
            .as_ref()
            .map_or(MutationKind::ChildList, child_list_kind);
        let requires_render = child
            .as_ref()
            .is_some_and(|child| state.mutation_requires_render(child));
        state.record_mutation_with_render(parent.as_ref(), kind, requires_render);
        if let Some(previous_parent) = previous_parent.as_ref() {
            state.extend_invalidation_root(previous_parent);
        }
        if let (Some(parent), Some(child)) = (parent.as_ref(), child.as_ref()) {
            state.diagnose(format!(
                "append {} to {}",
                node_label(child),
                node_label(parent)
            ));
            state.queue_dynamic_script(child);
        }
    }
    JsValue::from(if changed {
        child.map(|node| state.id_for(&node)).unwrap_or_default()
    } else {
        0
    })
}

fn insert_before(args: &[JsValue], state: &mut HostState) -> JsValue {
    let parent = state.node(argument_id(args, 1));
    let child = state.node(argument_id(args, 2));
    let previous_parent = child.as_ref().and_then(|child| child.parent());
    let reference_id = argument_id(args, 3);
    let changed = if reference_id == 0 {
        parent
            .as_ref()
            .zip(child.clone())
            .is_some_and(|(parent, child)| Node::append_child(parent, child))
    } else {
        parent
            .as_ref()
            .zip(child.clone())
            .zip(state.node(reference_id))
            .is_some_and(|((parent, child), reference)| {
                Node::insert_before(parent, child, &reference)
            })
    };
    if changed {
        if let (Some(parent), Some(child)) = (parent.as_ref(), child.as_ref()) {
            state.adopt_subtree(parent, child);
        }
        let kind = child
            .as_ref()
            .map_or(MutationKind::ChildList, child_list_kind);
        let requires_render = child
            .as_ref()
            .is_some_and(|child| state.mutation_requires_render(child));
        state.record_mutation_with_render(parent.as_ref(), kind, requires_render);
        if let Some(previous_parent) = previous_parent.as_ref() {
            state.extend_invalidation_root(previous_parent);
        }
        state.diagnose("insert node before sibling".into());
        if let Some(child) = child.as_ref() {
            state.queue_dynamic_script(child);
        }
    }
    JsValue::from(if changed {
        child.map(|node| state.id_for(&node)).unwrap_or_default()
    } else {
        0
    })
}

fn remove_child(args: &[JsValue], state: &mut HostState) -> JsValue {
    let parent = state.node(argument_id(args, 1));
    let child = state.node(argument_id(args, 2));
    let requires_render = child
        .as_ref()
        .or(parent.as_ref())
        .is_some_and(|target| state.mutation_requires_render(target));
    let kind = child
        .as_ref()
        .map_or(MutationKind::ChildList, child_list_kind);
    let changed = parent
        .as_ref()
        .zip(child.as_ref())
        .is_some_and(|(parent, child)| Node::remove_child(parent, child));
    if changed {
        if let Some(child) = child.as_ref() {
            state.record_removed_subtree(child);
        }
        state.record_mutation_with_render(parent.as_ref(), kind, requires_render);
        state.diagnose("remove child node".into());
    }
    JsValue::from(changed)
}

fn remove(args: &[JsValue], state: &mut HostState) -> JsValue {
    let node = state.node(argument_id(args, 1));
    let parent = node.as_ref().and_then(|node| node.parent());
    let changed = node.as_ref().is_some_and(|node| node.parent().is_some());
    let requires_render = node
        .as_ref()
        .is_some_and(|node| state.mutation_requires_render(node));
    let kind = node
        .as_ref()
        .map_or(MutationKind::ChildList, child_list_kind);
    if let Some(node) = node.as_ref() {
        Node::remove_from_parent(node);
    }
    if changed {
        if let Some(node) = node.as_ref() {
            state.record_removed_subtree(node);
        }
        state.record_mutation_with_render(parent.as_ref(), kind, requires_render);
        state.diagnose("remove node".into());
    }
    JsValue::from(changed)
}

fn set_text(args: &[JsValue], context: &mut Context, state: &mut HostState) -> JsResult<JsValue> {
    let contents = argument_string(args, 2, context)?;
    let node = state.node(argument_id(args, 1));
    let mut kind = node.as_ref().map_or(MutationKind::CharacterData, |node| {
        if matches!(&node.data, NodeData::Text(_) | NodeData::Comment(_)) {
            MutationKind::CharacterData
        } else {
            // Element.textContent replaces its child list, including the identity of any text
            // node. Treating that as character data would leave the new node without style state.
            MutationKind::ChildList
        }
    });
    let removed = node
        .as_ref()
        .filter(|node| !matches!(&node.data, NodeData::Text(_) | NodeData::Comment(_)))
        .map(|node| node.children.borrow().clone())
        .unwrap_or_default();
    if removed.iter().any(subtree_contains_style) {
        kind = MutationKind::Stylesheet;
    }
    if !contents.is_empty()
        && node
            .as_ref()
            .is_some_and(|node| !matches!(&node.data, NodeData::Text(_) | NodeData::Comment(_)))
    {
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

fn set_inner_html(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<JsValue> {
    mutate_inner_html(args, context, state, false)
}

fn append_inner_html(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<JsValue> {
    mutate_inner_html(args, context, state, true)
}

fn mutate_inner_html(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
    append: bool,
) -> JsResult<JsValue> {
    let html = argument_string(args, 2, context)?;
    state.ensure_node_capacity(estimated_markup_nodes(&html))?;
    let node = state.node(argument_id(args, 1));
    let removed = node
        .as_ref()
        .filter(|_| !append)
        .map(|node| node.children.borrow().clone())
        .unwrap_or_default();
    let changed = node.as_ref().is_some_and(|node| {
        if append {
            append_html_fragment(&state.document, node, &html);
        } else {
            Node::replace_inner_html(node, &html, true);
        }
        state.register_subtree(node);
        true
    });
    if changed {
        for removed in &removed {
            state.record_removed_subtree(removed);
        }
        let kind = if removed.iter().any(subtree_contains_style)
            || node.as_ref().is_some_and(subtree_contains_style)
        {
            MutationKind::Stylesheet
        } else {
            MutationKind::ChildList
        };
        state.record_mutation(node.as_ref(), kind);
        if let Some(node) = node {
            let action = if append { "append" } else { "replace" };
            state.diagnose(format!("{action} innerHTML of {}", node_label(&node)));
        }
    }
    Ok(JsValue::from(changed))
}

fn estimated_markup_nodes(html: &str) -> usize {
    // Each markup opener can introduce at most one node, with at most one intervening text node.
    // Overestimation is intentional because the retained realm keeps identities for detached nodes.
    html.bytes()
        .filter(|byte| *byte == b'<')
        .count()
        .saturating_mul(2)
        .saturating_add(1)
        .min(MAX_DOM_NODES.saturating_add(1))
}

fn child_list_kind(root: &NodeRef) -> MutationKind<'static> {
    if subtree_contains_style(root) {
        MutationKind::Stylesheet
    } else {
        MutationKind::ChildList
    }
}

fn subtree_contains_style(root: &NodeRef) -> bool {
    Node::descendants(root).any(|node| node.tag_name() == Some("style"))
}

fn contains_ascii_tag(html: &str, tag: &str) -> bool {
    let needle = format!("<{tag}");
    html.as_bytes()
        .windows(needle.len())
        .any(|candidate| candidate.eq_ignore_ascii_case(needle.as_bytes()))
}
