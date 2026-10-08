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
                ScriptFetchAction::Beacon { .. } => {
                    // WorkerNavigator has no Beacon method in this implementation.
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

    fn retire_orphans(&mut self, handles: &HashMap<u32, WorkerHandle>) -> Vec<u64> {
        let retired = self
            .active
            .iter()
            .filter_map(|(wire, (worker, _))| (!handles.contains_key(worker)).then_some(*wire))
            .collect::<Vec<_>>();
        for wire in &retired {
            self.active.remove(wire);
        }
        retired
    }

    pub(super) fn cancel_orphans(
        &mut self,
        handles: &HashMap<u32, WorkerHandle>,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        let mut first_error = None;
        for wire in self.retire_orphans(handles) {
            if let Err(error) = connection.abort_fetch(document, wire) {
                first_error.get_or_insert(error);
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    fn deliver(
        &mut self,
        request: u64,
        event: ScriptFetchEvent,
        handles: &HashMap<u32, WorkerHandle>,
    ) -> Option<(u32, mailbox::AdmissionError)> {
        let terminal = matches!(event, ScriptFetchEvent::End | ScriptFetchEvent::Abort(_));
        let mut failed = None;
        if let Some(&(worker, id)) = self.active.get(&request)
            && let Some(handle) = handles.get(&worker)
            && let Err(error) = handle.commands.try_send(WorkerCommand::Fetch { id, event })
        {
            failed = Some((worker, error));
        }
        if terminal {
            self.active.remove(&request);
        }
        failed
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
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        if let Some((worker, error)) = self.streaming.deliver(request, event, &self.handles) {
            self.fail_worker(worker, error, connection, document)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturated_fetch_chunk_retires_all_worker_requests_for_abort() {
        let (commands, _receiver) = mailbox::Mailbox::new(Arc::new(AtomicUsize::new(0)));
        for endpoint in 0..mailbox::MAX_PENDING_COMMANDS {
            commands
                .try_send(WorkerCommand::PortClose(endpoint as u32))
                .unwrap();
        }
        let mut handles = HashMap::new();
        handles.insert(
            7,
            WorkerHandle {
                commands,
                cancelled: Arc::new(AtomicBool::new(false)),
                execution: ScriptCancellation::default(),
            },
        );
        let mut fetches = WorkerFetches::default();
        fetches.active.insert(100, (7, 11));
        fetches.active.insert(101, (7, 12));
        assert_eq!(
            fetches.deliver(100, ScriptFetchEvent::Chunk(vec![1, 2, 3]), &handles),
            Some((7, mailbox::AdmissionError::Saturated))
        );
        assert_eq!(fetches.active.len(), 2);
        let mut aborted = fetches.retire_orphans(&HashMap::new());
        aborted.sort_unstable();
        assert_eq!(aborted, vec![100, 101]);
        assert!(fetches.active.is_empty());
    }

    #[test]
    fn terminal_fetch_event_removes_its_request_even_when_mailbox_is_full() {
        let (commands, _receiver) = mailbox::Mailbox::new(Arc::new(AtomicUsize::new(0)));
        for endpoint in 0..mailbox::MAX_PENDING_COMMANDS {
            commands
                .try_send(WorkerCommand::PortClose(endpoint as u32))
                .unwrap();
        }
        let mut handles = HashMap::new();
        handles.insert(
            7,
            WorkerHandle {
                commands,
                cancelled: Arc::new(AtomicBool::new(false)),
                execution: ScriptCancellation::default(),
            },
        );
        let mut fetches = WorkerFetches::default();
        fetches.active.insert(100, (7, 11));
        assert_eq!(
            fetches.deliver(100, ScriptFetchEvent::End, &handles),
            Some((7, mailbox::AdmissionError::Saturated))
        );
        assert!(fetches.active.is_empty());
    }
}
