//! Main-realm dedicated-worker actions and asynchronous event delivery.

use super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;
use crate::fetch::CredentialsMode;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub enum ScriptWorkerAction {
    Start {
        id: u32,
        url: String,
        kind: ScriptKind,
        name: String,
        credentials: CredentialsMode,
    },
    PostMessage {
        id: u32,
        serialized: String,
    },
    Terminate {
        id: u32,
    },
}

#[derive(Deserialize)]
struct WorkerOptions {
    #[serde(default = "classic_worker_type", rename = "type")]
    worker_type: String,
    #[serde(default)]
    name: String,
    #[serde(default = "same_origin_credentials")]
    credentials: String,
}

fn classic_worker_type() -> String {
    "classic".into()
}

fn same_origin_credentials() -> String {
    "same-origin".into()
}

pub(super) fn worker_host_call(
    operation: &str,
    args: &[JsValue],
    context: &mut Context,
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "workerStart" => {
            let url = state.resolved_url(&argument_string(args, 1, context)?);
            let options: WorkerOptions = serde_json::from_str(&argument_string(args, 2, context)?)
                .map_err(|error| {
                    JsNativeError::typ().with_message(format!("invalid Worker options: {error}"))
                })?;
            let kind = match options.worker_type.as_str() {
                "classic" => ScriptKind::Classic,
                "module" => ScriptKind::Module,
                other => {
                    return Err(JsNativeError::typ()
                        .with_message(format!("unsupported Worker type `{other}`"))
                        .into());
                }
            };
            let id = state.next_worker_id;
            let credentials = match options.credentials.as_str() {
                "omit" => CredentialsMode::Omit,
                "same-origin" => CredentialsMode::SameOrigin,
                "include" => CredentialsMode::Include,
                other => {
                    return Err(JsNativeError::typ()
                        .with_message(format!("unsupported Worker credentials `{other}`"))
                        .into());
                }
            };
            state.next_worker_id = state.next_worker_id.checked_add(1).ok_or_else(|| {
                JsNativeError::range().with_message("Worker identifiers were exhausted")
            })?;
            state
                .pending_worker_actions
                .push(ScriptWorkerAction::Start {
                    id,
                    url,
                    kind,
                    name: options.name,
                    credentials,
                });
            Ok(Some(JsValue::from(id)))
        }
        "workerPostMessage" => {
            state
                .pending_worker_actions
                .push(ScriptWorkerAction::PostMessage {
                    id: argument_id(args, 1),
                    serialized: argument_string(args, 2, context)?,
                });
            Ok(Some(JsValue::undefined()))
        }
        "workerTerminate" => {
            state
                .pending_worker_actions
                .push(ScriptWorkerAction::Terminate {
                    id: argument_id(args, 1),
                });
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}

pub(super) fn deliver_worker_event(
    context: &mut Context,
    id: u32,
    event: Result<String, String>,
) -> JsResult<()> {
    let callback = context
        .global_object()
        .get(boa_engine::js_string!("__completeWorkerEvent"), context)?;
    let callback = callback
        .as_callable()
        .ok_or_else(|| JsNativeError::typ().with_message("Worker event hook is unavailable"))?;
    let (kind, payload) = match event {
        Ok(message) => ("message", message),
        Err(error) => ("error", error),
    };
    callback.call(
        &JsValue::undefined(),
        &[
            JsValue::from(id),
            js_string(kind.to_string()),
            js_string(payload),
        ],
        context,
    )?;
    context.run_jobs()
}
