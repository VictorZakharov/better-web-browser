//! Realm-local socket IDs map to document IPC IDs without reaching page callbacks.

use super::*;
use crate::fetch::RequestClient;
use crate::renderer_protocol::{
    WebSocketCommand, WebSocketEvent, WebSocketEventKind, WebSocketOperation,
};

const WORKER_WEBSOCKET_BIT: u64 = 1 << 63;

fn is_worker_websocket(id: u64) -> bool {
    id & WORKER_WEBSOCKET_BIT != 0
}

#[derive(Default)]
pub(super) struct WorkerWebSockets {
    next_sequence: u64,
    active: HashMap<u64, (u32, u32, RequestClient)>,
}

impl WorkerWebSockets {
    pub(super) fn apply(
        &mut self,
        worker: u32,
        actions: Vec<ScriptWebSocketAction>,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        for action in actions {
            let wire_id = match action.operation {
                WebSocketOperation::Open { .. } => {
                    self.next_sequence = self
                        .next_sequence
                        .checked_add(1)
                        .filter(|sequence| *sequence < WORKER_WEBSOCKET_BIT)
                        .ok_or("Worker WebSocket identifiers exhausted")?;
                    WORKER_WEBSOCKET_BIT | self.next_sequence
                }
                _ => {
                    let Some((&wire, _)) = self
                        .active
                        .iter()
                        .find(|(_, owner)| owner.0 == worker && owner.1 == action.id)
                    else {
                        continue;
                    };
                    wire
                }
            };
            let opening = matches!(action.operation, WebSocketOperation::Open { .. });
            connection.send_websocket_command(WebSocketCommand {
                document,
                socket_id: wire_id,
                client: action.client,
                operation: action.operation,
            })?;
            if opening {
                self.active
                    .insert(wire_id, (worker, action.id, action.client));
            }
        }
        Ok(())
    }

    pub(super) fn deliver(
        &mut self,
        mut event: WebSocketEvent,
        handles: &HashMap<u32, WorkerHandle>,
    ) -> Option<(u32, mailbox::AdmissionError)> {
        let wire_id = event.socket_id;
        let terminal = matches!(event.kind, WebSocketEventKind::Close { .. });
        let mut failed = None;
        if let Some(&(worker, id, _)) = self.active.get(&event.socket_id)
            && let Some(handle) = handles.get(&worker)
        {
            event.socket_id = u64::from(id);
            if let Err(error) = handle.commands.try_send(WorkerCommand::WebSocket(event)) {
                failed = Some((worker, error));
            }
        }
        if terminal {
            self.active.remove(&wire_id);
        }
        failed
    }

    fn cancel_commands(&mut self, worker: u32, document: DocumentId) -> Vec<WebSocketCommand> {
        let retired = self
            .active
            .iter()
            .filter_map(|(&wire, &(owner, _, client))| (owner == worker).then_some((wire, client)))
            .collect::<Vec<_>>();
        for (wire, _) in &retired {
            self.active.remove(wire);
        }
        retired
            .into_iter()
            .map(|(socket_id, client)| WebSocketCommand {
                document,
                socket_id,
                client,
                operation: WebSocketOperation::Cancel,
            })
            .collect()
    }

    pub(super) fn cancel_worker(
        &mut self,
        worker: u32,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        for command in self.cancel_commands(worker, document) {
            connection.send_websocket_command(command)?;
        }
        Ok(())
    }
}

impl super::super::DocumentRuntime {
    pub(in crate::renderer_process::child) fn is_worker_websocket(id: u64) -> bool {
        is_worker_websocket(id)
    }

    pub(in crate::renderer_process::child) fn deliver_worker_websocket_event(
        &mut self,
        event: WebSocketEvent,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        if let Some((worker, error)) = self
            .workers
            .websockets
            .deliver(event, &self.workers.handles)
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
    fn worker_socket_ids_are_disjoint_and_termination_retires_inflight_open() {
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
        let wire = WORKER_WEBSOCKET_BIT | 1;
        let mut sockets = WorkerWebSockets::default();
        sockets
            .active
            .insert(wire, (7, 11, RequestClient::default()));
        assert!(!is_worker_websocket(11));
        assert!(is_worker_websocket(wire));
        assert!(
            sockets
                .deliver(
                    WebSocketEvent {
                        document,
                        socket_id: wire,
                        kind: WebSocketEventKind::Open {
                            protocol: "chat".into(),
                        },
                    },
                    &handles,
                )
                .is_none()
        );
        let mut command = receiver.try_recv().unwrap();
        assert!(matches!(command.take(), WorkerCommand::WebSocket(event)
            if event.socket_id == 11));
        let cancel = sockets.cancel_commands(7, document);
        assert!(matches!(cancel.as_slice(), [WebSocketCommand {
            socket_id, operation: WebSocketOperation::Cancel, ..
        }] if *socket_id == wire));
        assert!(
            sockets
                .deliver(
                    WebSocketEvent {
                        document,
                        socket_id: wire,
                        kind: WebSocketEventKind::Close {
                            code: 1006,
                            reason: String::new(),
                            clean: false,
                        },
                    },
                    &handles,
                )
                .is_none()
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn saturated_worker_socket_event_cancels_all_live_sockets() {
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
        let first = WORKER_WEBSOCKET_BIT | 1;
        let second = WORKER_WEBSOCKET_BIT | 2;
        let mut sockets = WorkerWebSockets::default();
        sockets
            .active
            .insert(first, (7, 11, RequestClient::default()));
        sockets
            .active
            .insert(second, (7, 12, RequestClient::default()));
        assert_eq!(
            sockets.deliver(
                WebSocketEvent {
                    document,
                    socket_id: first,
                    kind: WebSocketEventKind::Open {
                        protocol: "chat".into(),
                    },
                },
                &handles,
            ),
            Some((7, mailbox::AdmissionError::Saturated))
        );
        let mut cancelled = sockets
            .cancel_commands(7, document)
            .into_iter()
            .map(|command| {
                assert!(matches!(command.operation, WebSocketOperation::Cancel));
                command.socket_id
            })
            .collect::<Vec<_>>();
        cancelled.sort_unstable();
        assert_eq!(cancelled, vec![first, second]);
        assert!(sockets.active.is_empty());
    }
}
