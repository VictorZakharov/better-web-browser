//! Owned bridge replies transfer byte allocations to V8. Borrowed values keep
//! copying semantics; author ArrayBuffers are never borrowed mutably by Rust.
use super::{JsError, JsErrorKind, JsResult, JsValue, allocation_error};

pub(in crate::engine::script::engine) fn value_from_v8(
    scope: &mut v8::PinScope,
    value: v8::Local<v8::Value>,
) -> JsResult<JsValue> {
    if value.is_undefined() {
        return Ok(JsValue::Undefined);
    }
    if value.is_null() {
        return Ok(JsValue::Null);
    }
    if value.is_boolean() {
        return Ok(JsValue::Boolean(value.boolean_value(scope)));
    }
    if value.is_number() {
        return value
            .number_value(scope)
            .map(JsValue::Number)
            .ok_or_else(|| JsError {
                kind: JsErrorKind::Type,
                message: "could not convert JavaScript number".into(),
            });
    }
    if value.is_array_buffer_view() {
        let view = v8::Local::<v8::ArrayBufferView>::try_from(value).map_err(|_| JsError {
            kind: JsErrorKind::Type,
            message: "could not read JavaScript typed array".into(),
        })?;
        let mut bytes = vec![0; view.byte_length()];
        view.copy_contents(&mut bytes);
        return Ok(JsValue::Bytes(bytes));
    }
    if value.is_array() {
        let array =
            v8::Local::<v8::Array>::try_from(value).map_err(|_| allocation_error("array"))?;
        let mut values = Vec::with_capacity(array.length() as usize);
        for index in 0..array.length() {
            let item = array.get_index(scope, index).ok_or_else(|| JsError {
                kind: JsErrorKind::Type,
                message: "could not read JavaScript array item".into(),
            })?;
            values.push(value_from_v8(scope, item)?);
        }
        return Ok(JsValue::Array(values));
    }
    value
        .to_string(scope)
        .map(|value| JsValue::String(value.to_rust_string_lossy(scope)))
        .ok_or_else(|| JsError {
            kind: JsErrorKind::Type,
            message: "could not convert JavaScript value to a string".into(),
        })
}

pub(in crate::engine::script::engine) fn value_to_v8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: &JsValue,
) -> JsResult<v8::Local<'s, v8::Value>> {
    Ok(match value {
        JsValue::Undefined => v8::undefined(scope).into(),
        JsValue::Null => v8::null(scope).into(),
        JsValue::Boolean(value) => v8::Boolean::new(scope, *value).into(),
        JsValue::Number(value) => v8::Number::new(scope, *value).into(),
        JsValue::String(value) => v8::String::new(scope, value)
            .ok_or_else(|| allocation_error("string"))?
            .into(),
        JsValue::Utf16(value) => {
            v8::String::new_from_two_byte(scope, value.units(), v8::NewStringType::Normal)
                .ok_or_else(|| allocation_error("storage string"))?
                .into()
        }
        JsValue::Bytes(value) => bytes(scope, value.clone())?,
        JsValue::Array(values) => {
            let array = v8::Array::new(scope, values.len() as i32);
            for (index, value) in values.iter().enumerate() {
                let value = value_to_v8(scope, value)?;
                if !array.set_index(scope, index as u32, value).unwrap_or(false) {
                    return Err(allocation_error("array item"));
                }
            }
            array.into()
        }
        JsValue::Object(entries) => {
            let object = v8::Object::new(scope);
            for (name, value) in entries {
                let name = v8::String::new(scope, name)
                    .ok_or_else(|| allocation_error("object property name"))?;
                let value = value_to_v8(scope, value)?;
                if !object.set(scope, name.into(), value).unwrap_or(false) {
                    return Err(allocation_error("object property"));
                }
            }
            object.into()
        }
    })
}

pub(super) fn value_to_v8_owned<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: JsValue,
) -> JsResult<v8::Local<'s, v8::Value>> {
    match value {
        JsValue::Bytes(value) => bytes(scope, value),
        JsValue::Array(values) => {
            let array = v8::Array::new(scope, values.len() as i32);
            for (index, value) in values.into_iter().enumerate() {
                let value = value_to_v8_owned(scope, value)?;
                if !array.set_index(scope, index as u32, value).unwrap_or(false) {
                    return Err(allocation_error("array item"));
                }
            }
            Ok(array.into())
        }
        JsValue::Object(entries) => {
            let object = v8::Object::new(scope);
            for (name, value) in entries {
                let name = v8::String::new(scope, &name)
                    .ok_or_else(|| allocation_error("object property name"))?;
                let value = value_to_v8_owned(scope, value)?;
                if !object.set(scope, name.into(), value).unwrap_or(false) {
                    return Err(allocation_error("object property"));
                }
            }
            Ok(object.into())
        }
        value => value_to_v8(scope, &value),
    }
}

fn bytes<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: Vec<u8>,
) -> JsResult<v8::Local<'s, v8::Value>> {
    let length = value.len();
    let backing = v8::ArrayBuffer::new_backing_store_from_vec(value).make_shared();
    let buffer = v8::ArrayBuffer::with_backing_store(scope, &backing);
    v8::Uint8Array::new(scope, buffer, 0, length)
        .map(Into::into)
        .ok_or_else(|| allocation_error("Uint8Array"))
}

#[cfg(test)]
mod tests;
