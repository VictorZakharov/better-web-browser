//! Renderer-side resource installation and script-network completions.

pub(super) mod events;
mod installation;
mod lifecycle;
mod streaming;

use super::DocumentRuntime;
use super::fetch::{into_fetch_result, page_resource_request, validate_script_response};
use crate::engine::{Page, PageResource, ScriptKind, ScriptOutcome, ScriptRuntime};
use crate::limits::bounded_utf8_prefix;
use crate::renderer_process::child::connection::{ChildConnection, PendingFetchBatch};
use crate::renderer_protocol::{BrowserFetchResponse, DocumentId};
use std::collections::{HashMap, HashSet};

pub(super) struct PendingResourceFetch {
    batch: PendingFetchBatch,
    by_request: HashMap<u64, PageResource>,
    load_blockers: HashSet<PageResource>,
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
            load_blockers: HashSet::new(),
        }),
        deferred_batch.map(|batch| PendingResourceFetch {
            batch,
            by_request: deferred_by_request,
            load_blockers: HashSet::new(),
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
        if let Some(runtime) = self.script_runtime.as_mut() {
            self.parser_scripts.prepare_modules(runtime);
        }
        let mut seen = HashSet::new();
        let resources = self
            .page
            .resources
            .iter()
            .filter(|resource| is_presentational_resource(resource))
            .cloned()
            .chain(self.parser_scripts.resources())
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
        if let Some(batch) = connection.start_fetch_batch(self.id, requests)? {
            self.pending_resource_preloads.push(PendingResourceFetch {
                batch,
                by_request,
                load_blockers: HashSet::new(),
            });
        }
        self.register_document_load_resources();
        Ok(())
    }

    pub(in crate::renderer_process::child) fn finish_completed_resource_preloads(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<Option<crate::renderer_protocol::RendererRuntimeUpdate>, String> {
        self.finish_ready_dynamic_scripts(connection)?;
        let changes = self.finish_ready_resource_preloads(connection)?;
        self.start_presentational_preloads(connection)?;
        let render = changes.as_ref().is_some_and(|changes| changes.render);
        let style = changes.as_ref().is_some_and(|changes| changes.style);
        let dynamic_ready = self
            .script_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.has_ready_dynamic_scripts());
        self.publish_document_load_readiness(render || self.pending_async_outcome.render_requested);
        let document_ready = self
            .script_runtime
            .as_ref()
            .is_some_and(ScriptRuntime::has_ready_document_task);
        if !render && !self.parser_scripts.has_ready() && !dynamic_ready && !document_ready {
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
