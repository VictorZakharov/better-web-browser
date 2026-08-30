//! Renderer-side resource installation and script-network completions.

mod streaming;

use super::fetch::{into_fetch_result, page_resource_request, validate_script_response};
use super::{DocumentRuntime, merge_outcome};
use crate::engine::script::ScriptInput;
use crate::engine::{Page, PageResource, ScriptKind, ScriptOutcome};
use crate::limits::bounded_utf8_prefix;
use crate::renderer_process::child::connection::{ChildConnection, PendingFetchBatch};
use crate::renderer_protocol::{BrowserFetchResponse, DocumentId};
use std::collections::HashMap;

const MAX_RESOURCE_DIAGNOSTICS: usize = 32;
const MAX_RESOURCE_DIAGNOSTIC_BYTES: usize = 512;

pub(super) struct PendingResourceFetch {
    batch: PendingFetchBatch,
    by_request: HashMap<u64, PageResource>,
}

impl PendingResourceFetch {
    fn contains(&self, resource: &PageResource) -> bool {
        self.by_request.values().any(|pending| pending == resource)
    }
}

pub(super) fn start_resource_preloads(
    connection: &mut ChildConnection,
    document: DocumentId,
    first_paint: Vec<PageResource>,
    deferred: Vec<PageResource>,
) -> Result<(Option<PendingResourceFetch>, Option<PendingResourceFetch>), String> {
    let (mut requests, first_by_request) = resource_requests(connection, document, first_paint);
    let first_ids = first_by_request.keys().copied().collect();
    let (deferred_requests, deferred_by_request) =
        resource_requests(connection, document, deferred);
    requests.extend(deferred_requests);
    let Some(batch) = connection.start_fetch_batch(document, requests)? else {
        return Ok((None, None));
    };
    let (first_batch, deferred_batch) = batch.split(first_ids)?;
    Ok((
        first_batch.map(|batch| PendingResourceFetch {
            batch,
            by_request: first_by_request,
        }),
        deferred_batch.map(|batch| PendingResourceFetch {
            batch,
            by_request: deferred_by_request,
        }),
    ))
}

pub(super) fn discard_resource_preloads(
    connection: &mut ChildConnection,
    pending: PendingResourceFetch,
) -> Result<(), String> {
    connection.finish_fetch_batch(pending.batch).map(|_| ())
}

impl DocumentRuntime {
    pub(super) fn fetch_resources(
        &mut self,
        connection: &mut ChildConnection,
        include: impl Fn(&Page, &PageResource) -> bool,
    ) -> Result<bool, String> {
        let resources = self
            .page
            .resources
            .iter()
            .filter(|resource| !self.loaded_resources.contains(*resource))
            .filter(|resource| include(&self.page, resource))
            .cloned()
            .collect::<Vec<_>>();
        if resources.is_empty() {
            return Ok(false);
        }
        let (requests, mut by_request) = resource_requests(connection, self.id, resources);
        let responses = connection.fetch_batch(self.id, requests)?;
        self.install_resource_responses(connection, responses, &mut by_request, false)
    }

    pub(super) fn start_presentational_preloads(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        if self.pending_resource_preloads.is_some() {
            return Ok(());
        }
        let resources = self
            .page
            .resources
            .iter()
            .filter(|resource| !self.loaded_resources.contains(*resource))
            .filter(|resource| is_presentational_resource(resource))
            .cloned()
            .collect::<Vec<_>>();
        let (requests, by_request) = resource_requests(connection, self.id, resources);
        let Some(batch) = connection.start_fetch_batch(self.id, requests)? else {
            return Ok(());
        };
        self.pending_resource_preloads = Some(PendingResourceFetch { batch, by_request });
        Ok(())
    }

