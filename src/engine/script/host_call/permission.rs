//! Permission queries are queued for the browser's origin-keyed grant stores.

use super::super::binding_helpers::argument_string;
use super::super::*;
use crate::renderer_protocol::{DocumentId, PermissionName, PermissionRequest};

const MAX_PERMISSION_QUERIES_PER_DOCUMENT: usize = 64;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "permissionIsActive" {
        return Ok(Some(JsValue::from(
            state
                .frame_active
                .as_ref()
                .is_none_or(|active| active.get()),
        )));
    }
    if operation != "permissionQuery" {
        return Ok(None);
    }
    let name = match argument_string(args, 1)?.as_str() {
        "notifications" => PermissionName::Notifications,
        "geolocation" => PermissionName::Geolocation,
        "accelerometer" => PermissionName::Accelerometer,
        "gyroscope" => PermissionName::Gyroscope,
        "magnetometer" => PermissionName::Magnetometer,
        "ambient-light-sensor" => PermissionName::AmbientLightSensor,
        _ => {
            return Err(JsNativeError::typ()
                .with_message("Unsupported permission name")
                .into());
        }
    };
    if state.permission_query_count >= MAX_PERMISSION_QUERIES_PER_DOCUMENT {
        return Err(JsNativeError::range()
            .with_message("Permission query limit exceeded")
            .into());
    }
    let request_id = state
        .fetch_identifiers
        .borrow_mut()
        .allocate(state.document.id())?;
    state.permission_query_count += 1;
    state.pending_permission_actions.push(PermissionRequest {
        document: DocumentId::new(1).expect("nonzero protocol document"),
        request_id: u64::from(request_id),
        client: state.fetch_client,
        embedded: state.embedded,
        name,
    });
    Ok(Some(JsValue::from(request_id)))
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    update: &crate::renderer_protocol::PermissionUpdate,
) -> JsResult<()> {
    let serialized = serde_json::json!({
        "id":update.request_id,
        "name":update.name.as_str(),
        "state":update.state.as_str(),
        "rejected":update.rejected,
    })
    .to_string();
    context.call_private_hook(
        "__receivePermissionUpdate",
        &[JsValue::from(JsString::from(serialized))],
    )?;
    context.run_jobs()
}
