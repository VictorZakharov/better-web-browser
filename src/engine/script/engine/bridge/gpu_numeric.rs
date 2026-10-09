//! Owned, bounded numeric WebGL setters, before the generic JSON value bridge.
//! Web IDL conversion remains in the bootstrap. This boundary neither coerces
//! values nor borrows author memory; no HostState borrow spans V8 property reads.
use super::*;
use crate::engine::webgl::{MAX_NUMERIC_VALUES, NumericCommand};

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
    let result = prepare(scope, &arguments).and_then(|(id, command)| {
        // Recheck liveness after reads: malformed direct private calls must not
        // hold RefCell borrows across getters or nested author execution.
        match &*bridge {
            HostBridge::Document(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                let mut host = host.borrow_mut();
                let native_started = host.host_call_profile.start();
                let live = host.webgl.execute_numeric(id, command);
                host.host_call_profile
                    .record("webglCommandValues", native_started);
                Ok(live)
            }
            HostBridge::Worker(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                let live = host.borrow_mut().webgl.execute_numeric(id, command);
                Ok(live)
            }
        }
    });
    bridge.profile_bridge("webglCommandValues", started);
    match result {
        Ok(live) => output.set(v8::Boolean::new(scope, live).into()),
        Err(error) => throw_error(scope, error),
    }
}

fn invalid() -> JsError {
    JsNativeError::typ()
        .with_message("Invalid bounded WebGL numeric command")
        .into()
}

fn prepare(
    scope: &mut v8::PinScope,
    args: &v8::FunctionCallbackArguments,
) -> JsResult<(u32, NumericCommand)> {
    if args.length() != 5 {
        return Err(invalid());
    }
    let id = number(scope, args.get(1))?;
    if id < 1. || id > u32::MAX as f64 || id.fract() != 0. {
        return Err(invalid());
    }
    let op = v8::Local::<v8::String>::try_from(args.get(2)).map_err(|_| invalid())?;
    if op.length() > 64 {
        return Err(invalid());
    }
    let op = op.to_rust_string_lossy(scope);
    let integers = array(args.get(3))?;
    let floats = FloatList::new(scope, args.get(4))?;
    // Freeze both lengths before any element read can run an accessor. Even a
    // malformed direct call cannot grow the second allocation through the first.
    let integer_count = integers.length();
    let float_count = floats.length();
    let count = integer_count as usize + float_count;
    if count > MAX_NUMERIC_VALUES {
        return Err(invalid());
    }
    let mut integer_values = [0.0; MAX_NUMERIC_VALUES];
    let mut float_values = [0.0; MAX_NUMERIC_VALUES];
    for index in 0..integer_count {
        let value = integers.get_index(scope, index).ok_or_else(invalid)?;
        integer_values[index as usize] = number(scope, value)?;
    }
    floats.copy(scope, &mut float_values[..float_count])?;
    if integers.length() != integer_count || floats.length() != float_count {
        return Err(invalid());
    }
    let mut i = Vec::with_capacity(integer_count as usize);
    for &value in &integer_values[..integer_count as usize] {
        if !value.is_finite() || value.abs() > 9_007_199_254_740_991. || value.fract() != 0. {
            return Err(invalid());
        }
        i.push(value as i64);
    }
    let f = float_values[..float_count].to_vec();
    let command = NumericCommand::new(op, i, f).ok_or_else(invalid)?;
    Ok((id as u32, command))
}

enum FloatList<'s> {
    Array(v8::Local<'s, v8::Array>),
    Typed(v8::Local<'s, v8::Float32Array>),
}
impl<'s> FloatList<'s> {
    fn new(scope: &mut v8::PinScope, value: v8::Local<'s, v8::Value>) -> JsResult<Self> {
        if value.is_proxy() {
            return Err(invalid());
        }
        if let Ok(array) = v8::Local::<v8::Array>::try_from(value) {
            return Ok(Self::Array(array));
        }
        let view = v8::Local::<v8::Float32Array>::try_from(value).map_err(|_| invalid())?;
        Self::fixed(scope, view)?;
        Ok(Self::Typed(view))
    }
    fn length(&self) -> usize {
        match self {
            Self::Array(array) => array.length() as usize,
            Self::Typed(view) => view.length(),
        }
    }
    fn fixed(scope: &mut v8::PinScope, view: v8::Local<v8::Float32Array>) -> JsResult<()> {
        let buffer = view.buffer(scope).ok_or_else(invalid)?;
        // Public IDL conversion snapshots shared sources into a fixed private
        // ArrayBuffer. Direct malformed calls cannot pass racing shared bytes.
        if buffer.is_shared_array_buffer()
            || buffer.was_detached()
            || buffer.get_backing_store().is_resizable_by_user_javascript()
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn copy(&self, scope: &mut v8::PinScope, out: &mut [f64]) -> JsResult<()> {
        match self {
            Self::Array(array) => {
                for (index, target) in out.iter_mut().enumerate() {
                    let value = array.get_index(scope, index as u32).ok_or_else(invalid)?;
                    *target = number(scope, value)?;
                }
            }
            Self::Typed(view) => {
                // Integer getters may have detached the buffer since admission.
                // No author memory is retained or read by the native GL thread.
                Self::fixed(scope, *view)?;
                let mut bytes = [0u8; MAX_NUMERIC_VALUES * 4];
                let count = out.len() * 4;
                if view.byte_length() != count || view.copy_contents(&mut bytes[..count]) != count {
                    return Err(invalid());
                }
                for (target, component) in out.iter_mut().zip(bytes[..count].chunks_exact(4)) {
                    *target = f32::from_ne_bytes(component.try_into().unwrap()) as f64;
                }
            }
        }
        Ok(())
    }
}

fn array(value: v8::Local<v8::Value>) -> JsResult<v8::Local<v8::Array>> {
    // Never let a proxy hide its length or introduce an unbounded conversion.
    if value.is_proxy() {
        return Err(invalid());
    }
    v8::Local::<v8::Array>::try_from(value).map_err(|_| invalid())
}

fn number(scope: &mut v8::PinScope, value: v8::Local<v8::Value>) -> JsResult<f64> {
    if !value.is_number() {
        return Err(invalid());
    }
    value.number_value(scope).ok_or_else(invalid)
}
