//! Browser-owned capabilities that depend on user intent or sensitive state.

use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if let Some(value) = speech::dispatch(operation, args, state)? {
        return Ok(Some(value));
    }
    if let Some(value) = notification::dispatch(operation, args, state)? {
        return Ok(Some(value));
    }
    if let Some(value) = geolocation::dispatch(operation, args, state)? {
        return Ok(Some(value));
    }
    if let Some(value) = sensor::dispatch(operation, args, state)? {
        return Ok(Some(value));
    }
    media_devices::dispatch(operation, args, state)
}
