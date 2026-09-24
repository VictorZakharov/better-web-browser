//! Bounded frame navigation/script fetches routed over the existing streaming Fetch transport.
use super::*;
use crate::engine::page::PageResource;
use crate::fetch::FetchResponse;

pub(super) enum ScriptOwner {
    Parser,
    Dynamic(NodeId, String),
    Module(String),
}

pub(super) enum Purpose {
    Navigation(FrameNavigation),
    Style {
        document: NodeId,
        url: String,
        integrity: Vec<String>,
    },
    Image {
        document: NodeId,
        url: String,
    },
    Script {
        document: NodeId,
        resource: PageResource,
        owner: ScriptOwner,
        integrity: Vec<String>,
    },
}

pub(super) struct FrameFetch {
    purpose: Purpose,
    head: Option<FetchResponse>,
    bytes: Vec<u8>,
    received: usize,
}

impl FrameFetch {
    pub fn current(&self, context: &Context, active: &HashSet<NodeId>) -> bool {
        match &self.purpose {
            Purpose::Navigation(navigation) => context.frame_navigation_current(navigation),
            Purpose::Script { document, .. }
            | Purpose::Style { document, .. }
            | Purpose::Image { document, .. } => active.contains(document),
        }
    }
    pub fn navigation(request: FrameNavigation) -> Self {
        Self {
            purpose: Purpose::Navigation(request),
            head: None,
            bytes: Vec::new(),
            received: 0,
        }
    }
    pub(super) fn style(document: NodeId, url: String, integrity: Vec<String>) -> Self {
        Self {
            purpose: Purpose::Style {
                document,
                url,
                integrity,
            },
            head: None,
            bytes: Vec::new(),
            received: 0,
        }
    }
    pub(super) fn image(document: NodeId, url: String) -> Self {
        Self {
            purpose: Purpose::Image { document, url },
            head: None,
            bytes: Vec::new(),
            received: 0,
        }
    }
    pub(super) fn script(
        document: NodeId,
        resource: PageResource,
        owner: ScriptOwner,
        integrity: Vec<String>,
    ) -> Self {
        Self {
            purpose: Purpose::Script {
                document,
                resource,
                owner,
                integrity,
            },
            head: None,
            bytes: Vec::new(),
            received: 0,
        }
    }
    pub(super) fn retarget(&mut self, navigation: &FrameNavigation) {
        if let Purpose::Navigation(current) = &mut self.purpose {
            *current = navigation.clone();
        }
    }
}

impl ScriptRuntime {
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
        let Some(mut fetch) = self
            .frames
            .as_mut()
            .and_then(|frames| frames.fetches.remove(&id))
        else {
            return outcome;
        };
        let result = (|| -> Result<bool, String> {
            match event {
                ScriptFetchEvent::Head(Ok(head)) => {
                    if let Purpose::Navigation(navigation) = &mut fetch.purpose {
                        self.start_frame_stream(navigation, id, &head)?;
                    }
                    fetch.head = Some(head);
                    Ok(false)
                }
                ScriptFetchEvent::Head(Err(error)) | ScriptFetchEvent::Abort(error) => {
                    Err(error.to_string())
                }
                ScriptFetchEvent::Chunk(bytes) => {
                    let limit = if matches!(fetch.purpose, Purpose::Navigation(_)) {
                        crate::limits::MAX_HTML_INPUT_BYTES
                    } else if matches!(fetch.purpose, Purpose::Style { .. }) {
                        crate::limits::MAX_CSS_SOURCE_BYTES
                    } else if matches!(fetch.purpose, Purpose::Image { .. }) {
                        crate::limits::MAX_IMAGE_SOURCE_BYTES
                    } else {
                        MAX_SCRIPT_BYTES
                    };
                    fetch.received = fetch.received.saturating_add(bytes.len());
                    if fetch.received > limit {
                        return Err("frame response exceeded its byte limit".into());
                    }
                    if let Purpose::Navigation(navigation) = &fetch.purpose {
                        self.append_frame_stream(navigation.document, &bytes, false)?;
                    } else {
                        fetch.bytes.extend(bytes)
                    }
                    outcome.fetch_actions.push(ScriptFetchAction::Consume {
                        id,
                        total: fetch.received as u32,
                    });
                    Ok(false)
                }
                ScriptFetchEvent::End => {
                    if let Purpose::Navigation(navigation) = &fetch.purpose {
                        self.append_frame_stream(navigation.document, &[], true)?;
                    }
                    Ok(true)
                }
            }
        })();
        match result {
            Ok(false) => {
                self.frames.as_mut().unwrap().fetches.insert(id, fetch);
            }
            terminal => {
                self.host.borrow().fetch_identifiers.borrow_mut().finish(id);
                if let Err(error) = &terminal {
                    outcome.diagnostics.push(format!("iframe fetch: {error}"));
                    outcome.fetch_actions.push(ScriptFetchAction::Abort { id });
                }
                self.finish_frame_fetch(fetch, terminal.is_ok(), &mut outcome);
            }
        }
        outcome
    }

    fn finish_frame_fetch(
        &mut self,
        fetch: FrameFetch,
        success: bool,
        outcome: &mut ScriptOutcome,
    ) {
        match fetch.purpose {
            Purpose::Style {
                document,
                url,
                integrity,
            } => {
                let response = fetch.head.filter(|_| success).map(|mut head| {
                    head.body = crate::fetch::Body::from_bytes(fetch.bytes);
                    head
                });
                self.finish_frame_style(document, url, integrity, response);
            }
            Purpose::Image { document, url } => {
                let response =
                    fetch
                        .head
                        .filter(|head| success && head.is_success())
                        .map(|mut head| {
                            head.body = crate::fetch::Body::from_bytes(fetch.bytes);
                            head
                        });
                self.finish_frame_image(document, url, response, outcome);
            }
            Purpose::Navigation(navigation) => {
                if !success {
                    self.context
                        .as_ref()
                        .unwrap()
                        .fail_frame_navigation(&navigation);
                    // An interrupted committed response must still release the parser/load
                    // delay. It cannot kill the embedding document's renderer.
                    if fetch.head.is_some()
                        && let Err(error) = self.append_frame_stream(navigation.document, &[], true)
                    {
                        outcome.diagnostics.push(error)
                    }
                }
            }
            Purpose::Script {
                document,
                resource,
                owner,
                integrity,
            } => {
                let PageResource::Script { kind, .. } = &resource else {
                    return;
                };
                let result = fetch
                    .head
                    .filter(|head| success && head.is_success())
                    .ok_or_else(|| "iframe script fetch failed".to_string())
                    .and_then(|mut head| {
                        head.body = crate::fetch::Body::from_bytes(fetch.bytes);
                        for value in &integrity {
                            crate::fetch::integrity::verify(
                                value,
                                head.body.as_bytes(),
                                matches!(
                                    head.response_type,
                                    crate::fetch::ResponseType::Basic
                                        | crate::fetch::ResponseType::Cors
                                ),
                            )
                            .map_err(|error| error.to_string())?;
                        }
                        crate::engine::script::network::response::decode(head, *kind)
                    });
                self.finish_frame_script(document, resource, owner, result);
            }
        }
    }
}
