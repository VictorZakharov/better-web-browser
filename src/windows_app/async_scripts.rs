//! Non-render-blocking external classic-script fetch and delivery.

use super::browser_app::TabMessageRouter;
use super::resources::fetch_document_resource;
use super::tabs::TabId;
use super::*;
use better_web_browser::engine::script::ScriptInput;
use better_web_browser::fetch::{FetchSignal, RequestDestination};

// WinHTTP bounds each response at 16 MiB. Keep the in-flight set small so a hostile page cannot
// multiply that per-response allowance into hundreds of transient MiB before UI-thread budgeting.
const MAX_PARALLEL_ASYNC_SCRIPT_FETCHES: usize = 4;

#[derive(Clone)]
struct AsyncScriptRequest {
    source_url: String,
    node_ids: Vec<u128>,
}

struct FetchedAsyncScript {
    code: String,
    bytes: u64,
}

pub(super) struct AsyncScriptMessage {
    generation: u64,
    request: AsyncScriptRequest,
    result: Result<FetchedAsyncScript, String>,
    network_time: Duration,
    processing_time: Duration,
}

impl BrowserState {
    pub(super) unsafe fn begin_async_scripts(&mut self) {
        if self.script_runtime.is_none() {
            return;
        }
        let mut requests: Vec<AsyncScriptRequest> = Vec::new();
        for script in self
            .page
            .scripts
            .iter()
            .filter(|script| !script.blocks_first_paint && script.code.is_none())
        {
            if let Some(existing) = requests
                .iter_mut()
                .find(|request| request.source_url == script.source_url)
            {
                existing.node_ids.push(script.node.id().to_wire());
            } else {
                requests.push(AsyncScriptRequest {
                    source_url: script.source_url.clone(),
                    node_ids: vec![script.node.id().to_wire()],
                });
            }
        }
        if requests.is_empty() {
            return;
        }

        let generation = self.generation;
        let tab_id = self.id;
        let tab_router = self.app.tab_router.clone();
        let client = Arc::clone(&self.http_client);
        let fetch_signal = self.document_fetch.signal();
        let document_url = self.page.source_url.clone();
        let worker = std::thread::Builder::new()
            .name("breeze-async-scripts".into())
            .spawn(move || {
                for batch in requests.chunks(MAX_PARALLEL_ASYNC_SCRIPT_FETCHES) {
                    std::thread::scope(|scope| {
                        for request in batch.iter().cloned() {
                            let client = Arc::clone(&client);
                            let signal = fetch_signal.clone();
                            let document_url = &document_url;
                            let tab_router = tab_router.clone();
                            scope.spawn(move || {
                                let message = fetch_async_script(
                                    &client,
                                    &signal,
                                    document_url,
                                    generation,
                                    request,
                                );
                                post_async_script(&tab_router, tab_id, message);
                            });
                        }
                    });
                }
            });
        if let Err(error) = worker {
            self.set_status(&format!("Could not start async scripts: {error}"));
        }
    }

