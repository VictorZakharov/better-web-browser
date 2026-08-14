//! DOM tree and attribute mutation operations exposed through `__hostCall`.

use super::binding_helpers::{append_html_fragment, argument_id, argument_string, node_label};
use super::*;

pub(super) fn mutation_host_call(
    operation: &str,
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "appendChild" => append_child(args, state),
        "insertBefore" => insert_before(args, state),
        "removeChild" => remove_child(args, state),
        "remove" => remove(args, state),
        "textSet" => set_text(args, context, state)?,
        "attrSet" => set_attribute(args, context, state)?,
        "attrRemove" => remove_attribute(args, context, state)?,
        "innerHtmlSet" => set_inner_html(args, context, state)?,
        "innerHtmlAppend" => append_inner_html(args, context, state)?,
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn append_child(args: &[JsValue], state: &mut HostState) -> JsValue {
    let parent = state.node(argument_id(args, 1));
    let child = state.node(argument_id(args, 2));
    let changed = parent
        .as_ref()
        .zip(child.clone())
        .is_some_and(|(parent, child)| Node::append_child(parent, child));
    if changed {
        state.record_mutation(child.as_ref().or(parent.as_ref()));
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
        state.record_mutation(child.as_ref().or(parent.as_ref()));
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
    let changed = parent
        .zip(child)
        .is_some_and(|(parent, child)| Node::remove_child(&parent, &child));
    if changed {
        state.record_mutation_with_render(requires_render);
        state.diagnose("remove child node".into());
    }
    JsValue::from(changed)
}

fn remove(args: &[JsValue], state: &mut HostState) -> JsValue {
    let node = state.node(argument_id(args, 1));
    let changed = node.as_ref().is_some_and(|node| node.parent().is_some());
    let requires_render = node
        .as_ref()
        .is_some_and(|node| state.mutation_requires_render(node));
    if let Some(node) = node.as_ref() {
        Node::remove_from_parent(node);
    }
    if changed {
        state.record_mutation_with_render(requires_render);
        state.diagnose("remove node".into());
    }
    JsValue::from(changed)
}

fn set_text(args: &[JsValue], context: &mut Context, state: &mut HostState) -> JsResult<JsValue> {
    let contents = argument_string(args, 2, context)?;
    let node = state.node(argument_id(args, 1));
    let changed = node.as_ref().is_some_and(|node| {
        Node::set_text_content(node, &contents);
        state.register_subtree(node);
        true
    });
    if changed {
        state.record_mutation(node.as_ref());
        if let Some(node) = node {
            state.diagnose(format!("set textContent on {}", node_label(&node)));
        }
    }
    Ok(JsValue::from(changed))
}

fn set_attribute(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<JsValue> {
    let name = argument_string(args, 2, context)?;
    let value = argument_string(args, 3, context)?;
    let node = state.node(argument_id(args, 1));
    let changed = node
        .as_ref()
        .is_some_and(|node| node.set_attr(&name, &value));
    if changed {
        state.record_mutation(node.as_ref());
        if let Some(node) = node {
            state.diagnose(format!("set {} on {}", name, node_label(&node)));
            if name.eq_ignore_ascii_case("src") {
                state.queue_dynamic_script(&node);
            }
        }
    }
    Ok(JsValue::from(changed))
}

fn remove_attribute(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<JsValue> {
    let name = argument_string(args, 2, context)?;
    let node = state.node(argument_id(args, 1));
    let changed = node.as_ref().is_some_and(|node| node.remove_attr(&name));
    if changed {
        state.record_mutation(node.as_ref());
        if let Some(node) = node {
            state.diagnose(format!("remove {} from {}", name, node_label(&node)));
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
    let node = state.node(argument_id(args, 1));
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
        state.record_mutation(node.as_ref());
        if let Some(node) = node {
            let action = if append { "append" } else { "replace" };
            state.diagnose(format!("{action} innerHTML of {}", node_label(&node)));
        }
    }
    Ok(JsValue::from(changed))
}
