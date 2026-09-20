//! Frame navigation admission and document replacement, outside author-script stacks.
use super::*;
use crate::engine::dom::incremental::HtmlParser;
use crate::fetch::{FetchRequest, FetchUrl, Referrer};

impl ScriptRuntime {
    pub(super) fn advance_frame_navigation(&mut self) -> ScriptOutcome {
        let Some(mut request) = self
            .frames
            .as_mut()
            .and_then(|frames| frames.navigations.pop_front())
        else {
            return ScriptOutcome::default();
        };
        if !self
            .context
            .as_ref()
            .is_some_and(|context| context.frame_navigation_current(&request))
        {
            return ScriptOutcome::default();
        }
        let mut outcome = ScriptOutcome::default();
        if request.source.is_some() || request.url == "about:blank" {
            let source = request.source.clone().unwrap_or_default();
            if let Err(error) = self.commit_frame(request, source, "UTF-8") {
                outcome.errors.push(error);
            }
        } else {
            match FetchRequest::navigation(&request.url) {
                Ok(mut fetch) => {
                    if let Err(error) = request.embedding_policy.check_request(
                        crate::fetch::RequestDestination::Document,
                        &request.url,
                        0,
                    ) {
                        self.context
                            .as_ref()
                            .unwrap()
                            .fail_frame_navigation(&request);
                        outcome.diagnostics.push(error.to_string());
                        return outcome;
                    }
                    let host =
                        Rc::clone(&self.frames.as_ref().unwrap().children[&request.document].host);
                    let host = host.borrow();
                    fetch.origin = Some(request.initiator_origin.clone());
                    fetch.referrer = FetchUrl::parse(&request.initiator_url)
                        .map(Referrer::Url)
                        .unwrap_or(Referrer::NoReferrer);
                    fetch.response_body_limit = crate::limits::MAX_HTML_INPUT_BYTES;
                    match host
                        .fetch_identifiers
                        .borrow_mut()
                        .allocate(request.document)
                    {
                        Ok(id) => {
                            fetch.client = request.initiator_client;
                            fetch.embedding_client = request.embedding_client;
                            fetch.policy = request.embedding_policy.clone();
                            fetch.resulting_client = crate::fetch::RequestClient {
                                id: u64::from(id),
                                opaque: host.sandbox.opaque_origin,
                            };
                            request.response_client = fetch.resulting_client;
                            self.frames
                                .as_mut()
                                .unwrap()
                                .fetches
                                .insert(id, FrameFetch::navigation(request));
                            outcome.fetch_actions.push(ScriptFetchAction::Start {
                                id,
                                request: Box::new(fetch),
                            });
                        }
                        Err(error) => {
                            self.context
                                .as_ref()
                                .unwrap()
                                .fail_frame_navigation(&request);
                            outcome.diagnostics.push(error.to_string());
                        }
                    }
                }
                Err(error) => {
                    self.context
                        .as_ref()
                        .unwrap()
                        .fail_frame_navigation(&request);
                    outcome
                        .diagnostics
                        .push(format!("iframe navigation: {error}"));
                }
            }
        }
        outcome
    }

    pub(super) fn commit_frame(
        &mut self,
        mut request: FrameNavigation,
        source: String,
        encoding: &str,
    ) -> Result<(), String> {
        self.commit_frame_parser(&mut request, HtmlParser::new(&source), encoding, None)
            .map(|_| ())
    }

    pub(super) fn commit_frame_parser(
        &mut self,
        request: &mut FrameNavigation,
        parser: HtmlParser,
        encoding: &str,
        fetch: Option<u32>,
    ) -> Result<Option<NodeId>, String> {
        if !self
            .context
            .as_ref()
            .is_some_and(|context| context.frame_navigation_current(request))
        {
            return Ok(None);
        }
        let document = parser.dom().document.clone();
        if let Some(fetch) = fetch {
            self.host
                .borrow()
                .fetch_identifiers
                .borrow_mut()
                .reassign(fetch, document.id());
        }
        let id = self
            .context
            .as_deref_mut()
            .ok_or("inactive frame tree")?
            .replace_frame(request.element, document, request.url.clone())
            .map_err(|error| error.to_string())?
            .ok_or("frame was removed during navigation")?;
        self.sync_child_runtimes();
        let frames = self.frames.as_mut().ok_or("inactive frame scheduler")?;
        let child = frames
            .children
            .get_mut(&id)
            .ok_or("replacement frame was not registered")?;
        child.host.borrow_mut().document_character_set = encoding.into();
        if request.response_client.id != 0 {
            child.host.borrow_mut().fetch_client = request.response_client;
        }
        if let Some(policy) = &request.response_policy {
            child.host.borrow_mut().policy = policy.clone();
        }
        child
            .context
            .as_mut()
            .ok_or("inactive child context")?
            .refresh_code_generation_policy();
        frames
            .documents
            .insert(id, FrameDocument::new(parser, request.scripts));
        request.document = id;
        Ok(Some(id))
    }
}
