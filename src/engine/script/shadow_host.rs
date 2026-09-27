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
        "getHTML" => super::get_html_host::get_html_host_call(args, state)?,
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
        // Internal traversal needs closed roots too; Element.shadowRoot above still filters them.
        "shadowRootForTraversal" => {
            let root = state
                .node(argument_id(args, 1))
                .and_then(|host| host.shadow_root());
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
        "shadowSlotAssignment" => js_string(
            state
                .node(argument_id(args, 1))
                .and_then(|root| match root.data {
                    NodeData::ShadowRoot(ref shadow) => Some(if shadow.manual_slot_assignment {
                        "manual"
                    } else {
                        "named"
                    }),
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
        "shadowDeclarative" => JsValue::from(state.node(argument_id(args, 1)).is_some_and(
            |root| matches!(root.data, NodeData::ShadowRoot(ref shadow) if shadow.declarative.get()),
        )),
        "shadowRegistryIsGlobal" => JsValue::from(state.node(argument_id(args, 1)).is_some_and(
            |root| matches!(root.data, NodeData::ShadowRoot(ref shadow) if shadow.registry_is_global.get()),
        )),
        "shadowRegistryIsNull" => JsValue::from(state.node(argument_id(args, 1)).is_some_and(
            |root| matches!(root.data, NodeData::ShadowRoot(ref shadow) if shadow.registry_is_null.get()),
        )),
        "shadowKeepRegistryNull" => JsValue::from(state.node(argument_id(args, 1)).is_some_and(
            |root| matches!(root.data, NodeData::ShadowRoot(ref shadow) if shadow.keep_registry_null.get()),
        )),
        "shadowRegistryInitialize" => JsValue::from(
            state
                .node(argument_id(args, 1))
                .and_then(|root| match &root.data {
                    NodeData::ShadowRoot(shadow) if shadow.registry_is_null.get() => {
                        shadow.registry_is_null.set(false);
                        shadow.registry_is_global.set(
                            args.get(2).and_then(JsValue::as_boolean).unwrap_or(false),
                        );
                        Some(true)
                    }
                    _ => None,
                })
                .unwrap_or(false),
        ),
        "shadowRegistryAdopt" => {
            let is_global = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
            let is_null = args.get(3).and_then(JsValue::as_boolean).unwrap_or(false);
            if !(is_global && is_null)
                && let Some(root) = state.node(argument_id(args, 1))
                && let NodeData::ShadowRoot(shadow) = &root.data
            {
                shadow.registry_is_global.set(is_global);
                shadow.registry_is_null.set(is_null);
                JsValue::from(true)
            } else {
                JsValue::from(false)
            }
        }
        "documentRegistryInitializeScoped" => {
            let document = state.node(argument_id(args, 1));
            let eligible = document.as_ref().is_some_and(|document| {
                matches!(document.data, NodeData::Document)
                    && document.id() != state.document.id()
                    && state
                        .document_for(document)
                        .is_some_and(|owner| owner.id() == document.id())
            });
            if let Some(document) = document.filter(|_| eligible) {
                state
                    .documents
                    .borrow_mut()
                    .scoped_custom_element_documents
                    .insert(document.id().document());
            }
            JsValue::from(eligible)
        }
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
        // Focus/inertness follows the flat tree even through a closed root. The
        // public Node.assignedSlot operation above must still hide that root.
        "assignedSlotForTraversal" => {
            let slot = state
                .node(argument_id(args, 1))
                .and_then(|node| Node::assigned_slot(&node));
            JsValue::from(slot.map(|slot| state.id_for(&slot)).unwrap_or_default())
        }
        "isInComposedTree" => JsValue::from(
            state
                .node(argument_id(args, 1))
                .is_some_and(|node| Node::is_in_composed_tree(&node)),
        ),
        "assignedNodes" => {
            let flatten = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
            let nodes = state
                .node(argument_id(args, 1))
                .map(|slot| Node::assigned_nodes(&slot, flatten))
                .unwrap_or_default();
            js_string(join_node_ids(state, &nodes, false))
        }
        "assignSlot" => assign_slot(args, state)?,
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
    let creating_new_root = host.shadow_root().is_none();
    if creating_new_root {
        state.ensure_node_capacity(1)?;
    }
    let delegates_focus = args.get(3).and_then(JsValue::as_boolean).unwrap_or(false);
    let serializable = args.get(4).and_then(JsValue::as_boolean).unwrap_or(false);
    let clonable = args.get(5).and_then(JsValue::as_boolean).unwrap_or(false);
    let manual = matches!(args.get(6), Some(JsValue::String(assignment)) if assignment == "manual");
    let registry_is_global = args.get(7).and_then(JsValue::as_boolean).unwrap_or(true);
    let registry_is_null =
        !registry_is_global && args.get(8).and_then(JsValue::as_boolean).unwrap_or(false);
    let Some(root) = Node::attach_shadow_with_assignment(
        &host,
        mode,
        delegates_focus,
        serializable,
        clonable,
        manual,
    ) else {
        return Ok(JsValue::from(0));
    };
    if let NodeData::ShadowRoot(shadow) = &root.data
        && creating_new_root
    {
        shadow.registry_is_global.set(registry_is_global);
        shadow.registry_is_null.set(registry_is_null);
    }
    state.register_subtree(&root);
    state.record_mutation(Some(&host), MutationKind::Stylesheet);
    state.diagnose(format!(
        "attach {mode:?} shadow root to {}",
        host.id().to_wire()
    ));
    Ok(JsValue::from(state.id_for(&root)))
}

fn assign_slot(args: &[JsValue], state: &mut HostState) -> JsResult<JsValue> {
    let Some(slot) = state.node(argument_id(args, 1)) else {
        return Ok(JsValue::undefined());
    };
    if slot.tag_name() != Some("slot") {
        return Ok(JsValue::undefined());
    }
    let mut nodes = Vec::new();
    for index in 2..args.len() {
        let Some(node) = state.node(argument_id(args, index)) else {
            return Err(JsNativeError::typ()
                .with_message("slot.assign requires Element or Text nodes")
                .into());
        };
        if !matches!(node.data, NodeData::Element(_) | NodeData::Text(_)) {
            return Err(JsNativeError::typ()
                .with_message("slot.assign requires Element or Text nodes")
                .into());
        }
        nodes.push(node);
    }
    let affected = Node::assign_manual_nodes(&slot, &nodes);
    for changed in &affected {
        state.record_mutation(Some(changed), MutationKind::Stylesheet);
    }
    Ok(js_string(join_node_ids(state, &affected, false)))
}
