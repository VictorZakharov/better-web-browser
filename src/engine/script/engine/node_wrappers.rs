//! Native node brands and cross-realm handles. Numeric DOM IDs are local to a HostState.
use super::bridge::HostBridge;
use crate::engine::dom::NodeId;
use crate::engine::script::host_state::HostState;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Default)]
pub(super) struct Wrappers(RefCell<HashMap<NodeId, v8::Weak<v8::Object>>>);

impl Wrappers {
    pub(super) fn get<'s>(
        &self,
        scope: &v8::PinScope<'s, '_>,
        id: NodeId,
    ) -> Option<v8::Local<'s, v8::Object>> {
        self.0.borrow().get(&id)?.to_local(scope)
    }
}

pub(super) fn host(context: v8::Local<v8::Context>) -> Option<Rc<RefCell<HostState>>> {
    let bridge = context.get_slot::<HostBridge>()?;
    match &*bridge {
        HostBridge::Document(host) => host.upgrade(),
        _ => None,
    }
}

pub(super) fn dispatch(
    scope: &mut v8::PinScope,
    operation: &str,
    arguments: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    result.set(v8::null(scope).into());
    let context = scope.get_current_context();
    let Some(host) = host(context) else { return };
    let Some(wrappers) = context.get_slot::<Wrappers>() else {
        return;
    };
    if operation == "nodeWrapper" {
        let Some(id) = arguments.get(1).uint32_value(scope) else {
            return;
        };
        let Some(node) = host.borrow().node(id) else {
            return;
        };
        if let Some(wrapper) = wrappers
            .0
            .borrow()
            .get(&node.id())
            .and_then(|w| w.to_local(scope))
        {
            result.set(wrapper.into());
        }
        return;
    }
    let Ok(object) = v8::Local::<v8::Object>::try_from(arguments.get(1)) else {
        return;
    };
    let Some(origin) = object.get_creation_context(scope) else {
        return;
    };
    // Web IDL interface conversion accepts genuine platform objects, not instanceof checks
    // or author-controlled __id properties. Never import a foreign-origin wrapper.
    if origin.get_security_token(scope) != context.get_security_token(scope) {
        return;
    }
    let Some(name) = v8::String::new(scope, "Breeze.Node.handle") else {
        return;
    };
    let brand = v8::Private::for_api(scope, Some(name));
    if operation == "bindNodeWrapper" {
        if origin != context {
            return;
        }
        let Some(id) = arguments.get(2).uint32_value(scope) else {
            return;
        };
        let Some(node) = host.borrow().node(id) else {
            return;
        };
        // A second wrapper cannot replace the identity of a still-live platform object.
        if wrappers
            .0
            .borrow()
            .get(&node.id())
            .is_some_and(|w| w.to_local(scope).is_some())
        {
            return;
        }
        let value = v8::Integer::new_from_unsigned(scope, id);
        if object.set_private(scope, brand, value.into()) == Some(true) {
            wrappers
                .0
                .borrow_mut()
                .insert(node.id(), v8::Weak::new(scope, object));
            result.set(value.into());
        }
        return;
    }
    let Some(id) = object
        .get_private(scope, brand)
        .filter(|v| v.is_uint32())
        .and_then(|v| v.uint32_value(scope))
    else {
        return;
    };
    let Some(source) = self::host(origin) else {
        return;
    };
    let Some(node) = source.borrow().node(id) else {
        return;
    };
    // Private properties are not inherited, but also check the canonical wrapper identity.
    if !wrappers
        .0
        .borrow()
        .get(&node.id())
        .and_then(|w| w.to_local(scope))
        .is_some_and(|wrapper| wrapper == object)
    {
        return;
    }
    let id = host.borrow_mut().id_for(&node);
    result.set(v8::Integer::new_from_unsigned(scope, id).into());
}
