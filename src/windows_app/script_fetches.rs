//! Asynchronous JavaScript Fetch work owned and completed by one browser tab.

use super::browser_app::TabMessageRouter;
use super::resources::fetch_script_source;
use super::tabs::TabId;
use super::*;
use better_web_browser::fetch::{
    FetchController, FetchError, FetchErrorKind, FetchRequest, FetchResponse,
};

const MAX_PARALLEL_SCRIPT_FETCHES: usize = 8;
const MAX_QUEUED_SCRIPT_FETCHES: usize = 64;

pub(super) struct ScriptFetchMessage {
    generation: u64,
    id: u32,
    result: Result<FetchResponse, FetchError>,
    network_time: Duration,
}

impl BrowserState {
    pub(super) unsafe fn apply_script_fetch_actions(&mut self, actions: Vec<ScriptFetchAction>) {
        let mut failures = Vec::new();
        for action in actions {
            match action {
                ScriptFetchAction::Abort { id } => {
                    if let Some(controller) = self.script_fetches.get(&id) {
                        controller.abort();
                    } else if let Some(position) = self
                        .queued_script_fetches
                        .iter()
                        .position(|(queued_id, _)| *queued_id == id)
                    {
                        self.queued_script_fetches.remove(position);
                    }
                }
                ScriptFetchAction::Start { id, request }
                    if self.script_fetches.len() < MAX_PARALLEL_SCRIPT_FETCHES =>
                {
                    if let Some(error) = self.start_script_fetch(id, *request) {
                        failures.push(error);
                    }
                }
                ScriptFetchAction::Start { id, request }
                    if self.queued_script_fetches.len() < MAX_QUEUED_SCRIPT_FETCHES =>
                {
                    self.queued_script_fetches.push_back((id, *request));
                }
                ScriptFetchAction::Start { id, .. } => failures.push(ScriptFetchMessage {
                    generation: self.generation,
                    id,
                    result: Err(FetchError::new(
                        FetchErrorKind::Network,
                        format!(
                            "script Fetch queue reached its {MAX_QUEUED_SCRIPT_FETCHES}-request limit"
                        ),
                    )),
                    network_time: Duration::ZERO,
                }),
            }
        }
        for failure in failures {
            self.finish_script_fetch(failure);
        }
    }

    fn start_script_fetch(&mut self, id: u32, request: FetchRequest) -> Option<ScriptFetchMessage> {
        let controller = FetchController::new();
        let request = request.with_signal(controller.signal());
        self.script_fetches.insert(id, controller);
        let generation = self.generation;
        let tab_id = self.id;
        let router = self.app.tab_router.clone();
        let client = Arc::clone(&self.http_client);
        let worker = std::thread::Builder::new()
            .name("breeze-script-fetch".into())
            .spawn(move || {
                let started = Instant::now();
                let result = client.fetch(request);
                post_script_fetch(
                    &router,
                    tab_id,
                    ScriptFetchMessage {
                        generation,
                        id,
                        result,
                        network_time: started.elapsed(),
                    },
                );
            });
        worker.err().map(|error| {
            self.script_fetches.remove(&id);
            ScriptFetchMessage {
                generation,
                id,
                result: Err(FetchError::new(
                    FetchErrorKind::Network,
                    format!("could not start script Fetch worker: {error}"),
                )),
                network_time: Duration::ZERO,
            }
        })
    }

    pub(super) unsafe fn finish_script_fetch(&mut self, mut message: ScriptFetchMessage) {
        if message.generation != self.generation || self.script_runtime.is_none() {
            return;
        }
        self.script_fetches.remove(&message.id);
        let response_bytes = message
            .result
            .as_ref()
            .map(|response| response.body.len() as u64)
            .unwrap_or_default();
        if response_bytes > self.page_resource_budget {
            message.result = Err(FetchError::new(
                FetchErrorKind::BodyTooLarge,
                "script Fetch exceeded the document resource budget",
            ));
        }

        let client = Arc::clone(&self.http_client);
        let document_url = self.page.source_url.clone();
        let fetch_signal = self.document_fetch.signal();
        let cookie_header = client.document_cookie_header(&document_url);
        let mut resource_budget = self.page_resource_budget.saturating_sub(response_bytes);
        let mut bytes = response_bytes;
        let mut dynamic_network_time = Duration::ZERO;
        let mut processing_time = Duration::ZERO;
        let started = Instant::now();
        let mut outcome = {
            let Some(runtime) = self.script_runtime.as_mut() else {
                return;
            };
            if let Ok(header) = &cookie_header {
                runtime.set_document_cookie_header(header);
            }
            let mut dynamic_script_loader = |url: &str, kind| -> Result<String, String> {
                let request_started = Instant::now();
                let response =
                    fetch_script_source(&client, &fetch_signal, &document_url, url, kind)
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
            runtime.complete_fetch_with_loader(
                message.id,
                message.result,
                Some(&mut dynamic_script_loader),
            )
        };
        if let Err(error) = cookie_header {
            outcome
                .errors
                .push(format!("document.cookie refresh: {error}"));
        }
        let script_time = started.elapsed().saturating_sub(dynamic_network_time);
        self.complete_post_load_script_task(
            outcome,
            super::runtime::PostLoadScriptWork {
                script_time,
                network_time: message.network_time + dynamic_network_time,
                processing_time,
                bytes,
                resource_budget,
            },
        );
        self.start_queued_script_fetches();
    }

    fn start_queued_script_fetches(&mut self) {
        while self.script_fetches.len() < MAX_PARALLEL_SCRIPT_FETCHES {
            let Some((id, request)) = self.queued_script_fetches.pop_front() else {
                return;
            };
            if let Some(message) = self.start_script_fetch(id, request) {
                unsafe { self.finish_script_fetch(message) };
            }
        }
    }
}

fn post_script_fetch(router: &TabMessageRouter, tab_id: TabId, message: ScriptFetchMessage) {
    let pointer = Box::into_raw(Box::new(message));
    let posted = router.destination(tab_id).is_some_and(|window| unsafe {
        PostMessageW(
            window as Hwnd,
            WM_APP_SCRIPT_FETCH,
            tab_id.get() as usize,
            pointer as isize,
        ) != 0
    });
    if !posted {
        unsafe { drop(Box::from_raw(pointer)) };
    }
}
