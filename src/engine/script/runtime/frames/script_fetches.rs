//! Fetch ownership for parser scripts, inserted scripts, and import() in each child.
use super::resources::ScriptOwner;
use super::*;
use crate::engine::page::PageResource;
use crate::fetch::{FetchRequest, RequestDestination};

impl ScriptRuntime {
    pub(super) fn start_frame_resources(&mut self) {
        let Some(frames) = &mut self.frames else {
            return;
        };
        let mut jobs = Vec::new();
        for (id, child) in &mut frames.children {
            if let Some(document) = frames.documents.get_mut(id) {
                for resource in document.queue.resources() {
                    if document.requested.insert(resource.clone()) {
                        jobs.push((*id, resource, ScriptOwner::Parser));
                    }
                }
            }
            for request in child.take_dynamic_script_requests() {
                jobs.push((
                    *id,
                    PageResource::Script {
                        url: request.source_url,
                        kind: request.kind,
                        fetch_options: request.fetch_options,
                    },
                    ScriptOwner::Dynamic(request.node),
                ));
            }
            for (url, fetch_options) in child.take_module_requests() {
                jobs.push((
                    *id,
                    PageResource::Script {
                        url: url.clone(),
                        kind: ScriptKind::Module,
                        fetch_options,
                    },
                    ScriptOwner::Module(url),
                ));
            }
        }
        for (document, resource, owner) in jobs {
            let PageResource::Script {
                url, fetch_options, ..
            } = &resource
            else {
                continue;
            };
            let frames = self.frames.as_mut().unwrap();
            let host = frames.children[&document].host.borrow();
            let request = FetchRequest::subresource(
                url,
                host.inherited_url.as_deref().unwrap_or(&host.document_url),
                RequestDestination::Script,
            );
            let result = request
                .and_then(|mut request| {
                    host.policy
                        .check_request(RequestDestination::Script, url, 0)?;
                    request.policy = host.policy.clone();
                    request.origin = Some(host.document_origin.clone());
                    request.client = host.fetch_client;
                    request.mode = fetch_options.mode;
                    request.credentials = fetch_options.credentials;
                    request.referrer_policy = fetch_options.referrer_policy;
                    request.response_body_limit = MAX_SCRIPT_BYTES;
                    Ok(request)
                })
                .map_err(|error| error.to_string());
            let result = result.and_then(|request| {
                host.fetch_identifiers
                    .borrow_mut()
                    .allocate(document)
                    .map(|id| (id, request))
                    .map_err(|error| error.to_string())
            });
            drop(host);
            match result {
                Ok((id, request)) => {
                    frames
                        .fetches
                        .insert(id, FrameFetch::script(document, resource, owner));
                    frames.children[&document]
                        .host
                        .borrow_mut()
                        .pending_fetch_actions
                        .push(ScriptFetchAction::Start {
                            id,
                            request: Box::new(request),
                        });
                }
                Err(error) => self.finish_frame_script(document, resource, owner, Err(error)),
            }
        }
    }

    pub(super) fn finish_frame_script(
        &mut self,
        id: NodeId,
        resource: PageResource,
        owner: ScriptOwner,
        result: Result<(String, String), String>,
    ) {
        let Some(frames) = &mut self.frames else {
            return;
        };
        let Some(child) = frames.children.get_mut(&id) else {
            return;
        };
        match owner {
            ScriptOwner::Parser => {
                if let Err(error) = &result {
                    child.host.borrow_mut().diagnose(error.clone());
                }
                if let Some(document) = frames.documents.get_mut(&id) {
                    document.queue.complete(
                        &resource,
                        result.as_ref().ok().map(|(_, code)| code.as_str()),
                    );
                }
            }
            ScriptOwner::Dynamic(node) => {
                child.complete_dynamic_script(node, result.map(|(_, code)| code))
            }
            ScriptOwner::Module(url) => child.complete_module_fetch(url, result),
        }
    }
}
