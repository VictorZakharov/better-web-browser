//! Bounded native byte conversion for the existing serialized clone transport.
//! This is not a new public codec or a replacement for V8's ValueSerializer.
use base64::{Engine as _, engine::general_purpose::STANDARD};
mod view;

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENCODED_BYTES: usize = MAX_BYTES.div_ceil(3) * 4;

pub(super) fn dispatch(
    scope: &mut v8::PinScope,
    operation: &str,
    arguments: v8::FunctionCallbackArguments,
    mut output: v8::ReturnValue,
) {
    let input = v8::Local::new(scope, arguments.get(1));
    let result = convert(scope, operation, input);
    match result {
        Ok(value) => output.set(value),
        Err(message) => {
            if let Some(message) = v8::String::new(scope, message) {
                let exception = v8::Exception::type_error(scope, message);
                scope.throw_exception(exception);
            }
        }
    }
}

fn convert<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    operation: &str,
    input: v8::Local<'s, v8::Value>,
) -> Result<v8::Local<'s, v8::Value>, &'static str> {
    if operation == "cloneBinaryView" {
        return view::snapshot(scope, input);
    }
    if operation == "cloneBufferEncode" {
        let buffer = v8::Local::<v8::ArrayBuffer>::try_from(input)
            .map_err(|_| "clone value is not an ArrayBuffer")?;
        if buffer.was_detached() {
            return Err("clone buffer is detached");
        }
        let length = buffer.byte_length();
        if length > MAX_BYTES {
            return Err("clone buffer exceeds the 16 MiB limit");
        }
        let bytes = v8::Uint8Array::new(scope, buffer, 0, length)
            .ok_or("clone buffer view allocation failed")?;
        return convert(scope, "cloneBinaryEncode", bytes.into());
    }
    if operation == "arrayBufferDetach" {
        let buffer = v8::Local::<v8::ArrayBuffer>::try_from(input)
            .map_err(|_| "transfer value is not an ArrayBuffer")?;
        let _ = buffer.detach(None);
        return Ok(v8::undefined(scope).into());
    }
    if operation == "cloneBinaryEncode" {
        let view = v8::Local::<v8::ArrayBufferView>::try_from(input)
            .map_err(|_| "clone byte input is not an ArrayBufferView")?;
        if view.byte_length() > MAX_BYTES {
            return Err("clone byte input exceeds the 16 MiB limit");
        }
        if view
            .buffer(scope)
            .is_some_and(|buffer| buffer.was_detached())
        {
            return Err("clone byte input is detached");
        }
        let mut bytes = vec![0; view.byte_length()];
        // V8 handles view offsets and backing-store lifetime. Do not consult
        // author getters, constructors or typed-array iteration overrides.
        if view.copy_contents(&mut bytes) != bytes.len() {
            return Err("clone byte input could not be copied");
        }
        let encoded = STANDARD.encode(bytes);
        return v8::String::new(scope, &encoded)
            .map(Into::into)
            .ok_or("clone byte string allocation failed");
    }
    let text =
        v8::Local::<v8::String>::try_from(input).map_err(|_| "clone byte input is not a string")?;
    // Check before copying a page-controlled string or allocating decoded bytes.
    if text.length() > MAX_ENCODED_BYTES || text.utf8_length(scope) > MAX_ENCODED_BYTES {
        return Err("clone byte string exceeds the encoded limit");
    }
    let encoded = text.to_rust_string_lossy(scope);
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "invalid clone byte encoding")?;
    if bytes.len() > MAX_BYTES {
        return Err("decoded clone bytes exceed the 16 MiB limit");
    }
    let length = bytes.len();
    let backing = v8::ArrayBuffer::new_backing_store_from_vec(bytes).make_shared();
    let buffer = v8::ArrayBuffer::with_backing_store(scope, &backing);
    v8::Uint8Array::new(scope, buffer, 0, length)
        .map(Into::into)
        .ok_or("clone byte array allocation failed")
}
