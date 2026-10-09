//! One owned packet per bounded group, not one V8 callback per numeric setter.
use super::*;
use crate::engine::webgl::numeric_packet::{MAX_PACKET_VALUES, decode};

fn invalid() -> JsError {
    JsNativeError::typ()
        .with_message("Invalid bounded WebGL numeric packet")
        .into()
}

pub(super) fn dispatch(
    scope: &mut v8::PinScope,
    arguments: v8::FunctionCallbackArguments,
    mut output: v8::ReturnValue,
) {
    let Some(bridge) = scope.get_current_context().get_slot::<HostBridge>() else {
        throw_error(scope, inactive_host());
        return;
    };
    let started = bridge.profile_start();
    let result = prepare(scope, &arguments).and_then(|commands| {
        let run = |contexts: &mut crate::engine::webgl::Contexts| {
            let mut lost = std::collections::BTreeSet::new();
            for (id, command) in commands {
                if !contexts.execute_numeric(id, command) {
                    lost.insert(id);
                }
            }
            lost
        };
        match &*bridge {
            HostBridge::Document(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                let mut host = host.borrow_mut();
                let native_started = host.host_call_profile.start();
                let result = run(&mut host.webgl);
                host.host_call_profile
                    .record("webglCommandPacket", native_started);
                Ok(result)
            }
            HostBridge::Worker(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                Ok(run(&mut host.borrow_mut().webgl))
            }
        }
    });
    bridge.profile_bridge("webglCommandPacket", started);
    match result {
        Ok(lost) => {
            // Initialize own elements directly, not [[Set]] through a possibly
            // author-modified Array prototype. Native IDs stay in the closure.
            let values = lost
                .into_iter()
                .map(|id| v8::Integer::new_from_unsigned(scope, id).into())
                .collect::<Vec<_>>();
            let result = v8::Array::new_with_elements(scope, &values);
            output.set(result.into());
        }
        Err(error) => throw_error(scope, error),
    }
}

fn prepare(
    scope: &mut v8::PinScope,
    args: &v8::FunctionCallbackArguments,
) -> JsResult<Vec<(u32, crate::engine::webgl::NumericCommand)>> {
    if args.length() != 3 || args.get(1).is_proxy() || !args.get(2).is_number() {
        return Err(invalid());
    }
    let view = v8::Local::<v8::Float64Array>::try_from(args.get(1)).map_err(|_| invalid())?;
    let used = args.get(2).number_value(scope).ok_or_else(invalid)?;
    if !used.is_finite() || used < 1. || used > MAX_PACKET_VALUES as f64 || used.fract() != 0. {
        return Err(invalid());
    }
    let count = used as usize;
    let buffer = view.buffer(scope).ok_or_else(invalid)?;
    if buffer.is_shared_array_buffer()
        || buffer.was_detached()
        || buffer.get_backing_store().is_resizable_by_user_javascript()
        || view.length() < count
        || view.length() > MAX_PACKET_VALUES
    {
        return Err(invalid());
    }
    // No JS property reads/coercion or HostState borrow spans this copy.
    // Respect the view's own offset, and ignore its unused fixed-capacity tail.
    let mut bytes = vec![0; count * 8];
    if view.copy_contents(&mut bytes) != bytes.len() {
        return Err(invalid());
    }
    let values = bytes
        .chunks_exact(8)
        .map(|value| f64::from_ne_bytes(value.try_into().unwrap()))
        .collect::<Vec<_>>();
    decode(&values).ok_or_else(invalid)
}
