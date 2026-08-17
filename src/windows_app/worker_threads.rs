//! Dedicated-worker threads, commands, Fetch work, and UI-thread event delivery.

use super::browser_app::TabMessageRouter;
use super::tabs::TabId;
use super::*;
use better_web_browser::fetch::{
    CredentialsMode, FetchController, FetchError, FetchRequest, FetchResponse, RequestMode,
};
use std::sync::{Mutex, mpsc};

mod ui;

const MAX_DEDICATED_WORKERS_PER_TAB: usize = 16;

pub(super) struct WorkerHandle {
    commands: mpsc::Sender<WorkerCommand>,
    lifetime: FetchController,
}

impl WorkerHandle {
    pub(super) fn terminate(self) {
        self.lifetime.abort();
        let _ = self.commands.send(WorkerCommand::Terminate);
    }
}

enum WorkerCommand {
    Message(String),
    FetchComplete {
        id: u32,
        result: Result<FetchResponse, FetchError>,
        network_time: Duration,
        bytes: u64,
    },
    Terminate,
}

pub(super) struct WorkerEventMessage {
    generation: u64,
    id: u32,
    events: Vec<Result<String, String>>,
    console: Vec<String>,
    errors: Vec<String>,
    closed: bool,
    network_time: Duration,
    bytes: u64,
}

#[derive(Default)]
struct WorkerLoadStats {
    network_time: Duration,
    bytes: u64,
}

struct WorkerThreadConfig {
    id: u32,
    url: String,
    name: String,
    kind: better_web_browser::engine::ScriptKind,
    credentials: CredentialsMode,
    document_url: String,
    generation: u64,
    tab_id: TabId,
    router: TabMessageRouter,
    client: Arc<winhttp::HttpClient>,
    signal: better_web_browser::fetch::FetchSignal,
    commands: mpsc::Sender<WorkerCommand>,
    receiver: mpsc::Receiver<WorkerCommand>,
}

fn run_worker(config: WorkerThreadConfig) {
    let entry_started = Instant::now();
    let response = fetch_worker_source(
        &config.client,
        &config.signal,
        &config.document_url,
        &config.url,
        config.kind,
        config.credentials,
    );
    let entry_network_time = entry_started.elapsed();
    let response = match response {
        Ok(response) if response.is_success() => response,
        Ok(response) => {
            post_worker_event(
                &config,
                WorkerEventMessage::error(
                    config.generation,
                    config.id,
                    format!("entry script returned HTTP {}", response.status),
                ),
            );
            return;
        }
        Err(error) => {
            post_worker_event(
                &config,
                WorkerEventMessage::error(config.generation, config.id, error.to_string()),
            );
            return;
        }
    };
    let entry_bytes = response.body.len() as u64;
    let source = winhttp::decode_text(response.body.as_bytes(), response.content_type());
    let stats = Arc::new(Mutex::new(WorkerLoadStats::default()));
    let loader = worker_source_loader(&config, Arc::clone(&stats));
    let (runtime, initial) =
        WorkerRuntime::start(&config.url, &source, &config.name, config.kind, loader);
    let Some(mut runtime) = runtime else {
        emit_outcome(&config, initial, entry_network_time, entry_bytes);
        return;
    };
    let mut fetches = HashMap::new();
    if process_outcome(
        &config,
        initial,
        &mut fetches,
        entry_network_time,
        entry_bytes,
    ) {
        return;
    }
    let initial_dependencies = take_worker_stats(&stats);
    if initial_dependencies.bytes > 0 || !initial_dependencies.network_time.is_zero() {
        post_worker_event(
            &config,
            WorkerEventMessage::metrics(
                config.generation,
                config.id,
                initial_dependencies.network_time,
                initial_dependencies.bytes,
            ),
        );
    }
    let mut last_tick = Instant::now();
    loop {
        let timeout = runtime
            .next_timer_delay()
            .unwrap_or(Duration::from_millis(100))
            .min(Duration::from_millis(100));
        let command = config.receiver.recv_timeout(timeout);
        let elapsed = last_tick.elapsed();
        last_tick = Instant::now();
        let timed = runtime.advance_time(elapsed, 64);
        if process_outcome(&config, timed, &mut fetches, Duration::ZERO, 0) {
            break;
        }
        let outcome = match command {
            Ok(WorkerCommand::Message(serialized)) => runtime.dispatch_message(&serialized),
            Ok(WorkerCommand::FetchComplete {
                id,
                result,
                network_time,
                bytes,
            }) => {
                fetches.remove(&id);
                let outcome = runtime.complete_fetch(id, result);
                if process_outcome(&config, outcome, &mut fetches, network_time, bytes) {
                    break;
                }
                continue;
            }
            Ok(WorkerCommand::Terminate) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
        };
        if process_outcome(&config, outcome, &mut fetches, Duration::ZERO, 0) {
            break;
        }
        let drained = take_worker_stats(&stats);
        if drained.bytes > 0 || !drained.network_time.is_zero() {
            post_worker_event(
                &config,
                WorkerEventMessage::metrics(
                    config.generation,
                    config.id,
                    drained.network_time,
                    drained.bytes,
                ),
            );
        }
    }
    for (_, controller) in fetches {
        controller.abort();
    }
    runtime.cancel();
}