    pub(in crate::renderer_process::child) fn finish_completed_resource_preloads(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<Option<crate::renderer_protocol::RendererPresentation>, String> {
        let Some(changed) = self.finish_ready_resource_preloads(connection)? else {
            return Ok(None);
        };
        if !changed {
            return Ok(None);
        }
        let mut outcome = std::mem::take(&mut self.pending_media_outcome);
        self.apply_media_actions(&mut outcome, connection)?;
        connection.send_state_mutations(self.id, &mut outcome)?;
        self.text.register_web_fonts(&self.page.fonts);
        let style = self
            .page
            .refresh_resources_for_viewport(self.viewport.style_width, self.viewport.height);
        self.start_presentational_preloads(connection)?;
        let layout_started = std::time::Instant::now();
        self.rebuild_layout();
        let load = self
            .text
            .finish_load_report(crate::renderer_protocol::PageLoadReport {
                layout_micros: super::micros(layout_started.elapsed()),
                ..crate::renderer_protocol::PageLoadReport::default()
            });
        self.presentation(outcome, style, load).map(Some)
    }

    pub(super) fn finish_ready_resource_preloads(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<Option<bool>, String> {
        let Some(mut pending) = self.pending_resource_preloads.take() else {
            return Ok(None);
        };
        let responses = connection.take_ready_fetch_batch(&mut pending.batch)?;
        if responses.is_empty() {
            self.pending_resource_preloads = Some(pending);
            return Ok(None);
        }
        let changed =
            self.install_resource_responses(connection, responses, &mut pending.by_request, true)?;
        if !pending.batch.is_empty() {
            self.pending_resource_preloads = Some(pending);
        }
        Ok(Some(changed))
    }

    pub(super) fn finish_resource_preloads(
        &mut self,
        connection: &mut ChildConnection,
        pending: PendingResourceFetch,
    ) -> Result<bool, String> {
        let responses = connection.finish_fetch_batch(pending.batch)?;
        let mut by_request = pending.by_request;
        self.install_resource_responses(connection, responses, &mut by_request, true)
    }

    fn install_resource_responses(
        &mut self,
        connection: &mut ChildConnection,
        responses: Vec<BrowserFetchResponse>,
        by_request: &mut HashMap<u64, PageResource>,
        require_authoritative_match: bool,
    ) -> Result<bool, String> {
        let mut retained = false;
        for response in responses {
            let Some(resource) = by_request.remove(&response.head.request_id) else {
                return Err("browser returned an unknown resource request".into());
            };
            let label = resource_label(&resource);
            if require_authoritative_match && !self.page.resources.contains(&resource) {
                continue;
            }
            self.loaded_resources.insert(resource.clone());
            let response = match into_fetch_result(response) {
                Ok(response) => response,
                Err(error) => {
                    self.record_resource_diagnostic(format!("{label}: {error}"));
                    continue;
                }
            };
            if !response.is_success() {
                self.record_resource_diagnostic(format!(
                    "{label}: server returned HTTP {}",
                    response.status
                ));
                continue;
            }
            if let PageResource::Script { kind, .. } = &resource
                && let Err(error) = validate_script_response(&response, *kind)
            {
                self.record_resource_diagnostic(format!("{label}: {error}"));
                continue;
            }
            let size = response.body.len() as u64;
            if size > self.resource_budget {
                self.record_resource_diagnostic(format!(
                    "{label}: skipped {size} bytes because only {} page-resource bytes remain",
                    self.resource_budget
                ));
                continue;
            }
            let content_type = response.content_type().map(str::to_string);
            let bytes = response.body.into_bytes();
            let installed = match resource {
                PageResource::Stylesheet { url } => self
                    .page
                    .add_stylesheet_from(
                        &url,
                        crate::winhttp::decode_text(&bytes, content_type.as_deref()),
                    )
                    .then_some(())
                    .ok_or_else(|| "stylesheet was not installed".to_string()),
                PageResource::Image { url } => self.page.add_image(url, &bytes),
                PageResource::Media { node, .. } => connection
                    .decode_media(&bytes)
                    .and_then(|decode| self.install_media_decode(node, decode)),
                PageResource::Script {
                    url,
                    kind,
                    fetch_options,
                } => self
                    .page
                    .add_script(
                        &url,
                        kind,
                        fetch_options,
                        crate::winhttp::decode_text(&bytes, content_type.as_deref()),
                    )
                    .then_some(())
                    .ok_or_else(|| "script was not installed".to_string()),
                PageResource::Font {
                    url,
                    family,
                    weight,
                    italic,
                } => self.page.add_font(url, family, weight, italic, &bytes),
            };
            match installed {
                Ok(()) => {
                    retained = true;
                    self.resource_budget = self.resource_budget.saturating_sub(size);
                }
                Err(error) => self.record_resource_diagnostic(format!("{label}: {error}")),
            }
        }
        Ok(retained)
    }

    fn record_resource_diagnostic(&mut self, message: String) {
        if self.page.diagnostics.len() >= MAX_RESOURCE_DIAGNOSTICS {
            return;
        }
        self.page.diagnostics.push(
            bounded_utf8_prefix(&message, MAX_RESOURCE_DIAGNOSTIC_BYTES)
                .0
                .to_string(),
        );
    }

    pub(super) fn execute_pending_async_scripts(
        &mut self,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
        script_fetch_time: &mut std::time::Duration,
    ) -> Result<(), String> {
        let pending = self
            .page
            .scripts
            .iter()
            .filter(|script| !script.blocks_first_paint)
            .filter(|script| !self.executed_async_scripts.contains(&script.source_url))
            .map(|script| (script.source_url.clone(), script.kind, script.fetch_options))
            .next();
        let Some((url, kind, fetch_options)) = pending else {
            return Ok(());
        };
        if self.page.scripts.iter().any(|script| {
            script.source_url == url
                && script.kind == kind
                && script.fetch_options == fetch_options
                && script.code.is_none()
        }) {
            let resource = PageResource::Script {
                url: url.clone(),
                kind,
                fetch_options,
            };
            // The parser preload and the retained event loop share this resource record. Wait for
            // the in-flight preload rather than issuing a duplicate blocking request on every
            // renderer wakeup.
            if self
                .pending_resource_preloads
                .as_ref()
                .is_some_and(|pending| pending.contains(&resource))
            {
                return Ok(());
            }
            let result = if self.loaded_resources.contains(&resource) {
                Err("script could not be loaded".to_string())
            } else {
                let started = std::time::Instant::now();
                let result = fetch_script_source(connection, self.id, &url, kind, fetch_options);
                *script_fetch_time += started.elapsed();
                result
            };
            match result {
                Ok(code) => {
                    self.page.add_script(&url, kind, fetch_options, code);
                }
                Err(error) => outcome.errors.push(format!("{url}: {error}")),
            }
        }
        let inputs = self
            .page
            .scripts
            .iter()
            .filter(|script| !script.blocks_first_paint)
            .filter(|script| !self.executed_async_scripts.contains(&script.source_url))
            .filter_map(|script| {
                script.code.as_ref().map(|code| ScriptInput {
                    node: script.node.clone(),
                    source_url: script.source_url.clone(),
                    code: code.clone(),
                    kind: script.kind,
                    fetch_options: script.fetch_options,
                    finish_lifecycle: true,
                })
            })
            .take(1)
            .collect::<Vec<_>>();
        if self.script_runtime.is_some() {
            connection.report_renderer_task_stage(format!("executing async script {url}"))?;
        }
        self.executed_async_scripts.insert(url);
        if let Some(runtime) = self.script_runtime.as_mut() {
            // A dynamically inserted external script is a later event-loop task. Keep it queued
            // so the renderer can accept input between that task and this async script.
            let result = if inputs.iter().any(|input| input.kind == ScriptKind::Module) {
                let document = self.id;
                let mut loader = |url: &str, kind, options| {
                    let started = std::time::Instant::now();
                    let result = fetch_script_source(connection, document, url, kind, options);
                    *script_fetch_time += started.elapsed();
                    result
                };
                runtime.execute_additional_with_loader(&inputs, Some(&mut loader))
            } else {
                runtime.execute_additional_with_loader(&inputs, None)
            };
            merge_outcome(outcome, result, self.page.dom.document.id());
        }
        Ok(())
    }
}

fn is_presentational_resource(resource: &PageResource) -> bool {
    matches!(
        resource,
        PageResource::Stylesheet { .. }
            | PageResource::Image { .. }
            | PageResource::Media { .. }
            | PageResource::Font { .. }
    )
}

fn resource_label(resource: &PageResource) -> String {
    let (kind, url) = match resource {
        PageResource::Stylesheet { url } => ("stylesheet", url),
        PageResource::Image { url } => ("image", url),
        PageResource::Media { url, .. } => ("media", url),
        PageResource::Script { url, .. } => ("script", url),
        PageResource::Font { url, .. } => ("font", url),
    };
    let url = bounded_utf8_prefix(url, 384).0;
    format!("{kind} {url}")
}

fn resource_requests(
    connection: &mut ChildConnection,
    document: DocumentId,
    resources: Vec<PageResource>,
) -> (
    Vec<crate::renderer_protocol::RendererFetchRequest>,
    HashMap<u64, PageResource>,
) {
    let mut by_request = HashMap::new();
    let requests = resources
        .into_iter()
        .map(|resource| {
            let id = connection.allocate_request_id();
            let request = page_resource_request(id, document, &resource);
            by_request.insert(id, resource);
            request
        })
        .collect();
    (requests, by_request)
}

pub(super) fn fetch_script_source(
    connection: &mut ChildConnection,
    document: DocumentId,
    url: &str,
    kind: ScriptKind,
    fetch_options: crate::engine::ScriptFetchOptions,
) -> Result<String, String> {
    let resource = PageResource::Script {
        url: url.to_string(),
        kind,
        fetch_options,
    };
    let request = page_resource_request(connection.allocate_request_id(), document, &resource);
    let response = connection
        .fetch_batch(document, vec![request])?
        .pop()
        .ok_or_else(|| "browser omitted a script response".to_string())?;
    decode_script_response(response, kind)
}

pub(super) fn decode_script_response(
    response: BrowserFetchResponse,
    kind: ScriptKind,
) -> Result<String, String> {
    let response = into_fetch_result(response).map_err(|error| error.to_string())?;
    if !response.is_success() {
        return Err(format!("server returned HTTP {}", response.status));
    }
    validate_script_response(&response, kind).map_err(|error| error.to_string())?;
    Ok(crate::winhttp::decode_text(
        response.body.as_bytes(),
        response.content_type(),
    ))
}
