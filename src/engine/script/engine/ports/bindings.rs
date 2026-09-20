//! Trusted MessagePort facade installation and native endpoint operations.
use super::*;
pub(in crate::engine::script::engine) fn install(scope: &mut v8::PinScope) -> Option<()> {
    let context = scope.get_current_context();
    let tree = frames::tree(context)?;
    let document = node_wrappers::host(context)?.borrow().document.id();
    let global = context.global(scope);
    let key = v8::String::new(scope, "__nativePortBindings")?;
    let value = global.get(scope, key.into())?;
    let array = v8::Local::<v8::Array>::try_from(value).ok()?;
    let configure = v8::Local::<v8::Function>::try_from(array.get_index(scope, 0)?).ok()?;
    let create = v8::Local::<v8::Function>::try_from(array.get_index(scope, 1)?).ok()?;
    let deliver = v8::Local::<v8::Function>::try_from(array.get_index(scope, 2)?).ok()?;
    if global.delete(scope, key.into()) != Some(true) {
        return None;
    }
    tree.ports.bindings.borrow_mut().insert(
        document,
        Bindings {
            create: v8::Global::new(scope, create),
            deliver: v8::Global::new(scope, deliver),
        },
    );
    let methods = v8::Array::new(scope, 4);
    for index in 0..4 {
        let data = v8::Integer::new(scope, index as i32);
        let method = v8::Function::builder(dispatch)
            .data(data.into())
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope)?;
        methods.set_index(scope, index, method.into())?;
    }
    let receiver = v8::undefined(scope).into();
    configure.call(scope, receiver, &[methods.into()])?;
    Some(())
}
fn dispatch(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    let Some(tree) = frames::tree(scope.get_current_context()) else {
        return;
    };
    let ports = &tree.ports;
    let operation = args.data().int32_value(scope).unwrap_or(-1);
    if operation == 0 {
        ports.collect_retired();
        if ports.endpoints.borrow().len() > 4094 {
            throw_named(
                scope,
                "QuotaExceededError",
                "The document tree's MessagePort limit was reached",
            );
            return;
        }
        let Some(first) = ports.next.get().checked_add(1) else {
            return;
        };
        let Some(second) = first.checked_add(1) else {
            return;
        };
        ports.next.set(second);
        for (id, peer) in [(first, second), (second, first)] {
            ports.endpoints.borrow_mut().insert(
                id,
                Endpoint {
                    owner: None,
                    peer: Some(peer),
                    enabled: false,
                    closed: false,
                    close_event: false,
                    queue: VecDeque::new(),
                },
            );
        }
        let Some(a) = ports.receive(scope, first) else {
            return;
        };
        let Some(b) = ports.receive(scope, second) else {
            return;
        };
        result.set(v8::Array::new_with_elements(scope, &[a.into(), b.into()]).into());
        return;
    }
    let Ok(object) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        invalid(scope);
        return;
    };
    let Some(id) = id(scope, object) else {
        invalid(scope);
        return;
    };
    if id == 0 {
        return;
    }
    if ports.valid(scope, object) != Some(id) {
        invalid(scope);
        return;
    }
    if operation == 3 {
        ports.close(id);
        return;
    }
    if operation == 2 {
        if let Some(endpoint) = ports.endpoints.borrow_mut().get_mut(&id) {
            endpoint.enabled = true;
        }
        return;
    }
    let transfer_value = v8::Local::new(scope, args.get(2));
    let Ok(transfers) = v8::Local::<v8::Array>::try_from(transfer_value) else {
        return;
    };
    for index in 0..transfers.length() {
        if transfers.get_index(scope, index) == Some(object.into()) {
            throw_named(
                scope,
                "DataCloneError",
                "A MessagePort cannot transfer itself through its own endpoint",
            );
            return;
        }
    }
    let value = v8::Local::new(scope, args.get(1));
    let Some(mut data) = Serialized::write(scope, value, transfers) else {
        return;
    };
    // Getters during serialization can close, transfer or post recursively. Recheck all
    // state and quotas before committing any of this operation's transfers.
    let peer = ports
        .endpoints
        .borrow()
        .get(&id)
        .and_then(|endpoint| endpoint.peer);
    let total = ports.bytes.get().saturating_add(data.size());
    if peer.is_some_and(|peer| {
        ports
            .endpoints
            .borrow()
            .get(&peer)
            .is_some_and(|p| p.queue.len() >= 256)
    }) || total > 32 * 1024 * 1024
    {
        throw_named(
            scope,
            "QuotaExceededError",
            "The MessagePort queue exceeds its limit",
        );
        return;
    }
    if !data.commit_transfers(scope) {
        return;
    }
    if let Some(peer) = peer.and_then(|peer| {
        ports.endpoints.borrow_mut().get_mut(&peer).map(|p| {
            p.queue.push_back(data);
            peer
        })
    }) {
        let _ = peer;
        ports.bytes.set(total);
    }
}
fn invalid(scope: &mut v8::PinScope) {
    let text = v8::String::new(scope, "Invalid MessagePort receiver").unwrap();
    let error = v8::Exception::type_error(scope, text);
    scope.throw_exception(error);
}
