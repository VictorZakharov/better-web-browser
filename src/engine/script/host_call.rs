//! Native operations exposed to the JavaScript bootstrap through `__hostCall`.

use super::binding_helpers::*;
use super::*;
use boa_engine::object::builtins::JsArrayBuffer;

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

    let started = state.host_call_profile.start();
    let result = dispatch_host_call(&operation, args, context, state);
    state.host_call_profile.record(&operation, started);
    result
}

fn dispatch_host_call(
    operation: &str,
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<JsValue> {
    if let Some(value) = super::network::network_host_call(operation, args, context, state)? {
        return Ok(value);
    }
    if let Some(value) = super::workers::worker_host_call(operation, args, context, state)? {
        return Ok(value);
    }
    if let Some(value) =
        super::attribute_host::attribute_host_call(operation, args, context, state)?
    {
        return Ok(value);
    }
    if let Some(value) = super::dom_host::dom_host_call(operation, args, context, state)? {
        return Ok(value);
    }
    if let Some(value) = super::shadow_host::shadow_host_call(operation, args, context, state)? {
        return Ok(value);
    }
    if let Some(value) = super::cssom_host::cssom_host_call(operation, args, context, state)? {
        return Ok(value);
    }
    if let Some(value) = super::style_host::style_host_call(operation, args, context, state)? {
        return Ok(value);
    }
    if let Some(value) = super::mutation_host::mutation_host_call(operation, args, context, state)?
    {
        return Ok(value);
    }
    if let Some(value) =
        super::text_encoding_host::text_encoding_host_call(operation, args, context)?
    {
        return Ok(value);
    }

    match operation {
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
        "textGet" => {
            let value = state
                .node(argument_id(args, 1))
                .map(|node| node.text_content())
                .unwrap_or_default();
            Ok(js_string(value))
        }
        "innerHtmlGet" => {
            let value = state
                .node(argument_id(args, 1))
                .map(|node| serialize_children(&node))
                .unwrap_or_default();
            Ok(js_string(value))
        }
        "query" => {
            let selector = argument_string(args, 2, context)?;
            let node = state
                .node(argument_id(args, 1))
                .and_then(|root| query_selector(&root, &selector));
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
        "matches" => {
            let selector = argument_string(args, 2, context)?;
            let matches = state
                .node(argument_id(args, 1))
                .is_some_and(|node| matches_selector_list(&node, &selector));
            Ok(JsValue::from(matches))
        }
        "closest" => {
            let selector = argument_string(args, 2, context)?;
            let closest = state
                .node(argument_id(args, 1))
                .and_then(|node| closest_matching_element(&node, &selector));
            Ok(JsValue::from(
                closest.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "documentUrl" => Ok(js_string(state.document_url.clone())),
        "mediaMatches" => {
            let query = argument_string(args, 1, context)?;
            Ok(JsValue::from(
                crate::engine::css::media_query_matches_with_color_scheme(
                    &query,
                    state.media_viewport_width,
                    state.prefers_dark_color_scheme,
                ),
            ))
        }
        "cookieGet" => Ok(js_string(state.cookie_header())),
        "cookieSet" => {
            state.set_cookie(argument_string(args, 1, context)?);
            Ok(JsValue::undefined())
        }
        "storageLength" => {
            let area = storage_area(args, 1, context)?;
            Ok(JsValue::from(state.storage_len(area) as u32))
        }
        "storageKey" => {
            let area = storage_area(args, 1, context)?;
            let index = argument_id(args, 2) as usize;
            Ok(state
                .storage_key(area, index)
                .map_or_else(JsValue::null, |value| js_string(value.to_string())))
        }
        "storageGet" => {
            let area = storage_area(args, 1, context)?;
            let key = argument_string(args, 2, context)?;
            Ok(state
                .storage_get(area, &key)
                .map_or_else(JsValue::null, |value| js_string(value.to_string())))
        }
        "storageSet" => {
            let area = storage_area(args, 1, context)?;
            let key = argument_string(args, 2, context)?;
            let value = argument_string(args, 3, context)?;
            state.storage_set(area, key, value).map_err(storage_error)?;
            Ok(JsValue::undefined())
        }
        "storageRemove" => {
            let area = storage_area(args, 1, context)?;
            let key = argument_string(args, 2, context)?;
            state.storage_remove(area, key).map_err(storage_error)?;
            Ok(JsValue::undefined())
        }
        "storageClear" => {
            let area = storage_area(args, 1, context)?;
            state.storage_clear(area).map_err(storage_error)?;
            Ok(JsValue::undefined())
        }
        "arrayBufferDetach" => {
            let object = args.get(1).and_then(JsValue::as_object).ok_or_else(|| {
                JsNativeError::typ().with_message("transfer value is not an ArrayBuffer")
            })?;
            JsArrayBuffer::from_object(object)?.detach(&JsValue::undefined())?;
            Ok(JsValue::undefined())
        }
        "userAgent" => Ok(js_string(crate::branding::USER_AGENT.to_string())),
        "resolveUrl" => {
            let value = argument_string(args, 1, context)?;
            Ok(js_string(state.resolved_url(&value)))
        }
        "strictResolveUrl" => {
            let value = argument_string(args, 1, context)?;
            let base = if args.len() > 2 {
                argument_string(args, 2, context)?
            } else {
                state.document_url.clone()
            };
            let resolved = crate::navigation::resolve_web_url(&base, &value).ok_or_else(|| {
                JsNativeError::typ().with_message(format!("Invalid URL: {value}"))
            })?;
            Ok(js_string(resolved))
        }
        "parseWebUrl" => {
            let value = argument_string(args, 1, context)?;
            let parts = crate::navigation::web_url_parts(&value).ok_or_else(|| {
                JsNativeError::typ().with_message(format!("Invalid URL: {value}"))
            })?;
            Ok(js_string(
                serde_json::to_string(&parts).expect("URL parts serialize"),
            ))
        }
        "setWebUrlComponent" => {
            let value = argument_string(args, 1, context)?;
            let component = argument_string(args, 2, context)?;
            let input = argument_string(args, 3, context)?;
            let resolved = crate::navigation::set_web_url_component(&value, &component, &input)
                .ok_or_else(|| {
                    JsNativeError::typ().with_message(format!("Invalid URL {component}: {input}"))
                })?;
            Ok(js_string(resolved))
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
        "idleSchedule" => {
            let id = argument_id(args, 1);
            if id == 0 {
                return Err(JsNativeError::range()
                    .with_message("idle callback identifiers must be positive integers")
                    .into());
            }
            state.schedule_idle_callback(id, argument_duration(args, 2));
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

fn storage_area(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<crate::storage::StorageAreaKind> {
    match argument_string(args, index, context)?.as_str() {
        "local" => Ok(crate::storage::StorageAreaKind::Local),
        "session" => Ok(crate::storage::StorageAreaKind::Session),
        _ => Err(JsNativeError::typ()
            .with_message("invalid Web Storage area")
            .into()),
    }
}

fn storage_error(error: crate::storage::StorageError) -> boa_engine::JsError {
    JsNativeError::error()
        .with_message(error.to_string())
        .into()
}