fn process_outcome(
    config: &WorkerThreadConfig,
    mut outcome: WorkerRuntimeOutcome,
    fetches: &mut HashMap<u32, FetchController>,
    network_time: Duration,
    bytes: u64,
) -> bool {
    for action in std::mem::take(&mut outcome.fetch_actions) {
        match action {
            ScriptFetchAction::Start { id, request } => {
                let controller = FetchController::new();
                let request = (*request).with_signal(controller.signal());
                let client = Arc::clone(&config.client);
                let commands = config.commands.clone();
                let worker = std::thread::Builder::new()
                    .name("breeze-worker-fetch".into())
                    .spawn(move || {
                        let started = Instant::now();
                        let result = client.fetch(request);
                        let bytes = result
                            .as_ref()
                            .map(|response| response.body.len() as u64)
                            .unwrap_or_default();
                        let _ = commands.send(WorkerCommand::FetchComplete {
                            id,
                            result,
                            network_time: started.elapsed(),
                            bytes,
                        });
                    });
                match worker {
                    Ok(_) => {
                        fetches.insert(id, controller);
                    }
                    Err(error) => {
                        let _ = config.commands.send(WorkerCommand::FetchComplete {
                            id,
                            result: Err(better_web_browser::fetch::FetchError::new(
                                better_web_browser::fetch::FetchErrorKind::Network,
                                format!("could not start Worker Fetch: {error}"),
                            )),
                            network_time: Duration::ZERO,
                            bytes: 0,
                        });
                    }
                }
            }
            ScriptFetchAction::Abort { id } => {
                if let Some(controller) = fetches.get(&id) {
                    controller.abort();
                }
            }
        }
    }
    let closed = outcome.closed || !outcome.errors.is_empty();
    emit_outcome(config, outcome, network_time, bytes);
    closed
}

fn emit_outcome(
    config: &WorkerThreadConfig,
    outcome: WorkerRuntimeOutcome,
    network_time: Duration,
    bytes: u64,
) {
    let closed = outcome.closed || !outcome.errors.is_empty();
    let events = outcome
        .messages
        .iter()
        .cloned()
        .map(Ok)
        .chain(outcome.errors.iter().cloned().map(Err))
        .collect();
    post_worker_event(
        config,
        WorkerEventMessage {
            generation: config.generation,
            id: config.id,
            events,
            console: outcome.console,
            errors: outcome.errors,
            closed,
            network_time,
            bytes,
        },
    );
}

fn worker_source_loader(
    config: &WorkerThreadConfig,
    stats: Arc<Mutex<WorkerLoadStats>>,
) -> Arc<WorkerSourceLoader> {
    let client = Arc::clone(&config.client);
    let signal = config.signal.clone();
    let document_url = config.document_url.clone();
    let credentials = config.credentials;
    Arc::new(move |url, kind| {
        let started = Instant::now();
        let response = fetch_worker_source(&client, &signal, &document_url, url, kind, credentials)
            .map_err(|error| error.to_string())?;
        let mut stats = stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        stats.network_time += started.elapsed();
        stats.bytes += response.body.len() as u64;
        if !response.is_success() {
            return Err(format!("server returned HTTP {}", response.status));
        }
        Ok(winhttp::decode_text(
            response.body.as_bytes(),
            response.content_type(),
        ))
    })
}

fn fetch_worker_source(
    client: &winhttp::HttpClient,
    signal: &better_web_browser::fetch::FetchSignal,
    document_url: &str,
    url: &str,
    kind: better_web_browser::engine::ScriptKind,
    credentials: CredentialsMode,
) -> Result<FetchResponse, FetchError> {
    let mut request = FetchRequest::script(url, document_url)?;
    request.mode = match kind {
        better_web_browser::engine::ScriptKind::Classic => RequestMode::SameOrigin,
        better_web_browser::engine::ScriptKind::Module => RequestMode::Cors,
    };
    request.credentials = credentials;
    let response = client.fetch(request.with_signal(signal.clone()))?;
    super::resources::validate_script_response(&response, kind)?;
    Ok(response)
}

fn take_worker_stats(stats: &Mutex<WorkerLoadStats>) -> WorkerLoadStats {
    std::mem::take(
        &mut *stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    )
}

fn post_worker_event(config: &WorkerThreadConfig, message: WorkerEventMessage) {
    let pointer = Box::into_raw(Box::new(message));
    let posted = config
        .router
        .destination(config.tab_id)
        .is_some_and(|window| unsafe {
            PostMessageW(
                window as Hwnd,
                WM_APP_WORKER_EVENT,
                config.tab_id.get() as usize,
                pointer as isize,
            ) != 0
        });
    if !posted {
        unsafe { drop(Box::from_raw(pointer)) };
    }
}

impl WorkerEventMessage {
    fn error(generation: u64, id: u32, error: String) -> Self {
        Self {
            generation,
            id,
            events: vec![Err(error.clone())],
            console: Vec::new(),
            errors: vec![error],
            closed: true,
            network_time: Duration::ZERO,
            bytes: 0,
        }
    }

    fn metrics(generation: u64, id: u32, network_time: Duration, bytes: u64) -> Self {
        Self {
            generation,
            id,
            events: Vec::new(),
            console: Vec::new(),
            errors: Vec::new(),
            closed: false,
            network_time,
            bytes,
        }
    }
}
