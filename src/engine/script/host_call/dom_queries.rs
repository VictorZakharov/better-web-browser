//! DOM traversal, selector, focus, and geometry host operations.

use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "parent" => {
            let parent = state
                .node(argument_id(args, 1))
                .and_then(|node| node.parent());
            JsValue::from(parent.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "firstChild" => {
            let child = state
                .node(argument_id(args, 1))
                .and_then(|node| node.children.borrow().first().cloned());
            JsValue::from(child.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "lastChild" => {
            let child = state
                .node(argument_id(args, 1))
                .and_then(|node| node.children.borrow().last().cloned());
            JsValue::from(child.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "nextSibling" => JsValue::from(sibling_id(state, args, true)),
        "previousSibling" => JsValue::from(sibling_id(state, args, false)),
        "children" => {
            let children = state
                .node(argument_id(args, 1))
                .map(|node| node.children.borrow().clone())
                .unwrap_or_default();
            js_string(join_node_ids(state, &children, false))
        }
        "inclusiveAncestors" => {
            let mut nodes = Vec::new();
            let mut current = state.node(argument_id(args, 1));
            while let Some(node) = current {
                current = node.parent();
                nodes.push(node);
            }
            js_string(join_node_ids(state, &nodes, false))
        }
        "elementChildren" => {
            let children = state
                .node(argument_id(args, 1))
                .map(|node| node.children.borrow().clone())
                .unwrap_or_default();
            js_string(join_node_ids(state, &children, true))
        }
        "textGet" => {
            let value = state
                .node(argument_id(args, 1))
                .map(|node| node.text_content())
                .unwrap_or_default();
            js_string(value)
        }
        "query" => {
            let selector = argument_string(args, 2)?;
            let node = state
                .node(argument_id(args, 1))
                .and_then(|root| query_selector(&root, &selector));
            JsValue::from(node.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "queryAll" => {
            let selector = argument_string(args, 2)?;
            let nodes = state
                .node(argument_id(args, 1))
                .map(|root| query_selector_all(&root, &selector))
                .unwrap_or_default();
            js_string(join_node_ids(state, &nodes, false))
        }
        "matches" => {
            let selector = argument_string(args, 2)?;
            let matches = state
                .node(argument_id(args, 1))
                .is_some_and(|node| matches_selector_list(&node, &selector));
            JsValue::from(matches)
        }
        "setFocus" => {
            let next = state
                .node(argument_id(args, 1))
                .filter(|node| node.element().is_some());
            state.set_focus_target(next);
            JsValue::undefined()
        }
        "closest" => {
            let selector = argument_string(args, 2)?;
            let closest = state
                .node(argument_id(args, 1))
                .and_then(|node| closest_matching_element(&node, &selector));
            JsValue::from(closest.map(|node| state.id_for(&node)).unwrap_or_default())
        }
        "layoutRect" => {
            state.flush_layout_if_needed();
            let rect = state
                .node(argument_id(args, 1))
                .and_then(|node| state.layout_geometry.get(&node.id()).copied());
            rect.map_or_else(JsValue::null, |rect| {
                JsValue::Array(vec![
                    JsValue::from(rect.x as f64),
                    JsValue::from(rect.y as f64),
                    JsValue::from(rect.width as f64),
                    JsValue::from(rect.height as f64),
                ])
            })
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
