//! Bounded frame navigation/script fetches routed over the existing streaming Fetch transport.
use super::*;
use crate::engine::page::PageResource;
use crate::fetch::{FetchRequest, FetchResponse, RequestDestination};

pub(super) enum Purpose {
    Navigation(FrameNavigation),
    Script {
        document: NodeId,
        resource: PageResource,
    },
}

pub(super) struct FrameFetch {
    purpose: Purpose,
    head: Option<FetchResponse>,
    bytes: Vec<u8>,
    failed: Option<String>,
}

impl FrameFetch {
    pub fn current(&self, context: &Context, active: &HashSet<NodeId>) -> bool {
        match &self.purpose {
            Purpose::Navigation(navigation) => context.frame_navigation_current(navigation),
            Purpose::Script { document, .. } => active.contains(document),
        }
    }
    pub fn navigation(request: FrameNavigation) -> Self {
        Self {
            purpose: Purpose::Navigation(request),
            head: None,
            bytes: Vec::new(),
            failed: None,
        }
    }
    fn script(document: NodeId, resource: PageResource) -> Self {
        Self {
            purpose: Purpose::Script { document, resource },
            head: None,
            bytes: Vec::new(),
            failed: None,
        }
    }
}

impl ScriptRuntime {
    pub(super) fn start_frame_resources(&mut self) {
        let Some(frames) = &mut self.frames else {
            return;
        };
        for (id, document) in &mut frames.documents {
            let Some(child) = frames.children.get_mut(id) else {
                continue;
            };
            let resources: Vec<_> = document.queue.resources().collect();
            for resource in resources {
                if !document.requested.insert(resource.clone()) {
                    continue;
                }
                let PageResource::Script {
                    url, fetch_options, ..
                } = &resource
                else {
                    continue;
                };
                let mut host = child.host.borrow_mut();
                let request = FetchRequest::subresource(
                    url,
                    host.inherited_url.as_deref().unwrap_or(&host.document_url),
                    RequestDestination::Script,
                );
                match request {
                    Ok(mut request) => {
                        request.origin = Some(host.document_origin.clone());
                        request.mode = fetch_options.mode;
                        request.credentials = fetch_options.credentials;
                        request.referrer_policy = fetch_options.referrer_policy;
                        request.response_body_limit = MAX_SCRIPT_BYTES;
                        let allocated = host.fetch_identifiers.borrow_mut().allocate(*id);
                        match allocated {
                            Ok(fetch_id) => {
                                frames
                                    .fetches
                                    .insert(fetch_id, FrameFetch::script(*id, resource));
                                host.pending_fetch_actions.push(ScriptFetchAction::Start {
                                    id: fetch_id,
                                    request: Box::new(request),
                                });
                            }
                            Err(error) => {
                                host.diagnose(error.to_string());
                                document.queue.complete(&resource, None);
                            }
                        }
                    }
                    Err(error) => {
                        host.diagnose(error.to_string());
                        document.queue.complete(&resource, None);
                    }
                }
            }
        }
    }

    pub(in crate::engine::script::runtime) fn is_frame_fetch(&self, id: u32) -> bool {
        self.frames
            .as_ref()
            .is_some_and(|frames| frames.fetches.contains_key(&id))
    }

    pub(in crate::engine::script::runtime) fn deliver_frame_fetch(
        &mut self,
        id: u32,
        event: ScriptFetchEvent,
    ) -> ScriptOutcome {
        let mut outcome = ScriptOutcome::default();
        let Some(fetch) = self
            .frames
            .as_mut()
            .and_then(|frames| frames.fetches.get_mut(&id))
        else {
            return outcome;
        };
        let terminal = match event {
            ScriptFetchEvent::Head(Ok(head)) => {
                fetch.head = Some(head);
                false
            }
            ScriptFetchEvent::Head(Err(error)) | ScriptFetchEvent::Abort(error) => {
                fetch.failed = Some(error.to_string());
                true
            }
            ScriptFetchEvent::Chunk(bytes) => {
                let limit = if matches!(fetch.purpose, Purpose::Navigation(_)) {
                    crate::limits::MAX_HTML_INPUT_BYTES
                } else {
                    MAX_SCRIPT_BYTES
                };
                if fetch.bytes.len().saturating_add(bytes.len()) > limit {
                    fetch.failed = Some("frame response exceeded its byte limit".into());
                    outcome.fetch_actions.push(ScriptFetchAction::Abort { id });
                    true
                } else {
                    fetch.bytes.extend(bytes);
                    outcome.fetch_actions.push(ScriptFetchAction::Consume {
                        id,
                        total: fetch.bytes.len() as u32,
                    });
                    false
                }
            }
            ScriptFetchEvent::End => true,
        };
        if terminal {
            let fetch = self.frames.as_mut().unwrap().fetches.remove(&id).unwrap();
            self.host.borrow().fetch_identifiers.borrow_mut().finish(id);
            self.finish_frame_fetch(fetch, &mut outcome);
        }
        outcome
    }

    fn finish_frame_fetch(&mut self, fetch: FrameFetch, outcome: &mut ScriptOutcome) {
        let success =
            fetch.failed.is_none() && fetch.head.as_ref().is_some_and(|head| head.is_success());
        if let Some(error) = fetch.failed {
            outcome.diagnostics.push(format!("iframe fetch: {error}"));
        }
        match fetch.purpose {
            Purpose::Navigation(mut navigation) => {
                if !success {
                    return;
                }
                let Some(child) = self
                    .frames
                    .as_ref()
                    .and_then(|frames| frames.children.get(&navigation.document))
                else {
                    return;
                };
                let head = fetch.head.unwrap();
                let source_origin = child.host.borrow().document_origin.clone();
                if head.headers.get("x-frame-options").is_some_and(|value| {
                    value.eq_ignore_ascii_case("deny")
                        || value.eq_ignore_ascii_case("sameorigin")
                            && !source_origin.is_same_origin(&head.final_url().origin())
                }) {
                    outcome
                        .diagnostics
                        .push("iframe response refused by X-Frame-Options".into());
                    return;
                }
                if head.headers.get("content-security-policy").is_some() {
                    // Until policy containers can be transferred into child realms, fail closed.
                    outcome.diagnostics.push(
                        "iframe response requires unsupported Content-Security-Policy enforcement"
                            .into(),
                    );
                    return;
                }
                if head.content_type().is_some_and(|mime| {
                    !mime
                        .split(';')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .eq_ignore_ascii_case("text/html")
                }) {
                    outcome
                        .diagnostics
                        .push("iframe response is not an HTML document".into());
                    return;
                }
                navigation.url = head.final_url().as_str().to_string();
                let decoded = crate::winhttp::decode_document(&fetch.bytes, head.content_type());
                if let Err(error) = self.commit_frame(navigation, decoded.text, decoded.encoding) {
                    outcome.errors.push(error);
                }
            }
            Purpose::Script { document, resource } => {
                if let Some(document) = self
                    .frames
                    .as_mut()
                    .and_then(|frames| frames.documents.get_mut(&document))
                {
                    let code = success.then(|| {
                        crate::winhttp::decode_text(
                            &fetch.bytes,
                            fetch.head.as_ref().and_then(FetchResponse::content_type),
                        )
                    });
                    document.queue.complete(&resource, code.as_deref());
                }
            }
        }
    }
}
