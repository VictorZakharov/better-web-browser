//! Browser-owned projections survive an HTML charset restart; the realm does not.
use super::*;
use crate::storage::StorageProjection;

pub(crate) struct RestartState {
    cookie_version: u64,
    cookie_header: String,
    local: StorageProjection,
    session: StorageProjection,
}

impl ScriptRuntime {
    pub(crate) fn take_restart_state(&mut self, outcome: &mut ScriptOutcome) -> RestartState {
        let mut host = self.host.borrow_mut();
        // The abandoned realm must not dispatch a queued storage event, but its
        // browser-owned delivery slot still needs a receipt before replay proceeds.
        if let Some(event) = host.storage_event.take() {
            outcome
                .storage_event_receipts
                .push((event.update.area, event.update.version));
        }
        RestartState {
            cookie_version: host.cookie_version,
            cookie_header: std::mem::take(&mut host.cookie_header),
            local: std::mem::take(&mut host.local_storage),
            session: std::mem::take(&mut host.session_storage),
        }
    }

    pub(crate) fn restore_restart_state(&mut self, state: RestartState) {
        let mut host = self.host.borrow_mut();
        host.cookie_version = state.cookie_version;
        host.cookie_header = state.cookie_header;
        host.local_storage = state.local;
        host.session_storage = state.session;
    }
}
