//! Apply storage projections immediately; dispatch events only as document tasks.
use super::*;
use crate::storage::{StorageAreaKind, StorageError, StorageUpdate};

impl ScriptRuntime {
    /// Returns true if a DOM-manipulation task was queued. Only one unacknowledged
    /// event enters a renderer at a time; its receipt is emitted after dispatch.
    pub fn synchronize_storage(&mut self, update: StorageUpdate) -> Result<bool, StorageError> {
        let mut host = self.host.borrow_mut();
        let event = update.acknowledgement == 0;
        if event && host.storage_event.is_some() {
            return Err(StorageError::Invalid("unacknowledged storage event"));
        }
        host.storage_mut(update.area).apply(&update)?;
        if event && self.context.is_some() {
            host.storage_event = Some(
                host.document_load
                    .storage_event(update, !host.pending_dynamic_scripts.is_empty()),
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub(super) fn run_one(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
) -> bool {
    let Some(task) = host.borrow_mut().storage_event.take() else {
        return false;
    };
    let update = task.update;
    host.borrow_mut().begin_task();
    let change = update.change.expect("validated foreign change");
    let value = |value: Option<crate::storage::StorageString>| {
        value.map_or_else(JsValue::null, JsValue::Utf16)
    };
    let arguments = [
        JsValue::from(if update.area == StorageAreaKind::Local {
            "local".to_string()
        } else {
            "session".to_string()
        }),
        value(change.key),
        value(change.old_value),
        value(change.new_value),
        JsValue::from(update.source_url),
    ];
    if let Err(error) = context.call_global("__dispatchStorageEvent", &arguments) {
        outcome
            .errors
            .push(format!("storage event dispatch: {error}"));
    }
    if let Err(error) = context.run_jobs() {
        outcome
            .errors
            .push(format!("storage event promise jobs: {error}"));
    }
    outcome
        .storage_event_receipts
        .push((update.area, update.version));
    true
}
