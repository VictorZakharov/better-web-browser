//! Child navigation uses the shared decoder, incremental parser and encoding restart.
use super::*;
use crate::engine::dom::incremental::HtmlParser;
use crate::fetch::FetchResponse;
use crate::winhttp::{DecodedText, DocumentDecoder};

pub(super) struct Input {
    pub decoder: DocumentDecoder,
    pub restart: Option<DecodedText>,
    navigation: FrameNavigation,
    fetch: u32,
}

impl FrameDocument {
    pub(super) fn append_bytes(
        &mut self,
        bytes: &[u8],
        eof: bool,
    ) -> Result<Option<&'static str>, String> {
        let input = self.input.as_mut().ok_or("missing iframe stream")?;
        if let Some(decoded) = input.decoder.push(bytes, eof)? {
            if let Some(parser) = &mut self.parser {
                parser.append(&decoded.text, eof)?;
            }
            return Ok(Some(decoded.encoding));
        }
        Ok(None)
    }
}

impl ScriptRuntime {
    pub(super) fn start_frame_stream(
        &mut self,
        navigation: &mut FrameNavigation,
        id: u32,
        head: &FetchResponse,
    ) -> Result<(), String> {
        self.check_frame_response(navigation, head)?;
        let decoder = DocumentDecoder::new(head.content_type().unwrap_or(""));
        if let Some(document) =
            self.commit_frame_parser(navigation, HtmlParser::streaming(), "UTF-8", Some(id))?
        {
            self.frames
                .as_mut()
                .unwrap()
                .documents
                .get_mut(&document)
                .unwrap()
                .input = Some(Input {
                decoder,
                restart: None,
                navigation: navigation.clone(),
                fetch: id,
            });
        }
        Ok(())
    }

    pub(super) fn append_frame_stream(
        &mut self,
        document: NodeId,
        bytes: &[u8],
        eof: bool,
    ) -> Result<(), String> {
        let Some(frame) = self
            .frames
            .as_mut()
            .and_then(|frames| frames.documents.get_mut(&document))
        else {
            return Ok(());
        };
        if let Some(encoding) = frame.append_bytes(bytes, eof)?
            && let Some(child) = self
                .frames
                .as_ref()
                .and_then(|frames| frames.children.get(&document))
        {
            child.host.borrow_mut().document_character_set = encoding.into();
        }
        Ok(())
    }

    pub(super) fn restart_frame_encoding(&mut self, document: NodeId) -> Result<(), String> {
        let Some(frames) = &mut self.frames else {
            return Ok(());
        };
        let Some(frame) = frames.documents.get_mut(&document) else {
            return Ok(());
        };
        let Some(replay) = frame.input.as_mut().and_then(|input| input.restart.take()) else {
            return Ok(());
        };
        let mut input = frame.input.take().unwrap();
        let mut parser = HtmlParser::streaming();
        parser.append(&replay.text, input.decoder.ended())?;
        // The transport belongs to the navigation, not the old realm. Preserve it
        // while all other work from the tentatively decoded Document is cancelled.
        let fetch = self.frames.as_mut().unwrap().fetches.remove(&input.fetch);
        if let Some(new_id) = self.commit_frame_parser(
            &mut input.navigation,
            parser,
            replay.encoding,
            Some(input.fetch),
        )? {
            let frames = self.frames.as_mut().unwrap();
            if let Some(mut fetch) = fetch {
                fetch.retarget(&input.navigation);
                frames.fetches.insert(input.fetch, fetch);
            }
            frames.documents.get_mut(&new_id).unwrap().input = Some(input);
        }
        Ok(())
    }

    fn check_frame_response(
        &self,
        navigation: &mut FrameNavigation,
        head: &FetchResponse,
    ) -> Result<(), String> {
        for (redirects, url) in head.url_list.iter().enumerate() {
            navigation
                .embedding_policy
                .check_request(
                    crate::fetch::RequestDestination::Document,
                    url.as_str(),
                    redirects,
                )
                .map_err(|error| error.to_string())?;
        }
        let policy = crate::fetch::csp::PolicyContainer::from_headers(
            head.final_url().as_str(),
            &head.headers,
        )
        .map_err(|error| error.to_string())?;
        let ancestors = self
            .context
            .as_ref()
            .map(|context| context.frame_ancestor_origins(navigation.element))
            .unwrap_or_default();
        if ancestors.is_empty()
            || ancestors.iter().any(|origin| {
                policy.checks_ancestors()
                    && !policy.allows_url("frame-ancestors", &format!("{}/", origin.serialize()), 0)
            })
        {
            return Err("iframe response refused by CSP frame-ancestors".into());
        }
        if !policy.checks_ancestors()
            && head.headers.values("x-frame-options").any(|value| {
                value.split(',').any(|value| {
                    value.trim().eq_ignore_ascii_case("deny")
                        || value.trim().eq_ignore_ascii_case("sameorigin")
                            && ancestors
                                .iter()
                                .any(|origin| !origin.is_same_origin(&head.final_url().origin()))
                })
            })
        {
            return Err("iframe response refused by X-Frame-Options".into());
        }
        if head.content_type().is_some_and(|mime| {
            !mime
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/html")
        }) {
            return Err("iframe response is not an HTML document".into());
        }
        navigation.response_policy = Some(std::sync::Arc::new(policy));
        navigation.url = head.final_url().as_str().to_string();
        Ok(())
    }
}
