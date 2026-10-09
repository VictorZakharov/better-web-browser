//! Dedicated Worker realms hosted inside the document's AppContainer process.

mod database;
mod mailbox;
mod network;
mod streaming;
mod thread;
mod websocket;
use crate::engine::script::ScriptCancellation;
use crate::engine::script::worker_runtime::WorkerExecutionPolicy;
use thread::run_worker;

use self::network::{
    PendingWorkerFetch, WorkerNetworkRequest, WorkerSourceRequest, finish_ready_network_batches,
    start_ready_network_batch, worker_source_request,
};
use super::fetch::validate_script_response;
use super::merge_outcome;
use crate::engine::{
    ScriptDatabaseAction, ScriptFetchAction, ScriptFetchEvent, ScriptKind, ScriptOutcome,
    ScriptRuntime, ScriptWebSocketAction, ScriptWorkerAction, WorkerPortEvent, WorkerRuntime,
    WorkerRuntimeOutcome, WorkerSourceLoader,
};
use crate::fetch::CredentialsMode;
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::DocumentId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use crate::engine::script::runtime::platform_info::MAX_DEDICATED_WORKERS;

pub(super) struct RendererWorkers {
    handles: HashMap<u32, WorkerHandle>,
    network_sender: mpsc::Sender<WorkerNetworkRequest>,
    network: mpsc::Receiver<WorkerNetworkRequest>,
    pending_network: Vec<PendingWorkerFetch>,
    streaming: streaming::WorkerFetches,
    databases: database::WorkerDatabases,
    websockets: websocket::WorkerWebSockets,
    event_sender: mpsc::Sender<WorkerEvent>,
    events: mpsc::Receiver<WorkerEvent>,
    mailbox_bytes: Arc<AtomicUsize>,
    pending_failures: Vec<(u32, String)>,
    execution_diagnostics: bool,
}

pub(super) struct WorkerDriveContext<'a> {
    pub(super) connection: &'a mut ChildConnection,
    pub(super) document: DocumentId,
    pub(super) document_url: &'a str,
    pub(super) runtime: &'a mut Option<ScriptRuntime>,
    pub(super) document_root: crate::engine::dom::NodeId,
    pub(super) outcome: &'a mut ScriptOutcome,
}

impl RendererWorkers {
    pub(super) fn new(execution_diagnostics: bool) -> Self {
        let (network_sender, network) = mpsc::channel();
        let (event_sender, events) = mpsc::channel();
        Self {
            handles: HashMap::new(),
            network_sender,
            network,
            pending_network: Vec::new(),
            streaming: Default::default(),
            databases: Default::default(),
            websockets: Default::default(),
            event_sender,
            events,
            mailbox_bytes: Arc::new(AtomicUsize::new(0)),
            pending_failures: Vec::new(),
            execution_diagnostics,
        }
    }

    pub(super) fn has_work(&self) -> bool {
        !self.handles.is_empty()
            || !self.pending_network.is_empty()
            || !self.pending_failures.is_empty()
    }

