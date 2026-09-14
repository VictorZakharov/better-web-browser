//! Lossless Web Storage host operations.
use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "storageLength" => {
            let area = storage_area(args, 1)?;
            Ok(JsValue::from(state.storage_len(area) as u32))
        }
        "storageKey" => {
            let area = storage_area(args, 1)?;
            let index = argument_id(args, 2) as usize;
            Ok(state
                .storage_key(area, index)
                .map_or_else(JsValue::null, |value| JsValue::Utf16(value.clone())))
        }
        "storageGet" => {
            let area = storage_area(args, 1)?;
            let key = storage_string(args, 2)?;
            Ok(state
                .storage_get(area, &key)
                .map_or_else(JsValue::null, |value| JsValue::Utf16(value.clone())))
        }
        "storageSet" => {
            let area = storage_area(args, 1)?;
            let key = storage_string(args, 2)?;
            let value = storage_string(args, 3)?;
            match state.storage_set(area, key, value) {
                Ok(()) => Ok(JsValue::from(true)),
                Err(crate::storage::StorageError::QuotaExceeded) => Ok(JsValue::from(false)),
                Err(error) => Err(storage_error(error)),
            }
        }
        "storageRemove" => {
            let area = storage_area(args, 1)?;
            let key = storage_string(args, 2)?;
            state.storage_remove(area, key).map_err(storage_error)?;
            Ok(JsValue::undefined())
        }
        "storageClear" => {
            let area = storage_area(args, 1)?;
            state.storage_clear(area).map_err(storage_error)?;
            Ok(JsValue::undefined())
        }
        _ => return Ok(None),
    }?;
    Ok(Some(value))
}

fn storage_area(args: &[JsValue], index: usize) -> JsResult<crate::storage::StorageAreaKind> {
    match argument_string(args, index)?.as_str() {
        "local" => Ok(crate::storage::StorageAreaKind::Local),
        "session" => Ok(crate::storage::StorageAreaKind::Session),
        _ => Err(JsNativeError::typ()
            .with_message("invalid Web Storage area")
            .into()),
    }
}

fn storage_error(error: crate::storage::StorageError) -> engine::JsError {
    JsNativeError::error()
        .with_message(error.to_string())
        .into()
}
fn storage_string(args: &[JsValue], index: usize) -> JsResult<crate::storage::StorageString> {
    match args.get(index) {
        Some(JsValue::Utf16(value)) => Ok(value.clone()),
        _ => Ok(argument_string(args, index)?.into()),
    }
}
