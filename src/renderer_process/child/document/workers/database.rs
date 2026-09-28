//! Worker origin-storage requests share the document IPC but retain realm-local callbacks.
use super::*;
use crate::renderer_protocol::{DATABASE_RETIRE_CLIENT_PAYLOAD, DatabaseCommand, DatabaseEvent};

// Document requests use u32 IDs. A separate wire namespace prevents a worker
// response (including one arriving after termination) reaching the page realm.
const WORKER_DATABASE_BIT: u64 = 1 << 63;

fn is_worker_database_request(id: u64) -> bool {
    id & WORKER_DATABASE_BIT != 0
}

#[derive(Default)]
pub(super) struct WorkerDatabases {
    next_sequence: u64,
    active: HashMap<u64, (u32, u32)>,
    clients: HashMap<u32, crate::fetch::RequestClient>,
}

impl WorkerDatabases {
    pub(super) fn apply(
        &mut self,
        worker: u32,
        actions: Vec<ScriptDatabaseAction>,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        for action in actions {
            if self
                .clients
                .get(&worker)
                .is_some_and(|client| *client != action.client)
            {
                return Err("Worker origin-storage client identity changed".into());
            }
            let wire_id = self.allocate_wire_id()?;
            connection.send_database_command(DatabaseCommand {
                document,
                request_id: wire_id,
                client: action.client,
                payload: action.payload,
            })?;
            self.clients.insert(worker, action.client);
            self.active.insert(wire_id, (worker, action.id));
        }
        Ok(())
    }

    fn allocate_wire_id(&mut self) -> Result<u64, String> {
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .filter(|sequence| *sequence < WORKER_DATABASE_BIT)
            .ok_or("Worker origin-storage request identifiers exhausted")?;
        Ok(WORKER_DATABASE_BIT | self.next_sequence)
    }

    pub(super) fn deliver(
        &mut self,
        event: DatabaseEvent,
        handles: &HashMap<u32, WorkerHandle>,
    ) -> Option<(u32, mailbox::AdmissionError)> {
        if let Some((worker, id)) = self.active.remove(&event.request_id)
            && let Some(handle) = handles.get(&worker)
            && let Err(error) = handle.commands.try_send(WorkerCommand::Database {
                id,
                payload: event.payload,
            })
        {
            return Some((worker, error));
        }
        None
    }

    fn retire_command(
        &mut self,
        worker: u32,
        document: DocumentId,
    ) -> Result<Option<DatabaseCommand>, String> {
        self.active.retain(|_, (owner, _)| *owner != worker);
        let Some(client) = self.clients.remove(&worker) else {
            return Ok(None);
        };
        Ok(Some(DatabaseCommand {
            document,
            request_id: self.allocate_wire_id()?,
            client,
            payload: DATABASE_RETIRE_CLIENT_PAYLOAD.into(),
        }))
    }

    pub(super) fn cancel_worker(
        &mut self,
        worker: u32,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        if let Some(command) = self.retire_command(worker, document)? {
            connection.send_database_command(command)?;
        }
        Ok(())
    }
}

impl super::super::DocumentRuntime {
    pub(in crate::renderer_process::child) fn is_worker_database_request(id: u64) -> bool {
        is_worker_database_request(id)
    }

    pub(in crate::renderer_process::child) fn deliver_worker_database_event(
        &mut self,
        event: DatabaseEvent,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        if let Some((worker, error)) = self.workers.databases.deliver(event, &self.workers.handles)
        {
            self.workers
                .fail_worker(worker, error, connection, self.id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_results_use_a_disjoint_namespace_and_ignore_retired_requests() {
        let document = DocumentId::new(1).unwrap();
        let (commands, receiver) = mailbox::Mailbox::new(Arc::new(AtomicUsize::new(0)));
        let mut handles = HashMap::new();
        handles.insert(
            7,
            WorkerHandle {
                commands,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        );
        let first = WORKER_DATABASE_BIT | 1;
        let stale = WORKER_DATABASE_BIT | 2;
        let mut databases = WorkerDatabases::default();
        databases.active.insert(first, (7, 11));
        databases.active.insert(stale, (7, 12));
        assert!(!is_worker_database_request(11));
        assert!(is_worker_database_request(first));
        assert!(
            databases
                .deliver(
                    DatabaseEvent {
                        document,
                        request_id: first,
                        payload: "ok".into(),
                    },
                    &handles,
                )
                .is_none()
        );
        let mut command = receiver.try_recv().unwrap();
        assert!(
            matches!(command.take(), WorkerCommand::Database { id: 11, payload }
            if payload == "ok")
        );
        let client = crate::fetch::RequestClient {
            id: WORKER_DATABASE_BIT | 7,
            opaque: false,
        };
        databases.clients.insert(7, client);
        let retire = databases.retire_command(7, document).unwrap().unwrap();
        assert_eq!(retire.client, client);
        assert_eq!(retire.payload, DATABASE_RETIRE_CLIENT_PAYLOAD);
        assert!(is_worker_database_request(retire.request_id));
        assert!(
            databases
                .deliver(
                    DatabaseEvent {
                        document,
                        request_id: stale,
                        payload: "late".into(),
                    },
                    &handles,
                )
                .is_none()
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn saturated_worker_result_retires_client_instead_of_losing_the_response() {
        let document = DocumentId::new(1).unwrap();
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
            },
        );
        let wire = WORKER_DATABASE_BIT | 1;
        let client = crate::fetch::RequestClient {
            id: WORKER_DATABASE_BIT | 7,
            opaque: false,
        };
        let mut databases = WorkerDatabases::default();
        databases.active.insert(wire, (7, 11));
        databases.clients.insert(7, client);
        assert_eq!(
            databases.deliver(
                DatabaseEvent {
                    document,
                    request_id: wire,
                    payload: "result".into(),
                },
                &handles,
            ),
            Some((7, mailbox::AdmissionError::Saturated))
        );
        let retire = databases.retire_command(7, document).unwrap().unwrap();
        assert_eq!(retire.client, client);
        assert_eq!(retire.payload, DATABASE_RETIRE_CLIENT_PAYLOAD);
        assert!(databases.active.is_empty());
        assert!(databases.clients.is_empty());
    }
}
