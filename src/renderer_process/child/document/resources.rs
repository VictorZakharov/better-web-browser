//! Renderer-side resource installation and script-network completions.

use super::fetch::{
    into_fetch_result, page_resource_request, script_api_request, validate_script_response,
};
use super::{DocumentRuntime, merge_outcome};
use crate::engine::script::ScriptInput;
use crate::engine::{Page, PageResource, ScriptFetchAction, ScriptKind, ScriptOutcome};
use crate::renderer_process::child::connection::{ChildConnection, PendingFetchBatch};
use crate::renderer_protocol::{BrowserFetchResponse, DocumentId};
use std::collections::{HashMap, HashSet};

pub(super) struct PendingResourceFetch {
    batch: PendingFetchBatch,
    by_request: HashMap<u64, PageResource>,
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
        let (requests, by_request) = resource_requests(connection, self.id, resources);
        let responses = connection.fetch_batch(self.id, requests)?;
        self.install_resource_responses(responses, by_request, false)
    }

    pub(super) fn finish_resource_preloads(
        &mut self,
        connection: &mut ChildConnection,
        pending: PendingResourceFetch,
    ) -> Result<bool, String> {
        let responses = connection.finish_fetch_batch(pending.batch)?;
        self.install_resource_responses(responses, pending.by_request, true)
    }

    fn install_resource_responses(
        &mut self,
        responses: Vec<BrowserFetchResponse>,
        mut by_request: HashMap<u64, PageResource>,
        require_authoritative_match: bool,
    ) -> Result<bool, String> {
        let mut retained = false;
        for response in responses {
            let Some(resource) = by_request.remove(&response.head.request_id) else {
                return Err("browser returned an unknown resource request".into());
            };
            if require_authoritative_match && !self.page.resources.contains(&resource) {
                continue;
            }
            self.loaded_resources.insert(resource.clone());
            let Ok(response) = into_fetch_result(response) else {
                continue;
            };
            if !response.is_success() {
                continue;
            }
            if let PageResource::Script { kind, .. } = &resource
                && validate_script_response(&response, *kind).is_err()
            {
                continue;
            }
            let size = response.body.len() as u64;
            if size > self.resource_budget {
                continue;
            }
            let content_type = response.content_type().map(str::to_string);
            let bytes = response.body.into_bytes();
            let installed = match resource {
                PageResource::Stylesheet { url } => self.page.add_stylesheet_from(
                    &url,
                    crate::winhttp::decode_text(&bytes, content_type.as_deref()),
                ),
                PageResource::Image { url } => self.page.add_image(url, &bytes).is_ok(),
                PageResource::Script {
                    url,
                    kind,
                    fetch_options,
                } => self.page.add_script(
                    &url,
                    kind,
                    fetch_options,
                    crate::winhttp::decode_text(&bytes, content_type.as_deref()),
                ),
                PageResource::Font {
                    url,
                    family,
                    weight,
                    italic,
                } => self
                    .page
                    .add_font(url, family, weight, italic, &bytes)
                    .is_ok(),
            };
            if installed {
                retained = true;
                self.resource_budget = self.resource_budget.saturating_sub(size);
            }
        }
        Ok(retained)
    }

    pub(super) fn execute_pending_async_scripts(
        &mut self,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
    ) -> Result<(), String> {
        let pending = self
            .page
            .scripts
            .iter()
            .filter(|script| !script.blocks_first_paint)
            .filter(|script| !self.executed_async_scripts.contains(&script.source_url))
            .map(|script| (script.source_url.clone(), script.kind, script.fetch_options))
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return Ok(());
        }
        for (url, kind, fetch_options) in &pending {
            if self.page.scripts.iter().any(|script| {
                script.source_url == *url
                    && script.kind == *kind
                    && script.fetch_options == *fetch_options
                    && script.code.is_none()
            }) {
                let resource = PageResource::Script {
                    url: url.clone(),
                    kind: *kind,
                    fetch_options: *fetch_options,
                };
                let result = if self.loaded_resources.contains(&resource) {
                    Err("script could not be loaded".to_string())
                } else {
                    fetch_script_source(connection, self.id, url, *kind, *fetch_options)
                };
                match result {
                    Ok(code) => {
                        self.page.add_script(url, *kind, *fetch_options, code);
                    }
                    Err(error) => outcome.errors.push(format!("{url}: {error}")),
                }
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
            .collect::<Vec<_>>();
        for (url, ..) in pending {
            self.executed_async_scripts.insert(url);
        }
        if let Some(runtime) = self.script_runtime.as_mut() {
            let document = self.id;
            let mut loader = |url: &str, kind, options| {
                fetch_script_source(connection, document, url, kind, options)
            };
            let result = runtime.execute_additional_with_loader(&inputs, Some(&mut loader));
            merge_outcome(outcome, result, self.page.dom.document.id());
        }
        Ok(())
    }

    pub(super) fn complete_pending_fetches(
        &mut self,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
    ) -> Result<(), String> {
        let actions = std::mem::take(&mut self.pending_fetches);
        let mut aborted = HashSet::new();
        let mut requests = Vec::new();
        let mut script_ids = HashMap::new();
        for action in actions {
            match action {
                ScriptFetchAction::Abort { id } => {
                    aborted.insert(id);
                }
                ScriptFetchAction::Start { id, request } if !aborted.contains(&id) => {
                    let wire_id = connection.allocate_request_id();
                    script_ids.insert(wire_id, id);
                    requests.push(script_api_request(wire_id, self.id, *request));
                }
                ScriptFetchAction::Start { .. } => {}
            }
        }
        if requests.is_empty() {
            return Ok(());
        }
        let responses = connection.fetch_batch(self.id, requests)?;
        for response in responses {
            let Some(script_id) = script_ids.remove(&response.head.request_id) else {
                return Err("browser returned an unknown script Fetch request".into());
            };
            if let Some(runtime) = self.script_runtime.as_mut() {
                let document = self.id;
                let mut loader = |url: &str, kind, options| {
                    fetch_script_source(connection, document, url, kind, options)
                };
                let result = runtime.complete_fetch_with_loader(
                    script_id,
                    into_fetch_result(response),
                    Some(&mut loader),
                );
                merge_outcome(outcome, result, self.page.dom.document.id());
            }
        }
        Ok(())
    }
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
