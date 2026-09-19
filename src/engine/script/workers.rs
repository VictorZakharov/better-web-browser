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
        document_url: String,
        client: crate::fetch::RequestClient,
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
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "workerStart" => {
            if !state.policy.is_empty() {
                return Err(JsNativeError::typ()
                    .with_message(
                        "Workers with inherited Content Security Policy are not yet supported",
                    )
                    .into());
            }
            let url = state.resolved_url(&argument_string(args, 1)?);
            let options: WorkerOptions =
                serde_json::from_str(&argument_string(args, 2)?).map_err(|error| {
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
            let id = state
                .worker_identifiers
                .borrow_mut()
                .allocate(state.document.id())?;
            state
                .pending_worker_actions
                .push(ScriptWorkerAction::Start {
                    id,
                    url,
                    kind,
                    name: options.name,
                    credentials,
                    document_url: state
                        .inherited_url
                        .as_ref()
                        .unwrap_or(&state.document_url)
                        .clone(),
                    client: state.fetch_client,
                });
            Ok(Some(JsValue::from(id)))
        }
        "workerPostMessage" => {
            let id = argument_id(args, 1);
            if state.worker_identifiers.borrow().owner(id) != Some(state.document.id()) {
                return Ok(Some(JsValue::undefined()));
            }
            state
                .pending_worker_actions
                .push(ScriptWorkerAction::PostMessage {
                    id,
                    serialized: argument_string(args, 2)?,
                });
            Ok(Some(JsValue::undefined()))
        }
        "workerTerminate" => {
            let id = argument_id(args, 1);
            if state.worker_identifiers.borrow().owner(id) != Some(state.document.id()) {
                return Ok(Some(JsValue::undefined()));
            }
            state.worker_identifiers.borrow_mut().finish(id);
            state
                .pending_worker_actions
                .push(ScriptWorkerAction::Terminate { id });
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
    let (kind, payload) = match event {
        Ok(message) => ("message", message),
        Err(error) => ("error", error),
    };
    context.call_global(
        "__completeWorkerEvent",
        &[
            JsValue::from(id),
            js_string(kind.to_string()),
            js_string(payload),
        ],
    )?;
    context.run_jobs()
}
