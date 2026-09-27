//! Uploads that must cross the broker boundary before their document navigates.

use super::{ChildConnection, DocumentRuntime, script_api_request, script_beacon_request};
use crate::engine::ScriptFetchAction;
use std::collections::HashSet;

impl DocumentRuntime {
    /// Ordinary Fetch remains document-owned, but Beacon and keepalive Fetch must
    /// be admitted while their initiating client still exists.
    pub(in crate::renderer_process::child) fn start_pending_survivable_fetches(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        let mut retained = Vec::new();
        let mut requests = Vec::new();
        let mut started = Vec::new();
        let aborted = self
            .pending_fetches
            .iter()
            .filter_map(|action| match action {
                ScriptFetchAction::Abort { id } => Some(*id),
                _ => None,
            })
            .collect::<HashSet<_>>();
        for action in std::mem::take(&mut self.pending_fetches) {
            match action {
                ScriptFetchAction::Beacon { request } => {
                    let id = connection.allocate_request_id();
                    requests.push(script_beacon_request(id, self.id, *request));
                }
                ScriptFetchAction::Start { id, request }
                    if request.keepalive && !aborted.contains(&id) =>
                {
                    let wire_id = connection.allocate_request_id();
                    requests.push(script_api_request(wire_id, self.id, *request));
                    started.push((wire_id, id));
                }
                other => retained.push(other),
            }
        }
        self.pending_fetches = retained;
        if !requests.is_empty() {
            connection.start_streaming_fetch_batch(self.id, requests)?;
            self.active_script_fetches.extend(started);
        }
        Ok(())
    }
}
