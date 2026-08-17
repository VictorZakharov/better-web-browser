//! Dedicated Worker realms hosted inside the document's AppContainer process.

use super::fetch::{into_fetch_result, script_api_request, validate_script_response};
use super::{fetch_script_source, merge_outcome};
use crate::engine::{
    ScriptKind, ScriptOutcome, ScriptRuntime, ScriptWorkerAction, WorkerRuntime,
    WorkerRuntimeOutcome, WorkerSourceLoader,
};
use crate::fetch::{
    CredentialsMode, FetchError, FetchErrorKind, FetchRequest, FetchResponse, RequestMode,
};
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::DocumentId;
use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const MAX_DEDICATED_WORKERS: usize = 16;
const MAX_WORKER_SERVICE_WAVES: usize = 64;

pub(super) struct RendererWorkers {
    handles: HashMap<u32, WorkerHandle>,
    network_sender: mpsc::Sender<WorkerNetworkRequest>,
    network: mpsc::Receiver<WorkerNetworkRequest>,
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
            event_sender,
            events,
        }
    }

    pub(super) fn has_work(&self) -> bool {
        !self.handles.is_empty()
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
        let mut delivered = false;
        for wave in 0..MAX_WORKER_SERVICE_WAVES {
            let wait = if wave == 0 && self.has_work() {
                Duration::from_millis(10)
            } else {
                Duration::from_millis(2)
            };
            let mut requests = Vec::new();
            match self.network.recv_timeout(wait) {
                Ok(request) => requests.push(request),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("renderer Worker network channel disconnected".into());
                }
            }
            requests.extend(self.network.try_iter());
            let had_requests = !requests.is_empty();
            if had_requests {
                self.complete_network_batch(connection, document, requests)?;
            }

            let events = self.events.try_iter().collect::<Vec<_>>();
            let had_events = !events.is_empty();
            for event in events {
                delivered = true;
                if event.closed
                    && let Some(handle) = self.handles.remove(&event.id)
                {
                    handle.terminate();
                }
                let Some(runtime) = runtime.as_mut() else {
                    continue;
                };
                let mut loader =
                    |url: &str, kind| fetch_script_source(connection, document, url, kind);
                for message in event.messages {
                    let worker = runtime.complete_worker_event_with_loader(
                        event.id,
                        message,
                        Some(&mut loader),
                    );
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
            let had_actions = !actions.is_empty();
            if had_actions {
                self.apply(actions, document_url, outcome)?;
                continue;
            }
            if !had_requests && !had_events {
                break;
            }
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

    fn complete_network_batch(
        &mut self,
        connection: &mut ChildConnection,
        document: DocumentId,
        requests: Vec<WorkerNetworkRequest>,
    ) -> Result<(), String> {
        let mut replies = HashMap::new();
        let wire = requests
            .into_iter()
            .map(|request| {
                let request_id = connection.allocate_request_id();
                replies.insert(request_id, request.reply);
                script_api_request(request_id, document, request.request)
            })
            .collect::<Vec<_>>();
        for response in connection.fetch_batch(document, wire)? {
            let Some(reply) = replies.remove(&response.head.request_id) else {
                return Err("browser returned an unknown Worker Fetch response".into());
            };
            let _ = reply.send(into_fetch_result(response));
        }
        for (_, reply) in replies {
            let _ = reply.send(Err(FetchError::new(
                FetchErrorKind::Network,
                "browser omitted a Worker Fetch response",
            )));
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

struct WorkerNetworkRequest {
    request: FetchRequest,
    reply: mpsc::Sender<Result<FetchResponse, FetchError>>,
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

fn worker_source_request(
    network: &mpsc::Sender<WorkerNetworkRequest>,
    document_url: &str,
    url: &str,
    kind: ScriptKind,
    credentials: CredentialsMode,
) -> Result<FetchResponse, FetchError> {
    let mut request = FetchRequest::script(url, document_url)?;
    request.mode = match kind {
        ScriptKind::Classic => RequestMode::SameOrigin,
        ScriptKind::Module => RequestMode::Cors,
    };
    request.credentials = credentials;
    request_network(network, request)
}

fn request_network(
    network: &mpsc::Sender<WorkerNetworkRequest>,
    request: FetchRequest,
) -> Result<FetchResponse, FetchError> {
    let (reply, response) = mpsc::channel();
    network
        .send(WorkerNetworkRequest { request, reply })
        .map_err(|_| FetchError::new(FetchErrorKind::Network, "Worker broker disconnected"))?;
    response
        .recv()
        .map_err(|_| FetchError::new(FetchErrorKind::Network, "Worker broker stopped"))?
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
