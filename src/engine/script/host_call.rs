//! Native operations exposed to the JavaScript bootstrap through `__hostCall`.

use super::binding_helpers::*;
use super::*;

pub(super) fn host_call(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let operation = argument_string(args, 0, context)?;
    let host = context
        .get_data::<HostStateLink>()
        .and_then(|link| link.0.upgrade())
        .ok_or_else(|| JsNativeError::typ().with_message("browser host is not active"))?;
    let mut host = host.borrow_mut();
    let state = &mut *host;

    if let Some(value) = super::dom_host::dom_host_call(&operation, args, context, state)? {
        return Ok(value);
    }
    if let Some(value) = super::style_host::style_host_call(&operation, args, context, state)? {
        return Ok(value);
    }

    match operation.as_str() {
        "parent" => {
            let parent = state
                .node(argument_id(args, 1))
                .and_then(|node| node.parent());
            Ok(JsValue::from(
                parent.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "firstChild" => {
            let child = state
                .node(argument_id(args, 1))
                .and_then(|node| node.children.borrow().first().cloned());
            Ok(JsValue::from(
                child.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "lastChild" => {
            let child = state
                .node(argument_id(args, 1))
                .and_then(|node| node.children.borrow().last().cloned());
            Ok(JsValue::from(
                child.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "nextSibling" => Ok(JsValue::from(sibling_id(state, args, true))),
        "previousSibling" => Ok(JsValue::from(sibling_id(state, args, false))),
        "children" => {
            let children = state
                .node(argument_id(args, 1))
                .map(|node| node.children.borrow().clone())
                .unwrap_or_default();
            Ok(js_string(join_node_ids(state, &children, false)))
        }
        "elementChildren" => {
            let children = state
                .node(argument_id(args, 1))
                .map(|node| node.children.borrow().clone())
                .unwrap_or_default();
            Ok(js_string(join_node_ids(state, &children, true)))
        }
        "appendChild" => {
            let parent = state.node(argument_id(args, 1));
            let child = state.node(argument_id(args, 2));
            let changed = parent
                .zip(child.clone())
                .is_some_and(|(parent, child)| Node::append_child(&parent, child));
            if changed {
                state.record_mutation();
                if let (Some(parent), Some(child)) = (
                    state.node(argument_id(args, 1)),
                    state.node(argument_id(args, 2)),
                ) {
                    state.diagnose(format!(
                        "append {} to {}",
                        node_label(&child),
                        node_label(&parent)
                    ));
                    state.queue_dynamic_script(&child);
                }
            }
            Ok(JsValue::from(if changed {
                child.map(|node| state.id_for(&node)).unwrap_or_default()
            } else {
                0
            }))
        }
        "insertBefore" => {
            let parent = state.node(argument_id(args, 1));
            let child = state.node(argument_id(args, 2));
            let reference_id = argument_id(args, 3);
            let changed = if reference_id == 0 {
                parent
                    .zip(child.clone())
                    .is_some_and(|(parent, child)| Node::append_child(&parent, child))
            } else {
                let reference = state.node(reference_id);
                parent.zip(child.clone()).zip(reference).is_some_and(
                    |((parent, child), reference)| Node::insert_before(&parent, child, &reference),
                )
            };
            if changed {
                state.record_mutation();
                state.diagnose("insert node before sibling".into());
                if let Some(child) = child.as_ref() {
                    state.queue_dynamic_script(child);
                }
            }
            Ok(JsValue::from(if changed {
                child.map(|node| state.id_for(&node)).unwrap_or_default()
            } else {
                0
            }))
        }
        "removeChild" => {
            let parent = state.node(argument_id(args, 1));
            let child = state.node(argument_id(args, 2));
            let changed = parent
                .zip(child)
                .is_some_and(|(parent, child)| Node::remove_child(&parent, &child));
            if changed {
                state.record_mutation();
                state.diagnose("remove child node".into());
            }
            Ok(JsValue::from(changed))
        }
        "remove" => {
            let node = state.node(argument_id(args, 1));
            let changed = node.as_ref().is_some_and(|node| node.parent().is_some());
            if let Some(node) = node {
                Node::remove_from_parent(&node);
            }
            if changed {
                state.record_mutation();
                state.diagnose("remove node".into());
            }
            Ok(JsValue::from(changed))
        }
        "textGet" => {
            let value = state
                .node(argument_id(args, 1))
                .map(|node| node.text_content())
                .unwrap_or_default();
            Ok(js_string(value))
        }
        "textSet" => {
            let contents = argument_string(args, 2, context)?;
            let changed = if let Some(node) = state.node(argument_id(args, 1)) {
                Node::set_text_content(&node, &contents);
                state.register_subtree(&node);
                true
            } else {
                false
            };
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("set textContent on {}", node_label(&node)));
                }
            }
            Ok(JsValue::from(changed))
        }
        "attrGet" => {
            let name = argument_string(args, 2, context)?;
            let value = state
                .node(argument_id(args, 1))
                .and_then(|node| node.attr(&name));
            Ok(value.map_or_else(JsValue::null, js_string))
        }
        "attrSet" => {
            let name = argument_string(args, 2, context)?;
            let value = argument_string(args, 3, context)?;
            let changed = state
                .node(argument_id(args, 1))
                .is_some_and(|node| node.set_attr(&name, &value));
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("set {} on {}", name, node_label(&node)));
                    if name.eq_ignore_ascii_case("src") {
                        state.queue_dynamic_script(&node);
                    }
                }
            }
            Ok(JsValue::from(changed))
        }
        "attrRemove" => {
            let name = argument_string(args, 2, context)?;
            let changed = state
                .node(argument_id(args, 1))
                .is_some_and(|node| node.remove_attr(&name));
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("remove {} from {}", name, node_label(&node)));
                }
            }
            Ok(JsValue::from(changed))
        }
        "attrHas" => {
            let name = argument_string(args, 2, context)?;
            let present = state
                .node(argument_id(args, 1))
                .is_some_and(|node| node.attr(&name).is_some());
            Ok(JsValue::from(present))
        }
        "attrNames" => {
            let names = state
                .node(argument_id(args, 1))
                .and_then(|node| {
                    node.element().map(|element| {
                        element
                            .attrs
                            .borrow()
                            .iter()
                            .map(|attribute| attribute.name.local.to_string())
                            .collect::<Vec<_>>()
                            .join("\u{1f}")
                    })
                })
                .unwrap_or_default();
            Ok(js_string(names))
        }
        "innerHtmlGet" => {
            let value = state
                .node(argument_id(args, 1))
                .map(|node| serialize_children(&node))
                .unwrap_or_default();
            Ok(js_string(value))
        }
        "innerHtmlSet" => {
            let html = argument_string(args, 2, context)?;
            let changed = if let Some(node) = state.node(argument_id(args, 1)) {
                Node::replace_inner_html(&node, &html, true);
                state.register_subtree(&node);
                true
            } else {
                false
            };
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("replace innerHTML of {}", node_label(&node)));
                }
            }
            Ok(JsValue::from(changed))
        }
        "innerHtmlAppend" => {
            let html = argument_string(args, 2, context)?;
            let changed = if let Some(node) = state.node(argument_id(args, 1)) {
                append_html_fragment(&state.document, &node, &html);
                state.register_subtree(&node);
                true
            } else {
                false
            };
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("append innerHTML to {}", node_label(&node)));
                }
            }
            Ok(JsValue::from(changed))
        }
        "query" => {
            let selector = argument_string(args, 2, context)?;
            let node = state
                .node(argument_id(args, 1))
                .and_then(|root| query_selector_all(&root, &selector).into_iter().next());
            Ok(JsValue::from(
                node.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "queryAll" => {
            let selector = argument_string(args, 2, context)?;
            let nodes = state
                .node(argument_id(args, 1))
                .map(|root| query_selector_all(&root, &selector))
                .unwrap_or_default();
            Ok(js_string(join_node_ids(state, &nodes, false)))
        }
        "documentUrl" => Ok(js_string(state.document_url.clone())),
        "cookieGet" => Ok(js_string(state.cookie_header())),
        "cookieSet" => {
            state.set_cookie(argument_string(args, 1, context)?);
            Ok(JsValue::undefined())
        }
        "userAgent" => Ok(js_string(crate::branding::USER_AGENT.to_string())),
        "resolveUrl" => {
            let value = argument_string(args, 1, context)?;
            Ok(js_string(state.resolved_url(&value)))
        }
        "navigate" => {
            let value = argument_string(args, 1, context)?;
            let resolved = state.resolved_url(&value);
            state.navigation_url = Some(resolved.clone());
            Ok(js_string(resolved))
        }
        "timerSchedule" => {
            let id = argument_id(args, 1);
            if id == 0 {
                return Err(JsNativeError::range()
                    .with_message("timer identifiers must be positive integers")
                    .into());
            }
            let delay = argument_duration(args, 2);
            let repeat = args.get(3).and_then(JsValue::as_boolean).unwrap_or(false);
            state.schedule_timer(id, delay, repeat);
            Ok(JsValue::from(id))
        }
        "timerCancel" => {
            let cancelled = state.cancel_timer(argument_id(args, 1));
            Ok(JsValue::from(cancelled))
        }
        "console" => {
            let level = argument_string(args, 1, context)?;
            let message = argument_string(args, 2, context)?;
            state.console.push(format!("{level}: {message}"));
            Ok(JsValue::undefined())
        }
        _ => Err(JsNativeError::typ()
            .with_message(format!("unsupported browser host operation: {operation}"))
            .into()),
    }
}