    pub(super) fn drive(
        &mut self,
        actions: Vec<ScriptWorkerAction>,
        context: WorkerDriveContext<'_>,
    ) -> Result<bool, String> {
        let WorkerDriveContext {
            connection,
            document,
            document_url,
            runtime,
            document_root,
            outcome,
        } = context;
        self.apply(actions, document_url, outcome, connection, document)?;
        finish_ready_network_batches(connection, &mut self.pending_network)?;
        start_ready_network_batch(
            connection,
            document,
            &self.network,
            &mut self.pending_network,
        )?;
        let mut delivered = false;
        while let Ok(event) = self.events.try_recv() {
            if !self.handles.contains_key(&event.id) {
                continue;
            }
            delivered = true;
            if event.closed {
                self.retire_worker(event.id, None, connection, document)?;
            }
            if self.handles.contains_key(&event.id) {
                self.streaming
                    .apply(event.id, event.fetch_actions, connection, document)?;
                self.databases
                    .apply(event.id, event.database_actions, connection, document)?;
                self.websockets
                    .apply(event.id, event.websocket_actions, connection, document)?;
            }
            let Some(runtime) = runtime.as_mut() else {
                continue;
            };
            for message in event.messages {
                let worker = runtime.complete_worker_event_with_loader(event.id, message, None);
                merge_outcome(outcome, worker, document_root);
            }
            for port_event in event.port_events {
                let (endpoint, message) = match port_event {
                    WorkerPortEvent::Message {
                        endpoint,
                        serialized,
                    } => (endpoint, Some(serialized)),
                    WorkerPortEvent::Closed { endpoint } => (endpoint, None),
                };
                let worker = runtime.complete_worker_port_event(event.id, endpoint, message);
                merge_outcome(outcome, worker, document_root);
            }
            outcome.diagnostics.extend(
                event
                    .diagnostics
                    .into_iter()
                    .map(|entry| format!("Worker {}: {entry}", event.id)),
            );
            outcome.console.extend(
                event
                    .console
                    .into_iter()
                    .map(|entry| format!("Worker {}: {entry}", event.id)),
            );
            outcome.errors.extend(
                event
                    .errors
                    .into_iter()
                    .map(|error| format!("Worker {}: {error}", event.id)),
            );
        }
        self.report_failures(runtime, outcome, document_root);
        let actions = std::mem::take(&mut outcome.worker_actions);
        if !actions.is_empty() {
            self.apply(actions, document_url, outcome, connection, document)?;
        }
        self.report_failures(runtime, outcome, document_root);
        self.streaming
            .cancel_orphans(&self.handles, connection, document)?;
        Ok(delivered)
    }

    fn apply(
        &mut self,
        actions: Vec<ScriptWorkerAction>,
        _document_url: &str,
        outcome: &mut ScriptOutcome,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        for action in actions {
            match action {
                ScriptWorkerAction::Start {
                    id,
                    url,
                    kind,
                    name,
                    credentials,
                    document_url,
                    creator_secure_context,
                    client,
                    worker_client,
                } => {
                    if self.handles.len() >= MAX_DEDICATED_WORKERS {
                        outcome.errors.push(format!(
                            "dedicated Worker limit of {MAX_DEDICATED_WORKERS} was reached"
                        ));
                        continue;
                    }
                    let (commands, receiver) = mailbox::Mailbox::new(self.mailbox_bytes.clone());
                    let cancelled = Arc::new(AtomicBool::new(false));
                    let execution = ScriptCancellation::default();
                    let config = WorkerConfig {
                        id,
                        url,
                        kind,
                        name,
                        credentials,
                        document_url,
                        creator_secure_context,
                        client,
                        worker_client,
                        network: self.network_sender.clone(),
                        events: self.event_sender.clone(),
                        commands: receiver,
                        cancelled: cancelled.clone(),
                        execution: execution.clone(),
                        execution_diagnostics: self.execution_diagnostics,
                    };
                    std::thread::Builder::new()
                        .name(format!("breeze-renderer-worker-{id}"))
                        .spawn(move || run_worker(config))
                        .map_err(|error| format!("start dedicated Worker: {error}"))?;
                    self.handles.insert(
                        id,
                        WorkerHandle {
                            commands,
                            cancelled,
                            execution,
                        },
                    );
                }
                ScriptWorkerAction::PostMessage { id, serialized } => {
                    self.enqueue(id, WorkerCommand::Message(serialized), connection, document)?;
                }
                ScriptWorkerAction::PortPostMessage {
                    id,
                    endpoint,
                    serialized,
                } => {
                    self.enqueue(
                        id,
                        WorkerCommand::PortMessage {
                            endpoint,
                            serialized,
                        },
                        connection,
                        document,
                    )?;
                }
                ScriptWorkerAction::PortClose { id, endpoint } => {
                    self.enqueue(id, WorkerCommand::PortClose(endpoint), connection, document)?;
                }
                ScriptWorkerAction::Terminate { id } => {
                    self.retire_worker(id, None, connection, document)?;
                }
            }
        }
        Ok(())
    }

    fn enqueue(
        &mut self,
        id: u32,
        command: WorkerCommand,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        let Some(handle) = self.handles.get(&id) else {
            return Ok(());
        };
        if let Err(error) = handle.commands.try_send(command) {
            self.fail_worker(id, error, connection, document)?;
        }
        Ok(())
    }

