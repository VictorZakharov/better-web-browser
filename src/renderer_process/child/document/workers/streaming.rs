//! Worker Fetch is asynchronous; script loading remains a separate buffered operation.
use super::super::fetch::script_api_request;
use super::*;

#[derive(Default)]
pub(super) struct WorkerFetches {
    active: HashMap<u64, (u32, u32)>,
}

impl WorkerFetches {
    pub(super) fn apply(
        &mut self,
        worker: u32,
        actions: Vec<ScriptFetchAction>,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        for action in actions {
            match action {
                ScriptFetchAction::Start { id, request } => {
                    let wire_id = connection.allocate_request_id();
                    connection.start_streaming_fetch_batch(
                        document,
                        vec![script_api_request(wire_id, document, *request)],
                    )?;
                    self.active.insert(wire_id, (worker, id));
                }
                ScriptFetchAction::Consume { id, total } => {
                    if let Some((&wire_id, _)) = self
                        .active
                        .iter()
                        .find(|(_, owner)| **owner == (worker, id))
                    {
                        connection.consume_fetch(document, wire_id, total)?;
                    }
                }
                ScriptFetchAction::Abort { id } => {
                    let wire_id = self
                        .active
                        .iter()
                        .find_map(|(wire, owner)| (*owner == (worker, id)).then_some(*wire));
                    if let Some(wire_id) = wire_id {
                        connection.abort_fetch(document, wire_id)?;
                        self.active.remove(&wire_id);
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn cancel_orphans(
        &mut self,
        handles: &HashMap<u32, WorkerHandle>,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        let retired = self
            .active
            .iter()
            .filter_map(|(wire, (worker, _))| (!handles.contains_key(worker)).then_some(*wire))
            .collect::<Vec<_>>();
        for wire in retired {
            connection.abort_fetch(document, wire)?;
            self.active.remove(&wire);
        }
        Ok(())
    }
}

impl RendererWorkers {
    pub(in crate::renderer_process::child::document) fn owns_fetch(&self, request: u64) -> bool {
        self.streaming.active.contains_key(&request)
    }

    pub(in crate::renderer_process::child::document) fn deliver_fetch(
        &mut self,
        request: u64,
        event: ScriptFetchEvent,
    ) {
        let terminal = matches!(event, ScriptFetchEvent::End | ScriptFetchEvent::Abort(_));
        if let Some(&(worker, id)) = self.streaming.active.get(&request)
            && let Some(handle) = self.handles.get(&worker)
        {
            let _ = handle.commands.send(WorkerCommand::Fetch { id, event });
        }
        if terminal {
            self.streaming.active.remove(&request);
        }
    }
}
