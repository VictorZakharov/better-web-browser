//! Child stylesheet dependencies share the page's import discovery and CSSOM payloads.
use super::*;
use crate::engine::PageResource;
use crate::engine::css::StylesheetSource;
use crate::engine::page::{is_stylesheet, stylesheet_dependencies};
use crate::fetch::{FetchRequest, FetchResponse, RequestDestination};

#[derive(Default)]
pub(super) struct Sheets {
    requested: HashSet<String>,
    settled: HashMap<String, bool>,
    owners: HashMap<NodeId, (String, bool)>,
    pub(super) dirty: bool,
}

impl ScriptRuntime {
    pub(super) fn start_frame_styles(&mut self) {
        let Some(frames) = &mut self.frames else {
            return;
        };
        let mut outcomes = Vec::new();
        for (id, child) in &mut frames.children {
            let sheets = frames.styles.entry(*id).or_default();
            sheets.dirty = false;
            let mut jobs = Vec::new();
            let mut events = Vec::new();
            let mut blocking = false;
            {
                let host = child.host.borrow();
                let base = host.script_base_url();
                for node in Node::shadow_including_descendants(&host.document).filter(is_stylesheet)
                {
                    if Node::sheet_disabled(&node) {
                        continue;
                    }
                    if node.tag_name() == Some("style") && !host.policy.allows_style_inline() {
                        continue;
                    }
                    let signature = format!("{:?}|{}", node.attr("href"), node.text_content());
                    let owner = sheets
                        .owners
                        .entry(node.id())
                        .or_insert((signature.clone(), false));
                    if owner.0 != signature {
                        *owner = (signature, false);
                    }
                    let deps = stylesheet_dependencies(
                        &node,
                        &base,
                        &host.stylesheet_sources,
                        host.media_environment,
                    );
                    let mut failed = deps.truncated;
                    let mut pending = false;
                    for url in deps.urls {
                        if !sheets.requested.contains(&url) {
                            if sheets.requested.len() >= crate::limits::MAX_STYLESHEETS {
                                failed = true;
                                continue;
                            }
                            sheets.requested.insert(url.clone());
                            jobs.push(url.clone());
                        }
                        match sheets.settled.get(&url) {
                            None => pending = true,
                            Some(false) => failed = true,
                            Some(true) => (),
                        }
                    }
                    blocking |= pending
                        && crate::engine::css::media::media_matches_for_environment(
                            &node.attr("media").unwrap_or_default(),
                            host.media_environment,
                        );
                    if !pending && !owner.1 {
                        owner.1 = true;
                        events.push((node, if failed { "error" } else { "load" }));
                    }
                }
            }
            if let Some(document) = frames.documents.get_mut(id) {
                document.styles_pending = blocking;
                document.styles_load_pending = sheets.requested.len() != sheets.settled.len();
                document.update_pending(child);
            }
            for url in jobs {
                let integrity = {
                    let host = child.host.borrow();
                    crate::engine::page::resource_integrity(
                        &host.document,
                        &host.script_base_url(),
                        &PageResource::Stylesheet { url: url.clone() },
                    )
                };
                let result = {
                    let host = child.host.borrow();
                    host.policy
                        .check_request(RequestDestination::Style, &url, 0)
                        .and_then(|()| {
                            FetchRequest::subresource(
                                &url,
                                host.inherited_url.as_deref().unwrap_or(&host.document_url),
                                RequestDestination::Style,
                            )
                        })
                        .map(|mut request| {
                            if let Some(crossorigin) = crate::engine::page::stylesheet_crossorigin(
                                &host.document,
                                &host.script_base_url(),
                                &url,
                            ) {
                                request.mode = crate::fetch::RequestMode::Cors;
                                request.credentials =
                                    if crossorigin.eq_ignore_ascii_case("use-credentials") {
                                        crate::fetch::CredentialsMode::Include
                                    } else {
                                        crate::fetch::CredentialsMode::SameOrigin
                                    };
                            }
                            request.origin = Some(host.document_origin.clone());
                            request.client = host.fetch_client;
                            request.policy = host.policy.clone();
                            request.response_body_limit = crate::limits::MAX_CSS_SOURCE_BYTES;
                            request
                        })
                        .map_err(|error| error.to_string())
                        .and_then(|request| {
                            host.fetch_identifiers
                                .borrow_mut()
                                .allocate(*id)
                                .map(|fetch| (fetch, request))
                                .map_err(|error| error.to_string())
                        })
                };
                match result {
                    Ok((fetch, request)) => {
                        frames
                            .fetches
                            .insert(fetch, FrameFetch::style(*id, url, integrity));
                        child.host.borrow_mut().pending_fetch_actions.push(
                            ScriptFetchAction::Start {
                                id: fetch,
                                request: Box::new(request),
                            },
                        );
                    }
                    Err(error) => {
                        sheets.settled.insert(url, false);
                        sheets.dirty = true;
                        child.host.borrow_mut().diagnose(error);
                    }
                }
            }
            for (target, event_type) in events {
                let result = child
                    .dispatch_user_input(UserInputEvent::Simple {
                        target,
                        event_type,
                        bubbles: false,
                        cancelable: false,
                    })
                    .outcome;
                outcomes.push((*id, result));
            }
        }
        for (id, outcome) in outcomes {
            let outcome = self.collect_frame_result(id, outcome);
            documents::append(&mut self.frames.as_mut().unwrap().pending, outcome);
        }
    }

    pub(super) fn finish_frame_style(
        &mut self,
        document: NodeId,
        url: String,
        integrity: Vec<String>,
        response: Option<FetchResponse>,
    ) {
        let Some(frames) = &mut self.frames else {
            return;
        };
        let Some(child) = frames.children.get_mut(&document) else {
            return;
        };
        let mut host = child.host.borrow_mut();
        let response = response.filter(|response| {
            response.is_success()
                && integrity.iter().all(|value| {
                    crate::fetch::integrity::verify(
                        value,
                        response.body.as_bytes(),
                        matches!(
                            response.response_type,
                            crate::fetch::ResponseType::Basic | crate::fetch::ResponseType::Cors
                        ),
                    )
                    .is_ok()
                })
                && response.content_type().is_none_or(|mime| {
                    mime.split(';')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .eq_ignore_ascii_case("text/css")
                })
        });
        frames
            .styles
            .entry(document)
            .or_default()
            .settled
            .insert(url.clone(), response.is_some());
        if let Some(response) = response {
            let css =
                crate::winhttp::decode_text(response.body.as_bytes(), response.content_type());
            let mut source = StylesheetSource::linked(&url, css);
            source.base_url = response.final_url().as_str().into();
            host.stylesheet_sources.push(source);
        } else {
            host.diagnose(format!(
                "iframe stylesheet failed or has an invalid MIME type: {url}"
            ));
        }
    }
}
