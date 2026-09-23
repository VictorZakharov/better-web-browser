//! One dedicated worker's asynchronous message, timer, and Fetch event loop.
use super::*;

pub(super) fn run_worker(config: WorkerConfig) {
    let mut source_request = WorkerSourceRequest {
        network: config.network.clone(),
        cancelled: config.cancelled.clone(),
        base_url: config.document_url.clone(),
        credentials: config.credentials,
        creator_client: config.client,
        worker_client: config.worker_client,
    };
    let response = worker_source_request(&source_request, &config.url, config.kind, true);
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
    // External Worker globals use the entry response's policy, not the
    // creator document's policy. The creator still gates this entry request
    // through worker-src in the browser network process.
    let response_url = response
        .url_list
        .last()
        .map_or(config.url.as_str(), |url| url.as_str());
    let policy =
        match crate::fetch::csp::PolicyContainer::from_headers(response_url, &response.headers) {
            Ok(policy) => Arc::new(policy),
            Err(error) => {
                emit_error(&config, error.to_string());
                return;
            }
        };
    let source = crate::winhttp::decode_text(response.body.as_bytes(), response.content_type());
    source_request.base_url = response_url.to_string();
    let import_policy = policy.clone();
    let loader: Arc<WorkerSourceLoader> = Arc::new(move |url, kind| {
        import_policy
            .check_request_with_script(
                crate::fetch::RequestDestination::Script,
                url,
                0,
                Some(&crate::fetch::csp::ScriptSource {
                    nonce: None,
                    parser_inserted: false,
                }),
            )
            .map_err(|error| error.to_string())?;
        let response = worker_source_request(&source_request, url, kind, false)
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
    let (runtime, initial) = WorkerRuntime::start_with_policy(
        &config.url,
        &source,
        &config.name,
        config.kind,
        loader,
        policy,
    );
    let Some(mut runtime) = runtime else {
        emit(&config, initial);
        return;
    };
    if drive_worker_outcome(&config, initial) {
        return;
    }
    let mut last_tick = Instant::now();
    loop {
        let timeout = runtime
            .next_timer_delay()
            .unwrap_or(Duration::from_millis(100))
            .min(Duration::from_millis(100));
        let command = config.commands.recv_timeout(timeout);
        if config.cancelled.load(Ordering::Acquire) {
            break;
        }
        let elapsed = last_tick.elapsed();
        last_tick = Instant::now();
        let timed = runtime.advance_time(elapsed, 64);
        if drive_worker_outcome(&config, timed) {
            break;
        }
        match command {
            Ok(WorkerCommand::Fetch { id, event }) => {
                let fetched = runtime.deliver_fetch_event(id, event);
                if drive_worker_outcome(&config, fetched) {
                    break;
                }
            }
            Ok(WorkerCommand::Message(serialized)) => {
                let message = runtime.dispatch_message(&serialized);
                if drive_worker_outcome(&config, message) {
                    break;
                }
            }
            Ok(WorkerCommand::PortMessage {
                endpoint,
                serialized,
            }) => {
                let event = runtime.dispatch_port_message(endpoint, &serialized);
                if drive_worker_outcome(&config, event) {
                    break;
                }
            }
            Ok(WorkerCommand::PortClose(endpoint)) => {
                let event = runtime.dispatch_port_close(endpoint);
                if drive_worker_outcome(&config, event) {
                    break;
                }
            }
            Ok(WorkerCommand::Terminate) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    runtime.cancel();
}

fn drive_worker_outcome(config: &WorkerConfig, outcome: WorkerRuntimeOutcome) -> bool {
    let closed = outcome.closed || !outcome.errors.is_empty();
    emit(config, outcome);
    closed
}

fn emit(config: &WorkerConfig, mut outcome: WorkerRuntimeOutcome) {
    if config.cancelled.load(Ordering::Acquire) {
        return;
    }
    for action in &mut outcome.fetch_actions {
        if let ScriptFetchAction::Start { request, .. } = action {
            request.client = config.worker_client;
        }
    }
    let messages = outcome
        .messages
        .into_iter()
        .map(Ok)
        .chain(outcome.errors.iter().cloned().map(Err))
        .collect();
    let _ = config.events.send(WorkerEvent {
        id: config.id,
        fetch_actions: outcome.fetch_actions,
        messages,
        port_events: outcome.port_events,
        console: outcome.console,
        closed: outcome.closed || !outcome.errors.is_empty(),
        errors: outcome.errors,
    });
}

fn emit_error(config: &WorkerConfig, error: String) {
    if config.cancelled.load(Ordering::Acquire) {
        return;
    }
    let _ = config.events.send(WorkerEvent {
        id: config.id,
        fetch_actions: Vec::new(),
        messages: vec![Err(error.clone())],
        port_events: Vec::new(),
        console: Vec::new(),
        errors: vec![error],
        closed: true,
    });
}
