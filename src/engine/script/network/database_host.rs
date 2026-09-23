//! Asynchronous IndexedDB requests leave the renderer; no profile I/O runs in V8.
use super::super::binding_helpers::{argument_string, js_string};
use super::*;
use crate::limits::MAX_INDEXED_DB_IPC_BYTES;
use crate::renderer_protocol::DatabaseEvent;

#[derive(Debug, Clone)]
pub struct ScriptDatabaseAction {
    pub id: u32,
    pub client: crate::fetch::RequestClient,
    pub payload: String,
}

pub(in crate::engine::script) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation != "databaseRequest" {
        return Ok(None);
    }
    let payload = argument_string(args, 1)?;
    if payload.len() > MAX_INDEXED_DB_IPC_BYTES {
        return Err(JsNativeError::range()
            .with_message("IndexedDB request exceeds the browser limit")
            .into());
    }
    let id = state
        .fetch_identifiers
        .borrow_mut()
        .allocate(state.document.id())?;
    state.pending_database_actions.push(ScriptDatabaseAction {
        id,
        client: state.fetch_client,
        payload,
    });
    Ok(Some(JsValue::from(id)))
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    event: DatabaseEvent,
) -> JsResult<()> {
    context.call_global(
        "__receiveDatabaseEvent",
        &[
            JsValue::from(event.request_id as u32),
            js_string(event.payload),
        ],
    )?;
    context.run_jobs()
}
