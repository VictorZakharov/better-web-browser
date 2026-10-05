//! Read typed-array slots through V8, never author constructors or byte getters.
pub(super) fn snapshot<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    input: v8::Local<'s, v8::Value>,
) -> Result<v8::Local<'s, v8::Value>, &'static str> {
    let view = v8::Local::<v8::ArrayBufferView>::try_from(input)
        .map_err(|_| "clone value is not an ArrayBufferView")?;
    if view
        .get_backing_store()
        .is_some_and(|store| store.is_shared())
    {
        return Err("shared backing stores require shared-memory clone transport");
    }
    let buffer = view
        .buffer(scope)
        .ok_or("clone view has no backing buffer")?;
    if buffer.was_detached() {
        return Err("clone view is detached");
    }
    let name = if input.is_uint8_clamped_array() {
        "Uint8ClampedArray"
    } else if input.is_uint8_array() {
        "Uint8Array"
    } else if input.is_int8_array() {
        "Int8Array"
    } else if input.is_uint16_array() {
        "Uint16Array"
    } else if input.is_int16_array() {
        "Int16Array"
    } else if input.is_uint32_array() {
        "Uint32Array"
    } else if input.is_int32_array() {
        "Int32Array"
    } else if input.is_float16_array() {
        "Float16Array"
    } else if input.is_float32_array() {
        "Float32Array"
    } else if input.is_float64_array() {
        "Float64Array"
    } else if input.is_big_int64_array() {
        "BigInt64Array"
    } else if input.is_big_uint64_array() {
        "BigUint64Array"
    } else if input.is_data_view() {
        "DataView"
    } else {
        return Err("unsupported clone view brand");
    };
    let length = if input.is_data_view() {
        view.byte_length()
    } else {
        v8::Local::<v8::TypedArray>::try_from(input)
            .map_err(|_| "unsupported clone view brand")?
            .length()
    };
    let name = v8::String::new(scope, name).ok_or("clone view metadata allocation failed")?;
    let offset = v8::Number::new(scope, view.byte_offset() as f64);
    let length = v8::Number::new(scope, length as f64);
    Ok(v8::Array::new_with_elements(
        scope,
        &[name.into(), buffer.into(), offset.into(), length.into()],
    )
    .into())
}