    pub(super) unsafe fn finish_async_script(&mut self, message: AsyncScriptMessage) {
        if message.generation != self.generation || self.script_runtime.is_none() {
            return;
        }

        let mut resource_budget = self.page_resource_budget;
        let mut additional_bytes = 0_u64;
        let mut inputs = Vec::new();
        let mut outcome = ScriptOutcome::default();
        match message.result {
            Ok(fetched) if fetched.bytes <= resource_budget => {
                resource_budget -= fetched.bytes;
                additional_bytes += fetched.bytes;
                self.loaded_page_resources.insert(PageResource::Script {
                    url: message.request.source_url.clone(),
                });
                self.page
                    .add_script(&message.request.source_url, fetched.code.clone());
                for node_id in &message.request.node_ids {
                    if let Some(script) = self
                        .page
                        .scripts
                        .iter()
                        .find(|script| script.node.id().to_wire() == *node_id)
                    {
                        inputs.push(ScriptInput {
                            node: script.node.clone(),
                            source_url: message.request.source_url.clone(),
                            code: fetched.code.clone(),
                            finish_lifecycle: false,
                        });
                    }
                }
            }
            Ok(_) => outcome.errors.push(format!(
                "{}: async script skipped because the page resource budget was exhausted",
                message.request.source_url
            )),
            Err(error) => outcome.errors.push(format!(
                "{}: async script could not be loaded: {error}",
                message.request.source_url
            )),
        }

        let client = Arc::clone(&self.http_client);
        let fetch_signal = self.document_fetch.signal();
        let document_url = self.page.source_url.clone();
        let cookie_header = client.document_cookie_header(&document_url);
        let document_root = self.page.dom.document.id();
        let advance = self.take_script_runtime_elapsed();
        let mut dynamic_network_time = Duration::ZERO;
        let mut dynamic_processing_time = Duration::ZERO;
        let script_started = Instant::now();
        {
            let Some(runtime) = self.script_runtime.as_mut() else {
                return;
            };
            if let Ok(header) = &cookie_header {
                runtime.set_document_cookie_header(header);
            }
            let mut dynamic_script_loader = |url: &str| -> Result<String, String> {
                let request_started = Instant::now();
                let response = fetch_document_resource(
                    &client,
                    &fetch_signal,
                    &document_url,
                    url,
                    RequestDestination::Script,
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
                dynamic_processing_time += processing_started.elapsed();
                additional_bytes += size;
                resource_budget -= size;
                Ok(code)
            };
            merge_script_outcome(
                &mut outcome,
                runtime.advance_time_with_loader(
                    advance,
                    MAX_POST_LOAD_TIMER_CALLBACKS,
                    Some(&mut dynamic_script_loader),
                ),
                document_root,
            );
            if !outcome.runtime_stopped && outcome.navigation_url.is_none() && !inputs.is_empty() {
                merge_script_outcome(
                    &mut outcome,
                    runtime
                        .execute_additional_with_loader(&inputs, Some(&mut dynamic_script_loader)),
                    document_root,
                );
            }
        }
        if let Err(error) = cookie_header {
            outcome
                .errors
                .push(format!("document.cookie refresh: {error}"));
        }
        let script_time = script_started
            .elapsed()
            .saturating_sub(dynamic_network_time);
        self.complete_post_load_script_task(
            outcome,
            super::runtime::PostLoadScriptWork {
                script_time,
                network_time: message.network_time + dynamic_network_time,
                processing_time: message.processing_time + dynamic_processing_time,
                bytes: additional_bytes,
                resource_budget,
            },
        );
    }
}

fn fetch_async_script(
    client: &winhttp::HttpClient,
    signal: &FetchSignal,
    document_url: &str,
    generation: u64,
    request: AsyncScriptRequest,
) -> AsyncScriptMessage {
    let request_started = Instant::now();
    let response = fetch_document_resource(
        client,
        signal,
        document_url,
        &request.source_url,
        RequestDestination::Script,
    )
    .map_err(|error| error.to_string());
    let network_time = request_started.elapsed();
    let processing_started = Instant::now();
    let result = response.and_then(|response| {
        if !response.is_success() {
            return Err(format!("server returned HTTP {}", response.status));
        }
        Ok(FetchedAsyncScript {
            bytes: response.body.len() as u64,
            code: winhttp::decode_text(response.body.as_bytes(), response.content_type()),
        })
    });
    AsyncScriptMessage {
        generation,
        request,
        result,
        network_time,
        processing_time: processing_started.elapsed(),
    }
}

fn post_async_script(tab_router: &TabMessageRouter, tab_id: TabId, message: AsyncScriptMessage) {
    let pointer = Box::into_raw(Box::new(message));
    let posted = tab_router.destination(tab_id).is_some_and(|window| unsafe {
        PostMessageW(
            window as Hwnd,
            WM_APP_ASYNC_SCRIPT,
            tab_id.get() as usize,
            pointer as isize,
        ) != 0
    });
    if !posted {
        unsafe { drop(Box::from_raw(pointer)) };
    }
}

fn merge_script_outcome(
    target: &mut ScriptOutcome,
    mut source: ScriptOutcome,
    document_root: better_web_browser::engine::dom::NodeId,
) {
    if source.render_requested && source.invalidation.is_empty() {
        source.diagnostics.push(
            "render request had no invalidation root; using the full-document fallback".into(),
        );
        source.invalidation =
            better_web_browser::engine::invalidation::RenderInvalidation::full(document_root);
    }
    target
        .invalidation
        .merge_conservatively(source.invalidation, document_root);
    target.executed += source.executed;
    target.mutation_count += source.mutation_count;
    target.errors.append(&mut source.errors);
    target.console.append(&mut source.console);
    target.diagnostics.append(&mut source.diagnostics);
    if source.navigation_url.is_some() {
        target.navigation_url = source.navigation_url;
    }
    target.cookie_updates.append(&mut source.cookie_updates);
    target.runtime_stopped |= source.runtime_stopped;
    target.render_requested |= source.render_requested;
}
