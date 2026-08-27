//! Attribute reads and namespace-aware mutations exposed through `__hostCall`.

use super::binding_helpers::{argument_id, argument_string, js_string, node_label};
use super::*;

pub(super) fn attribute_host_call(
    operation: &str,
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "attrGet" => {
            let name = argument_string(args, 2, context)?;
            state
                .node(argument_id(args, 1))
                .and_then(|node| node.attr_qualified(&name))
                .map_or_else(JsValue::null, js_string)
        }
        "attrGetNs" => {
            let namespace = argument_string(args, 2, context)?;
            let local_name = argument_string(args, 3, context)?;
            state
                .node(argument_id(args, 1))
                .and_then(|node| node.attr_ns(optional(&namespace), &local_name))
                .map_or_else(JsValue::null, js_string)
        }
        "attrHas" => {
            let name = argument_string(args, 2, context)?;
            JsValue::from(
                state
                    .node(argument_id(args, 1))
                    .is_some_and(|node| node.attr_qualified(&name).is_some()),
            )
        }
        "attrHasNs" => {
            let namespace = argument_string(args, 2, context)?;
            let local_name = argument_string(args, 3, context)?;
            JsValue::from(
                state
                    .node(argument_id(args, 1))
                    .is_some_and(|node| node.attr_ns(optional(&namespace), &local_name).is_some()),
            )
        }
        "attrRecords" => js_string(attribute_records(state, argument_id(args, 1))),
        _ => return Ok(None),
    };
    Ok(Some(value))
}

pub(super) fn set_attribute(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<JsValue> {
    let name = argument_string(args, 2, context)?;
    let value = argument_string(args, 3, context)?;
    let node = state.node(argument_id(args, 1));
    let changed = node
        .as_ref()
        .is_some_and(|node| node.set_attr_qualified(&name, &value));
    record_attribute_mutation(state, node.as_ref(), &name, changed, true);
    Ok(JsValue::from(changed))
}

pub(super) fn set_attribute_ns(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
    replace: bool,
) -> JsResult<JsValue> {
    let namespace = argument_string(args, 2, context)?;
    let prefix = argument_string(args, 3, context)?;
    let local_name = argument_string(args, 4, context)?;
    let value = argument_string(args, 5, context)?;
    let node = state.node(argument_id(args, 1));
    let changed = node.as_ref().is_some_and(|node| {
        if replace {
            node.replace_attr_ns(optional(&namespace), optional(&prefix), &local_name, &value)
        } else {
            node.set_attr_ns(optional(&namespace), optional(&prefix), &local_name, &value)
        }
    });
    record_attribute_mutation(
        state,
        node.as_ref(),
        &local_name,
        changed,
        namespace.is_empty(),
    );
    Ok(JsValue::from(changed))
}

pub(super) fn remove_attribute(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<JsValue> {
    let name = argument_string(args, 2, context)?;
    let node = state.node(argument_id(args, 1));
    let changed = node
        .as_ref()
        .is_some_and(|node| node.remove_attr_qualified(&name));
    record_attribute_mutation(state, node.as_ref(), &name, changed, false);
    Ok(JsValue::from(changed))
}

pub(super) fn remove_attribute_ns(
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<JsValue> {
    let namespace = argument_string(args, 2, context)?;
    let local_name = argument_string(args, 3, context)?;
    let node = state.node(argument_id(args, 1));
    let changed = node
        .as_ref()
        .is_some_and(|node| node.remove_attr_ns(optional(&namespace), &local_name));
    record_attribute_mutation(state, node.as_ref(), &local_name, changed, false);
    Ok(JsValue::from(changed))
}

fn attribute_records(state: &HostState, id: u32) -> String {
    let records = state
        .node(id)
        .map(|node| {
            node.attributes()
                .iter()
                .map(|attribute| {
                    let namespace = attribute.name.ns.as_ref();
                    serde_json::json!({
                        "namespace": (!namespace.is_empty()).then_some(namespace),
                        "prefix": attribute.name.prefix.as_ref().map(ToString::to_string),
                        "localName": attribute.name.local.as_ref(),
                        "qualifiedName": attribute_qualified_name(attribute),
                        "value": attribute.value.as_ref(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::to_string(&records).unwrap_or_else(|_| "[]".to_string())
}

fn attribute_qualified_name(attribute: &html5ever::Attribute) -> String {
    attribute.name.prefix.as_ref().map_or_else(
        || attribute.name.local.to_string(),
        |prefix| format!("{prefix}:{}", attribute.name.local),
    )
}

fn optional(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn record_attribute_mutation(
    state: &mut HostState,
    node: Option<&NodeRef>,
    name: &str,
    changed: bool,
    queue_dynamic_script: bool,
) {
    if !changed {
        return;
    }
    state.record_mutation(node, MutationKind::Attribute(name));
    if let Some(node) = node {
        state.diagnose(format!("mutate {name} on {}", node_label(node)));
        if queue_dynamic_script && name.eq_ignore_ascii_case("src") {
            state.queue_dynamic_script(node);
        }
    }
}