    fn fail_worker(
        &mut self,
        id: u32,
        error: mailbox::AdmissionError,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        let detail = match error {
            mailbox::AdmissionError::Saturated => format!(
                "command mailbox exceeded {} commands, {} MiB per Worker, or {} MiB per document",
                mailbox::MAX_PENDING_COMMANDS,
                mailbox::MAX_WORKER_PENDING_BYTES / (1024 * 1024),
                mailbox::MAX_DOCUMENT_WORKER_PENDING_BYTES / (1024 * 1024),
            ),
            mailbox::AdmissionError::Disconnected => "command mailbox disconnected".into(),
        };
        self.retire_worker(id, Some(detail), connection, document)
    }

    fn retire_worker(
        &mut self,
        id: u32,
        failure: Option<String>,
        connection: &mut ChildConnection,
        document: DocumentId,
    ) -> Result<(), String> {
        let Some(handle) = self.handles.remove(&id) else {
            return Ok(());
        };
        handle.terminate();
        if let Some(reason) = failure {
            self.pending_failures.push((id, reason));
        }
        // Finish every retirement attempt even when the browser connection itself fails.
        let fetch = self
            .streaming
            .cancel_orphans(&self.handles, connection, document);
        let database = self.databases.cancel_worker(id, connection, document);
        let sockets = self.websockets.cancel_worker(id, connection, document);
        fetch?;
        database?;
        sockets?;
        Ok(())
    }

    fn report_failures(
        &mut self,
        runtime: &mut Option<ScriptRuntime>,
        outcome: &mut ScriptOutcome,
        document_root: crate::engine::dom::NodeId,
    ) {
        for (id, reason) in std::mem::take(&mut self.pending_failures) {
            outcome.errors.push(format!("Worker {id}: {reason}"));
            if let Some(runtime) = runtime.as_mut() {
                let worker = runtime.complete_worker_event_with_loader(id, Err(reason), None);
                merge_outcome(outcome, worker, document_root);
            }
        }
    }
}

impl Drop for RendererWorkers {
    fn drop(&mut self) {
        for (_, worker) in self.handles.drain() {
            worker.terminate();
        }
    }
}

struct WorkerHandle {
    commands: mailbox::Mailbox,
    cancelled: Arc<AtomicBool>,
    execution: ScriptCancellation,
}

impl WorkerHandle {
    fn terminate(self) {
        self.cancelled.store(true, Ordering::Release);
        self.execution.cancel();
        // The flag is authoritative. A full queue must never block termination.
        let _ = self.commands.try_send(WorkerCommand::Terminate);
    }
}

enum WorkerCommand {
    Message(crate::engine::script::worker_message::WorkerMessage),
    PortMessage { endpoint: u32, serialized: String },
    PortClose(u32),
    Fetch { id: u32, event: ScriptFetchEvent },
    Database { id: u32, payload: String },
    WebSocket(crate::renderer_protocol::WebSocketEvent),
    Terminate,
}

struct WorkerEvent {
    id: u32,
    fetch_actions: Vec<ScriptFetchAction>,
    database_actions: Vec<ScriptDatabaseAction>,
    websocket_actions: Vec<ScriptWebSocketAction>,
    messages: Vec<Result<crate::engine::script::worker_message::WorkerMessage, String>>,
    port_events: Vec<WorkerPortEvent>,
    console: Vec<String>,
    errors: Vec<String>,
    diagnostics: Vec<String>,
    closed: bool,
}

struct WorkerConfig {
    id: u32,
    url: String,
    kind: ScriptKind,
    name: String,
    credentials: CredentialsMode,
    document_url: String,
    creator_secure_context: bool,
    client: crate::fetch::RequestClient,
    worker_client: crate::fetch::RequestClient,
    network: mpsc::Sender<WorkerNetworkRequest>,
    events: mpsc::Sender<WorkerEvent>,
    commands: mpsc::Receiver<mailbox::QueuedCommand>,
    cancelled: Arc<AtomicBool>,
    execution: ScriptCancellation,
    execution_diagnostics: bool,
}
