//! Private packet byte operations bypass recursive JsValue conversion and base64.
use super::*;

pub(in crate::engine::script::engine) fn dispatch(
    scope: &mut v8::PinScope,
    operation: &str,
    arguments: v8::FunctionCallbackArguments,
    mut output: v8::ReturnValue,
) {
    if operation == "clonePacketBinaryDecode" {
        let input = arguments.get(1);
        // Legacy/persistent payloads contain base64, never the private '@' prefix.
        // This path deliberately does not consult a packet reader or author getters.
        if let Ok(text) = v8::Local::<v8::String>::try_from(input)
            && text.length() <= worker_message::MAX_MESSAGE_BYTES
        {
            let mut first = [0_u16; 1];
            text.write_v2(scope, 0, &mut first, v8::WriteFlags::empty());
            if first[0] != u16::from(b'@') {
                super::super::binary_clone::dispatch(scope, "cloneBinaryDecode", arguments, output);
                return;
            }
        }
    }
    let input = v8::Local::new(scope, arguments.get(1));
    match convert(scope, operation, input) {
        Ok(value) => output.set(value),
        Err(message) => super::super::messaging::throw_named(scope, "DataCloneError", message),
    }
}

fn convert<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    operation: &str,
    input: v8::Local<'s, v8::Value>,
) -> Result<v8::Local<'s, v8::Value>, &'static str> {
    let state = state(scope)?;
    if operation == "clonePacketBinaryDecode" {
        let text = v8::Local::<v8::String>::try_from(input)
            .map_err(|_| "Worker binary token is not a string")?;
        if text.length() > 96 {
            return Err("Worker binary token is too long");
        }
        let bytes = state.consume(&text.to_rust_string_lossy(scope))?;
        let length = bytes.len();
        // The receiver gets independent mutable backing storage. Sharing the envelope
        // across native queues eliminates wire copies, not the structured-clone copy.
        let mut copied = allocate(length)?;
        copied.copy_from_slice(&bytes);
        let store = v8::ArrayBuffer::new_backing_store_from_vec(copied).make_shared();
        let buffer = v8::ArrayBuffer::with_backing_store(scope, &store);
        return v8::Uint8Array::new(scope, buffer, 0, length)
            .map(Into::into)
            .ok_or("Worker binary receiver allocation failed");
    }
    let view = if operation == "clonePacketBufferEncode" {
        let buffer = v8::Local::<v8::ArrayBuffer>::try_from(input)
            .map_err(|_| "Worker value is not an ArrayBuffer")?;
        if buffer.was_detached() {
            return Err("Worker buffer is detached");
        }
        if buffer.byte_length() > MAX_BINARY_BYTES {
            return Err("Worker buffer exceeds the 16 MiB limit");
        }
        v8::Uint8Array::new(scope, buffer, 0, buffer.byte_length())
            .map(v8::Local::<v8::ArrayBufferView>::from)
            .ok_or("Worker buffer view allocation failed")?
    } else {
        v8::Local::<v8::ArrayBufferView>::try_from(input)
            .map_err(|_| "Worker byte input is not an ArrayBufferView")?
    };
    if view
        .buffer(scope)
        .is_none_or(|buffer| buffer.was_detached())
    {
        return Err("Worker byte input is detached");
    }
    let length = view.byte_length();
    let token = state.append(length, || {
        let mut bytes = allocate(length)?;
        if view.copy_contents(&mut bytes) != length {
            return Err("Worker byte input could not be copied");
        }
        Ok(Arc::new(bytes))
    })?;
    v8::String::new(scope, &token)
        .map(Into::into)
        .ok_or("Worker binary token allocation failed")
}

fn allocate(length: usize) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| "Worker binary storage allocation failed")?;
    bytes.resize(length, 0);
    Ok(bytes)
}
