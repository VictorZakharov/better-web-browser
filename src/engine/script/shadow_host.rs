//! Shadow-root identity and slot-distribution operations exposed to the JavaScript realm.

use super::binding_helpers::{argument_id, argument_string, join_node_ids, js_string};
use super::*;
use crate::engine::dom::ShadowRootMode;

pub(super) fn shadow_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "attachShadow" => attach_shadow(args, state)?,
        "shadowRoot" => {
            let root = state
                .node(argument_id(args, 1))
                .and_then(|host| host.shadow_root())
                .filter(|root| {
                    matches!(
                        root.data,
                        NodeData::ShadowRoot(ref shadow) if shadow.mode == ShadowRootMode::Open
                    )
                });
            JsValue::from(root.map(|root| state.id_for(&root)).unwrap_or_default())
        }
        "shadowHost" => {
            let host = state
                .node(argument_id(args, 1))
                .and_then(|root| root.shadow_host());
            JsValue::from(host.map(|host| state.id_for(&host)).unwrap_or_default())
        }
        "shadowMode" => js_string(
            state
                .node(argument_id(args, 1))
                .and_then(|root| match root.data {
                    NodeData::ShadowRoot(ref shadow) => Some(shadow.mode.as_str()),
                    _ => None,
                })
                .unwrap_or_default()
                .to_string(),
        ),
        "shadowDelegatesFocus" => JsValue::from(state.node(argument_id(args, 1)).is_some_and(
            |root| matches!(root.data, NodeData::ShadowRoot(ref shadow) if shadow.delegates_focus),
        )),
        "shadowSerializable" => JsValue::from(state.node(argument_id(args, 1)).is_some_and(
            |root| matches!(root.data, NodeData::ShadowRoot(ref shadow) if shadow.serializable),
        )),
        "shadowClonable" => JsValue::from(state.node(argument_id(args, 1)).is_some_and(
            |root| matches!(root.data, NodeData::ShadowRoot(ref shadow) if shadow.clonable),
        )),
        "rootNode" => {
            let composed = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
            let root = state.node(argument_id(args, 1)).map(|node| {
                if composed {
                    Node::shadow_including_root(&node)
                } else {
                    Node::tree_root(&node)
                }
            });
            JsValue::from(root.map(|root| state.id_for(&root)).unwrap_or_default())
        }
        "assignedSlot" => {
            let slot = state
                .node(argument_id(args, 1))
                .and_then(|node| Node::assigned_slot(&node))
                .filter(|slot| {
                    matches!(
                        Node::tree_root(slot).data,
                        NodeData::ShadowRoot(ref shadow) if shadow.mode == ShadowRootMode::Open
                    )
                });
            JsValue::from(slot.map(|slot| state.id_for(&slot)).unwrap_or_default())
        }
        "assignedNodes" => {
            let flatten = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
            let nodes = state
                .node(argument_id(args, 1))
                .map(|slot| Node::assigned_nodes(&slot, flatten))
                .unwrap_or_default();
            js_string(join_node_ids(state, &nodes, false))
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn attach_shadow(args: &[JsValue], state: &mut HostState) -> JsResult<JsValue> {
    let Some(host) = state.node(argument_id(args, 1)) else {
        return Ok(JsValue::from(0));
    };
    let mode = match argument_string(args, 2)?.as_str() {
        "open" => ShadowRootMode::Open,
        "closed" => ShadowRootMode::Closed,
        _ => return Ok(JsValue::from(0)),
    };
    state.ensure_node_capacity(1)?;
    let delegates_focus = args.get(3).and_then(JsValue::as_boolean).unwrap_or(false);
    let serializable = args.get(4).and_then(JsValue::as_boolean).unwrap_or(false);
    let clonable = args.get(5).and_then(JsValue::as_boolean).unwrap_or(false);
    let Some(root) = Node::attach_shadow(&host, mode, delegates_focus, serializable, clonable)
    else {
        return Ok(JsValue::from(0));
    };
    state.register_subtree(&root);
    state.record_mutation(Some(&host), MutationKind::Stylesheet);
    state.diagnose(format!(
        "attach {mode:?} shadow root to {}",
        host.id().to_wire()
    ));
    Ok(JsValue::from(state.id_for(&root)))
}
