//! Frame navigation admission and document replacement, outside author-script stacks.
use super::*;
use crate::engine::dom::incremental::HtmlParser;
use crate::fetch::{FetchRequest, FetchUrl, Referrer};

impl ScriptRuntime {
    pub(super) fn advance_frame_navigation(&mut self) -> ScriptOutcome {
        let Some(request) = self
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
                        Err(error) => outcome.errors.push(error.to_string()),
                    }
                }
                Err(error) => outcome
                    .diagnostics
                    .push(format!("iframe navigation: {error}")),
            }
        }
        outcome
    }

    pub(super) fn commit_frame(
        &mut self,
        request: FrameNavigation,
        source: String,
        encoding: &str,
    ) -> Result<(), String> {
        if !self
            .context
            .as_ref()
            .is_some_and(|context| context.frame_navigation_current(&request))
        {
            return Ok(());
        }
        let parser = HtmlParser::new(&source);
        let document = parser.dom().document.clone();
        let id = self
            .context
            .as_deref_mut()
            .ok_or("inactive frame tree")?
            .replace_frame(request.element, document, request.url)
            .map_err(|error| error.to_string())?
            .ok_or("frame was removed during navigation")?;
        self.sync_child_runtimes();
        let frames = self.frames.as_mut().ok_or("inactive frame scheduler")?;
        let child = frames
            .children
            .get_mut(&id)
            .ok_or("replacement frame was not registered")?;
        child.host.borrow_mut().document_character_set = encoding.into();
        frames.documents.insert(
            id,
            FrameDocument::new(request.element, request.epoch, parser, request.scripts),
        );
        Ok(())
    }
}
