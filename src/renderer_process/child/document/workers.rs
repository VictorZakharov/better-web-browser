//! Dedicated Worker realms hosted inside the document's AppContainer process.

mod network;
mod streaming;
mod thread;
use thread::run_worker;

use self::network::{
    PendingWorkerFetch, WorkerNetworkRequest, finish_ready_network_batches,
    start_ready_network_batch, worker_source_request,
};
use super::fetch::validate_script_response;
use super::merge_outcome;
use crate::engine::{
    ScriptFetchAction, ScriptFetchEvent, ScriptKind, ScriptOutcome, ScriptRuntime,
    ScriptWorkerAction, WorkerRuntime, WorkerRuntimeOutcome, WorkerSourceLoader,
};
use crate::fetch::CredentialsMode;
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::DocumentId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const MAX_DEDICATED_WORKERS: usize = 16;

pub(super) struct RendererWorkers {
    handles: HashMap<u32, WorkerHandle>,
    network_sender: mpsc::Sender<WorkerNetworkRequest>,
    network: mpsc::Receiver<WorkerNetworkRequest>,
    pending_network: Vec<PendingWorkerFetch>,
    streaming: streaming::WorkerFetches,
    event_sender: mpsc::Sender<WorkerEvent>,
    events: mpsc::Receiver<WorkerEvent>,
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
    pub(super) fn new() -> Self {
        let (network_sender, network) = mpsc::channel();
        let (event_sender, events) = mpsc::channel();
        Self {
            handles: HashMap::new(),
            network_sender,
            network,
            pending_network: Vec::new(),
            streaming: Default::default(),
            event_sender,
            events,
        }
    }

    pub(super) fn has_work(&self) -> bool {
        !self.handles.is_empty() || !self.pending_network.is_empty()
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
        self.apply(actions, document_url, outcome)?;
        finish_ready_network_batches(connection, &mut self.pending_network)?;
        start_ready_network_batch(
            connection,
            document,
            &self.network,
            &mut self.pending_network,
        )?;
        let mut delivered = false;
        for event in self.events.try_iter() {
            if !self.handles.contains_key(&event.id) {
                continue;
            }
            delivered = true;
            if event.closed
                && let Some(handle) = self.handles.remove(&event.id)
            {
                handle.terminate();
            }
            if self.handles.contains_key(&event.id) {
                self.streaming
                    .apply(event.id, event.fetch_actions, connection, document)?;
            }
            let Some(runtime) = runtime.as_mut() else {
                continue;
            };
            for message in event.messages {
                let worker = runtime.complete_worker_event_with_loader(event.id, message, None);
                merge_outcome(outcome, worker, document_root);
            }
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
        let actions = std::mem::take(&mut outcome.worker_actions);
        if !actions.is_empty() {
            self.apply(actions, document_url, outcome)?;
        }
        self.streaming
            .cancel_orphans(&self.handles, connection, document)?;
        Ok(delivered)
    }

    fn apply(
        &mut self,
        actions: Vec<ScriptWorkerAction>,
        _document_url: &str,
        outcome: &mut ScriptOutcome,
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
                    client,
                } => {
                    if self.handles.len() >= MAX_DEDICATED_WORKERS {
                        outcome.errors.push(format!(
                            "dedicated Worker limit of {MAX_DEDICATED_WORKERS} was reached"
                        ));
                        continue;
                    }
                    let (commands, receiver) = mpsc::channel();
                    let cancelled = Arc::new(AtomicBool::new(false));
                    let config = WorkerConfig {
                        id,
                        url,
                        kind,
                        name,
                        credentials,
                        document_url,
                        client,
                        network: self.network_sender.clone(),
                        events: self.event_sender.clone(),
                        commands: receiver,
                        cancelled: cancelled.clone(),
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
                        },
                    );
                }
                ScriptWorkerAction::PostMessage { id, serialized } => {
                    if let Some(worker) = self.handles.get(&id) {
                        let _ = worker.commands.send(WorkerCommand::Message(serialized));
                    }
                }
                ScriptWorkerAction::Terminate { id } => {
                    if let Some(worker) = self.handles.remove(&id) {
                        worker.terminate();
                    }
                }
            }
        }
        Ok(())
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
    commands: mpsc::Sender<WorkerCommand>,
    cancelled: Arc<AtomicBool>,
}

impl WorkerHandle {
    fn terminate(self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.commands.send(WorkerCommand::Terminate);
    }
}

enum WorkerCommand {
    Message(String),
    Fetch { id: u32, event: ScriptFetchEvent },
    Terminate,
}

struct WorkerEvent {
    id: u32,
    fetch_actions: Vec<ScriptFetchAction>,
    messages: Vec<Result<String, String>>,
    console: Vec<String>,
    errors: Vec<String>,
    closed: bool,
}

struct WorkerConfig {
    id: u32,
    url: String,
    kind: ScriptKind,
    name: String,
    credentials: CredentialsMode,
    document_url: String,
    client: crate::fetch::RequestClient,
    network: mpsc::Sender<WorkerNetworkRequest>,
    events: mpsc::Sender<WorkerEvent>,
    commands: mpsc::Receiver<WorkerCommand>,
    cancelled: Arc<AtomicBool>,
}
