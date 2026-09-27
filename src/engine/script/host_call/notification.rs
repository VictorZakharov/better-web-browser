//! Script-to-browser Notifications bridge. The renderer never decides permission.

use super::super::binding_helpers::{argument_string, js_string};
use super::super::types::ScriptNotificationAction;
use super::super::*;
use crate::renderer_protocol::{
    DocumentId, NotificationAction, NotificationEvent, NotificationRequest, NotificationUpdate,
};
use serde::Deserialize;

// Keep one script task from filling the renderer's pending IPC vector before
// the browser can enforce its active-notification and response-lane limits.
const MAX_NOTIFICATION_ACTIONS_PER_TASK: usize = 64;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Command {
    RequestPermission,
    Show {
        title: String,
        body: String,
        tag: String,
    },
    Close {
        id: u64,
    },
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "notificationPermission" {
        return Ok(Some(js_string(
            state.notification_permission.as_str().to_string(),
        )));
    }
    if operation != "notificationRequest" {
        return Ok(None);
    }
    let payload = argument_string(args, 1)?;
    if payload.len() > 12 * 1024 {
        return Err(JsNativeError::range()
            .with_message("Notification request exceeds the browser limit")
            .into());
    }
    let command: Command = serde_json::from_str(&payload)
        .map_err(|_| JsNativeError::typ().with_message("Invalid Notification request"))?;
    if state.pending_notification_actions.len() >= MAX_NOTIFICATION_ACTIONS_PER_TASK {
        return Err(JsNativeError::range()
            .with_message("Notification task queue limit exceeded")
            .into());
    }
    let needs_id = !matches!(command, Command::Close { .. });
    let action = match &command {
        Command::RequestPermission => NotificationAction::RequestPermission,
        Command::Show { title, body, tag } => NotificationAction::Show {
            title: title.clone(),
            body: body.clone(),
            tag: tag.clone(),
        },
        Command::Close { .. } => NotificationAction::Close,
    };
    let id = match command {
        Command::Close { id } => id,
        _ => u64::from(
            state
                .fetch_identifiers
                .borrow_mut()
                .allocate(state.document.id())?,
        ),
    };
    let request = NotificationRequest {
        document: DocumentId::new(1).expect("nonzero protocol document"),
        request_id: id,
        client: state.fetch_client,
        action: action.clone(),
    };
    if request.validate().is_err() {
        if needs_id {
            state.fetch_identifiers.borrow_mut().finish(id as u32);
        }
        return Err(JsNativeError::range()
            .with_message("Notification request has invalid values")
            .into());
    }
    state
        .pending_notification_actions
        .push(ScriptNotificationAction {
            request_id: id,
            client: state.fetch_client,
            action,
        });
    Ok(Some(JsValue::from(id as u32)))
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    update: &NotificationUpdate,
) -> JsResult<()> {
    let event = match update.event {
        NotificationEvent::Permission(permission) => serde_json::json!({
            "kind":"permission", "permission":permission.as_str(),
        }),
        NotificationEvent::Shown => serde_json::json!({"kind":"shown"}),
        NotificationEvent::Clicked => serde_json::json!({"kind":"clicked"}),
        NotificationEvent::Closed => serde_json::json!({"kind":"closed"}),
        NotificationEvent::Error => serde_json::json!({"kind":"error"}),
    };
    // The bootstrap accepts the flat shape to avoid exposing protocol enum tags to JS.
    let serialized = serde_json::json!({
        "id":update.request_id,
        "kind":event["kind"],
        "permission":event.get("permission"),
    })
    .to_string();
    context.call_global("__receiveNotificationUpdate", &[js_string(serialized)])?;
    context.run_jobs()
}
