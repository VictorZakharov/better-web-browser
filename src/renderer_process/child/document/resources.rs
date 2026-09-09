//! Renderer-side resource installation and script-network completions.

pub(super) mod events;
mod streaming;

use super::DocumentRuntime;
use super::fetch::{into_fetch_result, page_resource_request, validate_script_response};
use crate::engine::{Page, PageResource, ScriptKind, ScriptOutcome};
use crate::limits::bounded_utf8_prefix;
use crate::renderer_process::child::connection::{ChildConnection, PendingFetchBatch};
use crate::renderer_protocol::{BrowserFetchResponse, DocumentId};
use std::collections::{HashMap, HashSet};

const MAX_RESOURCE_DIAGNOSTICS: usize = 32;
const MAX_RESOURCE_DIAGNOSTIC_BYTES: usize = 512;

pub(super) struct PendingResourceFetch {
    batch: PendingFetchBatch,
    by_request: HashMap<u64, PageResource>,
}

pub(super) struct ResourceChanges {
    pub(super) render: bool,
    pub(super) style: bool,
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
        if self.dispatch_cached_resource_events()? {
            self.resource_render_pending = true;
        }
        let mut seen = HashSet::new();
        let resources = self
            .page
            .resources
            .iter()
            .filter(|resource| is_presentational_resource(resource))
            .cloned()
            .chain(self.async_scripts.resources())
            .filter(|resource| !self.loaded_resources.contains(resource))
            .filter(|resource| {
                !self
                    .pending_resource_preloads
                    .iter()
                    .any(|pending| pending.contains(resource))
            })
            .filter(|resource| seen.insert(resource.clone()))
            .collect::<Vec<_>>();
        let (requests, by_request) = resource_requests(connection, self.id, resources);
        let Some(batch) = connection.start_fetch_batch(self.id, requests)? else {
            return Ok(());
        };
        self.pending_resource_preloads
            .push(PendingResourceFetch { batch, by_request });
        Ok(())
    }

    pub(in crate::renderer_process::child) fn finish_completed_resource_preloads(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<Option<crate::renderer_protocol::RendererRuntimeUpdate>, String> {
        self.finish_ready_dynamic_scripts(connection)?;
        let changes = self.finish_ready_resource_preloads(connection)?;
        let render = changes.as_ref().is_some_and(|changes| changes.render);
        let style = changes.as_ref().is_some_and(|changes| changes.style);
        let dynamic_ready = self
            .script_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.has_ready_dynamic_scripts());
        if !render && !self.async_scripts.has_ready() && !dynamic_ready {
            return Ok(None);
        }
        // A network burst commonly completes several images at once. Rendering from this
        // response-end task serializes one complete style/layout/presentation pass per image and
        // prevents the renderer from reading the remaining response messages. Schedule an
        // immediate rendering checkpoint instead; messages already in the pipe are then handled
        // first and all completed resources share one layout.
        self.resource_render_pending |= render;
        self.resource_style_refresh_pending |= style;
        Ok(Some(crate::renderer_protocol::RendererRuntimeUpdate {
            document: self.id,
            clock_advanced: false,
            runtime: super::runtime_report(
                ScriptOutcome::default(),
                self.script_runtime.is_some(),
                self.media_runtime_report(),
            ),
            load: crate::renderer_protocol::PageLoadReport::default(),
            next_timer_micros: Some(0),
        }))
    }

    pub(super) fn finish_ready_resource_preloads(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<Option<ResourceChanges>, String> {
        let mut changed = None;
        for mut pending in std::mem::take(&mut self.pending_resource_preloads) {
            let responses = connection.take_ready_fetch_batch(&mut pending.batch)?;
            if !responses.is_empty() {
                let style = responses.iter().any(|response| {
                    pending
                        .by_request
                        .get(&response.head.request_id)
                        .is_some_and(|resource| matches!(resource, PageResource::Stylesheet { .. }))
                });
                let render = self.install_resource_responses(
                    connection,
                    responses,
                    &mut pending.by_request,
                    true,
                )?;
                let changes = changed.get_or_insert(ResourceChanges {
                    render: false,
                    style: false,
                });
                changes.render |= render;
                changes.style |= render && style;
            }
            if !pending.batch.is_empty() {
                self.pending_resource_preloads.push(pending);
            }
        }
        Ok(changed)
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
            if require_authoritative_match
                && !self.page.resources.contains(&resource)
                && !self.async_scripts.contains(&resource)
            {
                continue;
            }
            self.loaded_resources.insert(resource.clone());
            let response = match into_fetch_result(response) {
                Ok(response) => response,
                Err(error) => {
                    self.record_resource_diagnostic(format!("{label}: {error}"));
                    retained |= self.dispatch_resource_event(&resource, "error")?;
                    continue;
                }
            };
            if !response.is_success() {
                self.record_resource_diagnostic(format!(
                    "{label}: server returned HTTP {}",
                    response.status
                ));
                retained |= self.dispatch_resource_event(&resource, "error")?;
                continue;
            }
            if let PageResource::Script { kind, .. } = &resource
                && let Err(error) = validate_script_response(&response, *kind)
            {
                self.record_resource_diagnostic(format!("{label}: {error}"));
                retained |= self.dispatch_resource_event(&resource, "error")?;
                continue;
            }
            let size = response.body.len() as u64;
            if size > self.resource_budget {
                self.record_resource_diagnostic(format!(
                    "{label}: skipped {size} bytes because only {} page-resource bytes remain",
                    self.resource_budget
                ));
                retained |= self.dispatch_resource_event(&resource, "error")?;
                continue;
            }
            let event_resource = resource.clone();
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
                PageResource::Media { node, .. } => {
                    let mime_type = content_type.unwrap_or_else(|| "video/mp4".into());
                    let result = connection
                        .decode_media(&bytes)
                        .and_then(|decode| self.install_media_decode(node, decode, mime_type));
                    if let Err(error) = &result {
                        self.record_media_failure(error.clone());
                    }
                    result
                }
                PageResource::Script {
                    url,
                    kind,
                    fetch_options,
                } => {
                    let code = crate::winhttp::decode_text(&bytes, content_type.as_deref());
                    if code.len() > crate::limits::MAX_SCRIPT_BYTES {
                        self.record_resource_diagnostic(format!(
                            "{label}: script exceeds the per-script byte limit"
                        ));
                        retained |= self.dispatch_resource_event(&event_resource, "error")?;
                        continue;
                    }
                    let prepared = self.async_scripts.contains(&event_resource);
                    self.async_scripts.complete(&event_resource, Some(&code));
                    (self.page.add_script(&url, kind, fetch_options, code) || prepared)
                        .then_some(())
                        .ok_or_else(|| "script was not installed".to_string())
                }
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
                    retained |= self.dispatch_resource_event(&event_resource, "load")?;
                }
                Err(error) => {
                    self.record_resource_diagnostic(format!("{label}: {error}"));
                    retained |= self.dispatch_resource_event(&event_resource, "error")?;
                }
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
