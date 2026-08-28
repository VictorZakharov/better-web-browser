//! Dedicated Worker realms hosted inside the document's AppContainer process.

mod network;

use self::network::{
    PendingWorkerFetch, WorkerNetworkRequest, finish_ready_network_batches, request_network,
    start_ready_network_batch, worker_source_request,
};
use super::fetch::validate_script_response;
use super::{fetch_script_source, merge_outcome};
use crate::engine::{
    ScriptKind, ScriptOutcome, ScriptRuntime, ScriptWorkerAction, WorkerRuntime,
    WorkerRuntimeOutcome, WorkerSourceLoader,
};
use crate::fetch::CredentialsMode;
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::DocumentId;
use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const MAX_DEDICATED_WORKERS: usize = 16;

pub(super) struct RendererWorkers {
    handles: HashMap<u32, WorkerHandle>,
    network_sender: mpsc::Sender<WorkerNetworkRequest>,
    network: mpsc::Receiver<WorkerNetworkRequest>,
    pending_network: Vec<PendingWorkerFetch>,
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
            delivered = true;
            if event.closed
                && let Some(handle) = self.handles.remove(&event.id)
            {
                handle.terminate();
            }
            let Some(runtime) = runtime.as_mut() else {
                continue;
            };
            let mut loader = |url: &str, kind, options| {
                fetch_script_source(connection, document, url, kind, options)
            };
            for message in event.messages {
                let worker =
                    runtime.complete_worker_event_with_loader(event.id, message, Some(&mut loader));
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
        Ok(delivered)
    }

    fn apply(
        &mut self,
        actions: Vec<ScriptWorkerAction>,
        document_url: &str,
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
                } => {
                    if self.handles.len() >= MAX_DEDICATED_WORKERS {
                        outcome.errors.push(format!(
                            "dedicated Worker limit of {MAX_DEDICATED_WORKERS} was reached"
                        ));
                        continue;
                    }
                    let (commands, receiver) = mpsc::channel();
                    let config = WorkerConfig {
                        id,
                        url,
                        kind,
                        name,
                        credentials,
                        document_url: document_url.to_string(),
                        network: self.network_sender.clone(),
                        events: self.event_sender.clone(),
                        commands: receiver,
                    };
                    std::thread::Builder::new()
                        .name(format!("breeze-renderer-worker-{id}"))
                        .spawn(move || run_worker(config))
                        .map_err(|error| format!("start dedicated Worker: {error}"))?;
                    self.handles.insert(id, WorkerHandle { commands });
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
}

impl WorkerHandle {
    fn terminate(self) {
        let _ = self.commands.send(WorkerCommand::Terminate);
    }
}

enum WorkerCommand {
    Message(String),
    Terminate,
}

struct WorkerEvent {
    id: u32,
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
    network: mpsc::Sender<WorkerNetworkRequest>,
    events: mpsc::Sender<WorkerEvent>,
    commands: mpsc::Receiver<WorkerCommand>,
}

fn run_worker(config: WorkerConfig) {
    let response = worker_source_request(
        &config.network,
        &config.document_url,
        &config.url,
        config.kind,
        config.credentials,
    );
    let response = match response {
        Ok(response) if response.is_success() => response,
        Ok(response) => {
            emit_error(
                &config,
                format!("entry script returned HTTP {}", response.status),
            );
            return;
        }
        Err(error) => {
            emit_error(&config, error.to_string());
            return;
        }
    };
    if let Err(error) = validate_script_response(&response, config.kind) {
        emit_error(&config, error.to_string());
        return;
    }
    let source = crate::winhttp::decode_text(response.body.as_bytes(), response.content_type());
    let network = config.network.clone();
    let document_url = config.document_url.clone();
    let credentials = config.credentials;
    let loader: Arc<WorkerSourceLoader> = Arc::new(move |url, kind| {
        let response = worker_source_request(&network, &document_url, url, kind, credentials)
            .map_err(|error| error.to_string())?;
        if !response.is_success() {
            return Err(format!("server returned HTTP {}", response.status));
        }
        validate_script_response(&response, kind).map_err(|error| error.to_string())?;
        Ok(crate::winhttp::decode_text(
            response.body.as_bytes(),
            response.content_type(),
        ))
    });
    let (runtime, initial) =
        WorkerRuntime::start(&config.url, &source, &config.name, config.kind, loader);
    let Some(mut runtime) = runtime else {
        emit(&config, initial);
        return;
    };
    if drive_worker_outcome(&config, &mut runtime, initial) {
        return;
    }
    let mut last_tick = Instant::now();
    loop {
        let timeout = runtime
            .next_timer_delay()
            .unwrap_or(Duration::from_millis(100))
            .min(Duration::from_millis(100));
        let command = config.commands.recv_timeout(timeout);
        let elapsed = last_tick.elapsed();
        last_tick = Instant::now();
        let timed = runtime.advance_time(elapsed, 64);
        if drive_worker_outcome(&config, &mut runtime, timed) {
            break;
        }
        match command {
            Ok(WorkerCommand::Message(serialized)) => {
                let message = runtime.dispatch_message(&serialized);
                if drive_worker_outcome(&config, &mut runtime, message) {
                    break;
                }
            }
            Ok(WorkerCommand::Terminate) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    runtime.cancel();
}

fn drive_worker_outcome(
    config: &WorkerConfig,
    runtime: &mut WorkerRuntime,
    mut outcome: WorkerRuntimeOutcome,
) -> bool {
    loop {
        let actions = std::mem::take(&mut outcome.fetch_actions);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                crate::engine::ScriptFetchAction::Start { id, request } => {
                    let result = request_network(&config.network, *request);
                    append_worker_outcome(&mut outcome, runtime.complete_fetch(id, result));
                }
                crate::engine::ScriptFetchAction::Abort { .. } => {}
            }
        }
    }
    let closed = outcome.closed || !outcome.errors.is_empty();
    emit(config, outcome);
    closed
}

fn append_worker_outcome(target: &mut WorkerRuntimeOutcome, mut source: WorkerRuntimeOutcome) {
    target.messages.append(&mut source.messages);
    target.fetch_actions.append(&mut source.fetch_actions);
    target.console.append(&mut source.console);
    target.errors.append(&mut source.errors);
    target.closed |= source.closed;
}

fn emit(config: &WorkerConfig, outcome: WorkerRuntimeOutcome) {
    let messages = outcome
        .messages
        .into_iter()
        .map(Ok)
        .chain(outcome.errors.iter().cloned().map(Err))
        .collect();
    let _ = config.events.send(WorkerEvent {
        id: config.id,
        messages,
        console: outcome.console,
        errors: outcome.errors,
        closed: outcome.closed,
    });
}

fn emit_error(config: &WorkerConfig, error: String) {
    let _ = config.events.send(WorkerEvent {
        id: config.id,
        messages: vec![Err(error.clone())],
        console: Vec::new(),
        errors: vec![error],
        closed: true,
    });
}
