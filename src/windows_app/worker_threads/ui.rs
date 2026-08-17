use super::*;

impl BrowserState {
    pub(in crate::windows_app) unsafe fn apply_worker_actions(
        &mut self,
        actions: Vec<ScriptWorkerAction>,
    ) {
        for action in actions {
            match action {
                ScriptWorkerAction::Start {
                    id,
                    url,
                    kind,
                    name,
                    credentials,
                } => {
                    let result = if self.workers.len() >= MAX_DEDICATED_WORKERS_PER_TAB {
                        Err(format!(
                            "dedicated Worker limit of {MAX_DEDICATED_WORKERS_PER_TAB} was reached"
                        ))
                    } else {
                        self.start_worker(id, url, kind, name, credentials)
                    };
                    if let Err(error) = result {
                        self.finish_worker_event(WorkerEventMessage::error(
                            self.generation,
                            id,
                            error,
                        ));
                    }
                }
                ScriptWorkerAction::PostMessage { id, serialized } => {
                    if let Some(worker) = self.workers.get(&id) {
                        let _ = worker.commands.send(WorkerCommand::Message(serialized));
                    }
                }
                ScriptWorkerAction::Terminate { id } => {
                    if let Some(worker) = self.workers.remove(&id) {
                        worker.terminate();
                    }
                }
            }
        }
    }

    fn start_worker(
        &mut self,
        id: u32,
        url: String,
        kind: better_web_browser::engine::ScriptKind,
        name: String,
        credentials: CredentialsMode,
    ) -> Result<(), String> {
        let lifetime = FetchController::new();
        let signal = lifetime.signal();
        let (commands, receiver) = mpsc::channel();
        let worker_commands = commands.clone();
        let client = Arc::clone(&self.http_client);
        let document_url = self.page.source_url.clone();
        let router = self.app.tab_router.clone();
        let tab_id = self.id;
        let generation = self.generation;
        std::thread::Builder::new()
            .name(if name.is_empty() {
                format!("breeze-worker-{id}")
            } else {
                format!("breeze-worker-{name}")
            })
            .spawn(move || {
                run_worker(WorkerThreadConfig {
                    id,
                    url,
                    name,
                    kind,
                    credentials,
                    document_url,
                    generation,
                    tab_id,
                    router,
                    client,
                    signal,
                    commands: worker_commands,
                    receiver,
                });
            })
            .map_err(|error| format!("could not start dedicated Worker: {error}"))?;
        self.workers.insert(id, WorkerHandle { commands, lifetime });
        Ok(())
    }

    pub(in crate::windows_app) unsafe fn finish_worker_event(
        &mut self,
        message: WorkerEventMessage,
    ) {
        if message.generation != self.generation || self.script_runtime.is_none() {
            return;
        }
        if message.closed
            && let Some(worker) = self.workers.remove(&message.id)
        {
            worker.terminate();
        }
        let client = Arc::clone(&self.http_client);
        let document_url = self.page.source_url.clone();
        let fetch_signal = self.document_fetch.signal();
        let mut resource_budget = self.page_resource_budget.saturating_sub(message.bytes);
        let mut bytes = message.bytes;
        let mut dynamic_network_time = Duration::ZERO;
        let mut processing_time = Duration::ZERO;
        let started = Instant::now();
        let document_root = self.page.dom.document.id();
        let mut outcome = ScriptOutcome::default();
        {
            let Some(runtime) = self.script_runtime.as_mut() else {
                return;
            };
            let mut loader = |url: &str, kind| -> Result<String, String> {
                let request_started = Instant::now();
                let response = crate::windows_app::resources::fetch_script_source(
                    &client,
                    &fetch_signal,
                    &document_url,
                    url,
                    kind,
                )
                .map_err(|error| error.to_string());
                dynamic_network_time += request_started.elapsed();
                let response = response?;
                if !response.is_success() {
                    return Err(format!("server returned HTTP {}", response.status));
                }
                let size = response.body.len() as u64;
                if size > resource_budget {
                    return Err("page resource budget was exhausted".into());
                }
                let processing_started = Instant::now();
                let code = winhttp::decode_text(response.body.as_bytes(), response.content_type());
                processing_time += processing_started.elapsed();
                bytes += size;
                resource_budget -= size;
                Ok(code)
            };
            for event in message.events {
                crate::windows_app::async_scripts::merge_script_outcome(
                    &mut outcome,
                    runtime.complete_worker_event_with_loader(message.id, event, Some(&mut loader)),
                    document_root,
                );
            }
        }
        outcome.console.extend(
            message
                .console
                .into_iter()
                .map(|entry| format!("Worker {}: {entry}", message.id)),
        );
        outcome.errors.extend(
            message
                .errors
                .into_iter()
                .map(|error| format!("Worker {}: {error}", message.id)),
        );
        self.complete_post_load_script_task(
            outcome,
            crate::windows_app::runtime::PostLoadScriptWork {
                script_time: started.elapsed().saturating_sub(dynamic_network_time),
                network_time: message.network_time + dynamic_network_time,
                processing_time,
                bytes,
                resource_budget,
            },
        );
    }
}
