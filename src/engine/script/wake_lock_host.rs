//! Document-scoped intents; the browser validates visibility and owns the OS request.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptWakeLockAction {
    pub request_id: u64,
    pub client: crate::fetch::RequestClient,
    pub action: crate::renderer_protocol::WakeLockAction,
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "wakeLockAvailable" => Ok(Some(JsValue::from(
            !state.embedded && state.document_origin.is_potentially_trustworthy(),
        ))),
        "wakeLockRequest" => {
            let id = args
                .get(1)
                .and_then(JsValue::as_number)
                .filter(|id| {
                    id.is_finite()
                        && *id >= 1.0
                        && *id <= ((1_u64 << 53) - 1) as f64
                        && id.fract() == 0.0
                })
                .map(|id| id as u64)
                .ok_or_else(|| {
                    JsNativeError::range().with_message("Invalid wake lock request ID")
                })?;
            if state.embedded || !state.document_origin.is_potentially_trustworthy() {
                return Err(JsNativeError::typ()
                    .with_message("Wake lock is unavailable for this document")
                    .into());
            }
            if state.pending_wake_lock_actions.len() >= 64 {
                return Err(JsNativeError::range()
                    .with_message("Too many wake lock actions")
                    .into());
            }
            state.pending_wake_lock_actions.push(ScriptWakeLockAction {
                request_id: id,
                client: state.fetch_client,
                action: if args.get(2).and_then(JsValue::as_boolean) == Some(true) {
                    crate::renderer_protocol::WakeLockAction::Acquire
                } else {
                    crate::renderer_protocol::WakeLockAction::Release
                },
            });
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}
