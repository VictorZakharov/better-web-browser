use super::value::{JsError, JsErrorKind, JsResult, JsValue};
use crate::engine::script::host_state::HostState;
use crate::engine::script::worker_host::WorkerHostState;
use std::cell::RefCell;
use std::rc::Weak;

pub(in crate::engine::script) enum HostBridge {
    Document(Weak<RefCell<HostState>>),
    Worker(Weak<RefCell<WorkerHostState>>),
}

impl HostBridge {
    pub(in crate::engine::script) fn dispatch(&self, arguments: &[JsValue]) -> JsResult<JsValue> {
        let operation = arguments
            .first()
            .map(JsValue::string_value)
            .unwrap_or_default();
        match self {
            Self::Document(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                let mut host = host.borrow_mut();
                let started = host.host_call_profile.start();
                let result = crate::engine::script::host_call::dispatch_host_call(
                    &operation, arguments, &mut host,
                );
                host.host_call_profile.record(&operation, started);
                result
            }
            Self::Worker(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                crate::engine::script::worker_host::dispatch_worker_host_call(
                    &operation,
                    arguments,
                    &mut host.borrow_mut(),
                )
            }
        }
    }
}

fn inactive_host() -> JsError {
    JsError {
        kind: JsErrorKind::Type,
        message: "browser host is not active".into(),
    }
}

pub(super) fn install_host_call(
    scope: &mut v8::PinScope<'_, '_>,
    context: v8::Local<v8::Context>,
) -> JsResult<()> {
    let function = v8::Function::new(scope, host_call_callback)
        .ok_or_else(|| allocation_error("host function"))?;
    let name = v8::String::new(scope, "__hostCall")
        .ok_or_else(|| allocation_error("host function name"))?;
    context
        .global(scope)
        .set(scope, name.into(), function.into())
        .filter(|set| *set)
        .ok_or_else(|| JsError {
            kind: JsErrorKind::Error,
            message: "install JavaScript host bridge".into(),
        })?;
    Ok(())
}

fn host_call_callback(
    scope: &mut v8::PinScope,
    arguments: v8::FunctionCallbackArguments,
    mut return_value: v8::ReturnValue,
) {
    let operation = arguments.get(0).to_rust_string_lossy(scope);
    if operation == "arrayBufferDetach" {
        let value = arguments.get(1);
        let Ok(buffer) = v8::Local::<v8::ArrayBuffer>::try_from(value) else {
            throw_error(
                scope,
                JsError {
                    kind: JsErrorKind::Type,
                    message: "transfer value is not an ArrayBuffer".into(),
                },
            );
            return;
        };
        let _ = buffer.detach(None);
        return_value.set(v8::undefined(scope).into());
        return;
    }
    let mut values = Vec::with_capacity(arguments.length() as usize);
    for index in 0..arguments.length() {
        match value_from_v8(scope, arguments.get(index)) {
            Ok(value) => values.push(value),
            Err(error) => {
                throw_error(scope, error);
                return;
            }
        }
    }
    let Some(bridge) = scope.get_current_context().get_slot::<HostBridge>() else {
        throw_error(
            scope,
            JsError {
                kind: JsErrorKind::Type,
                message: "browser host bridge is unavailable".into(),
            },
        );
        return;
    };
    match bridge
        .dispatch(&values)
        .and_then(|value| value_to_v8(scope, &value))
    {
        Ok(value) => return_value.set(value),
        Err(error) => throw_error(scope, error),
    }
}

pub(super) fn value_from_v8(
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

pub(super) fn value_to_v8<'s>(
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
        JsValue::Bytes(value) => {
            let backing = v8::ArrayBuffer::new_backing_store_from_vec(value.clone()).make_shared();
            let buffer = v8::ArrayBuffer::with_backing_store(scope, &backing);
            v8::Uint8Array::new(scope, buffer, 0, value.len())
                .ok_or_else(|| allocation_error("Uint8Array"))?
                .into()
        }
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
    })
}

fn throw_error(scope: &mut v8::PinScope, error: JsError) {
    let Some(message) = v8::String::new(scope, &error.message) else {
        return;
    };
    let exception = match error.kind {
        JsErrorKind::Error => v8::Exception::error(scope, message),
        JsErrorKind::Type => v8::Exception::type_error(scope, message),
        JsErrorKind::Range => v8::Exception::range_error(scope, message),
    };
    scope.throw_exception(exception);
}

fn allocation_error(value: &str) -> JsError {
    JsError {
        kind: JsErrorKind::Range,
        message: format!("V8 could not allocate {value}"),
    }
}
