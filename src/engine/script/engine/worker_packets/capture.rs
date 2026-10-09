//! Serializer capability is captured before author code and never read from globals later.
use super::*;

const SERIALIZER: &str = "breeze.worker.packet.serializer";
pub(in crate::engine::script::engine) fn install(scope: &mut v8::PinScope) -> Option<()> {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let name = v8::String::new(scope, "__serializeWorkerPacket")?;
    let function = global.get(scope, name.into())?;
    v8::Local::<v8::Function>::try_from(function).ok()?;
    let private_name = v8::String::new(scope, SERIALIZER)?;
    let key = v8::Private::for_api(scope, Some(private_name));
    if global.set_private(scope, key, function) != Some(true)
        || global.delete(scope, name.into()) != Some(true)
    {
        return None;
    }
    let decoder = v8::String::new(scope, "__deserializeWorkerPacketWithPorts")?;
    if global.delete(scope, decoder.into()) != Some(true) {
        return None;
    }
    Some(())
}

pub(in crate::engine::script::engine) fn serialize(
    scope: &mut v8::PinScope,
    value: v8::Local<v8::Value>,
    transfers: v8::Local<v8::Value>,
) -> Option<WorkerMessage> {
    let state = match state(scope) {
        Ok(state) => state,
        Err(message) => {
            super::super::messaging::throw_named(scope, "DataCloneError", message);
            return None;
        }
    };
    let guard = match state.write() {
        Ok(guard) => guard,
        Err(message) => {
            super::super::messaging::throw_named(scope, "DataCloneError", message);
            return None;
        }
    };
    let context = scope.get_current_context();
    let global = context.global(scope);
    let name = v8::String::new(scope, SERIALIZER)?;
    let key = v8::Private::for_api(scope, Some(name));
    let function = global.get_private(scope, key)?;
    let function = match v8::Local::<v8::Function>::try_from(function) {
        Ok(function) => function,
        Err(_) => {
            super::super::messaging::throw_named(
                scope,
                "DataCloneError",
                "Worker serializer is unavailable",
            );
            return None;
        }
    };
    let receiver = v8::undefined(scope).into();
    let serialized = function.call(scope, receiver, &[value, transfers])?;
    let serialized = v8::Local::<v8::String>::try_from(serialized).ok()?;
    if serialized.utf8_length(scope) > worker_message::MAX_MESSAGE_BYTES {
        super::super::messaging::throw_named(
            scope,
            "DataCloneError",
            "Worker clone metadata exceeds its storage limit",
        );
        return None;
    }
    match guard.finish(serialized.to_rust_string_lossy(scope)) {
        Ok(message) => Some(message),
        Err(message) => {
            super::super::messaging::throw_named(scope, "DataCloneError", message);
            None
        }
    }
}
