//! Image subresources for child documents share Fetch/CSP and bounded Page decoding.

use super::*;
use crate::engine::Page;
use crate::engine::dom::Node;
use crate::engine::page::PageResource;
use crate::fetch::{FetchRequest, FetchResponse, RequestDestination};
use crate::limits::{
    MAX_IMAGE_SOURCE_BYTES, MAX_PAGE_DECODED_IMAGE_BYTES, MAX_PAGE_IMAGES, MAX_STYLE_IMAGES,
    MAX_URL_BYTES,
};

#[derive(Default)]
pub(super) struct FrameImages {
    scanned: Option<(u64, usize)>,
    requested: HashSet<String>,
    pub decoded: HashMap<String, crate::engine::DecodedImage>,
}

impl ScriptRuntime {
    pub(super) fn start_frame_images(&mut self) {
        let Some(frames) = &mut self.frames else {
            return;
        };
        let mut jobs = Vec::new();
        for (id, child) in &mut frames.children {
            let host = child.host.borrow();
            let state = frames.images.entry(*id).or_default();
            let revision = (
                host.document.document_mutation_version(),
                host.stylesheet_sources.len(),
            );
            if state.scanned == Some(revision) {
                continue;
            }
            state.scanned = Some(revision);
            let environment = host.media_environment;
            let mut page = Page::from_frame_document(
                host.document.clone(),
                &host.script_base_url(),
                host.stylesheet_sources.clone(),
                host.quirks_mode,
                environment,
            );
            page.images.extend(state.decoded.clone());
            page.refresh_resources_for_viewport(
                environment.viewport_width,
                environment.viewport_height,
            );
            page.install_embedded_images();
            state.decoded.extend(page.images);
            for resource in page.resources {
                let PageResource::Image { url } = resource else {
                    continue;
                };
                // data: sources were decoded inside the renderer. A network URL must fit
                // the browser/renderer wire protocol before it can become a Fetch action.
                if url.starts_with("data:")
                    || url.len() > MAX_URL_BYTES
                    || state.decoded.contains_key(&url)
                    || state.requested.len() >= MAX_PAGE_IMAGES + MAX_STYLE_IMAGES
                {
                    continue;
                }
                if state.requested.insert(url.clone()) {
                    jobs.push((*id, url));
                }
            }
        }
        for (document, url) in jobs {
            let frames = self.frames.as_mut().unwrap();
            let host = frames.children[&document].host.borrow();
            let result = FetchRequest::subresource(
                &url,
                host.inherited_url.as_deref().unwrap_or(&host.document_url),
                RequestDestination::Image,
            )
            .and_then(|mut request| {
                host.policy
                    .check_request(RequestDestination::Image, &url, 0)?;
                request.origin = Some(host.document_origin.clone());
                request.client = host.fetch_client;
                request.policy = host.policy.clone();
                request.response_body_limit = MAX_IMAGE_SOURCE_BYTES;
                Ok(request)
            })
            .map_err(|error| error.to_string())
            .and_then(|request| {
                host.fetch_identifiers
                    .borrow_mut()
                    .allocate(document)
                    .map(|id| (id, request))
                    .map_err(|error| error.to_string())
            });
            drop(host);
            match result {
                Ok((id, request)) => {
                    frames.fetches.insert(id, FrameFetch::image(document, url));
                    frames.children[&document]
                        .host
                        .borrow_mut()
                        .pending_fetch_actions
                        .push(ScriptFetchAction::Start {
                            id,
                            request: Box::new(request),
                        });
                }
                Err(error) => frames.children[&document]
                    .host
                    .borrow_mut()
                    .diagnose(format!("iframe image: {error}")),
            }
        }
    }

    pub(super) fn finish_frame_image(
        &mut self,
        document: NodeId,
        url: String,
        response: Option<FetchResponse>,
        outcome: &mut ScriptOutcome,
    ) {
        let Some(frames) = &mut self.frames else {
            return;
        };
        let Some(child) = frames.children.get_mut(&document) else {
            return;
        };
        let host = child.host.borrow();
        let mut page = Page::from_frame_document(
            host.document.clone(),
            &host.script_base_url(),
            host.stylesheet_sources.clone(),
            host.quirks_mode,
            host.media_environment,
        );
        let total_decoded = frames
            .images
            .values()
            .flat_map(|images| images.decoded.values())
            .fold(0_usize, |total, image| {
                total.saturating_add(image.bgra.len())
            });
        if let Some(state) = frames.images.get(&document) {
            page.images.extend(state.decoded.clone());
        }
        let mut result = response
            .ok_or_else(|| "image response failed".to_string())
            .and_then(|response| {
                if total_decoded >= MAX_PAGE_DECODED_IMAGE_BYTES {
                    return Err("child image memory budget exhausted".into());
                }
                page.add_image(url.clone(), response.body.as_bytes())
            });
        if result.is_ok()
            && page.images.get(&url).is_some_and(|image| {
                total_decoded.saturating_add(image.bgra.len()) > MAX_PAGE_DECODED_IMAGE_BYTES
            })
        {
            result = Err("child image memory budget exhausted".into());
        }
        let dimensions = result
            .is_ok()
            .then(|| {
                page.images
                    .get(&url)
                    .map(|image| (image.width, image.height))
            })
            .flatten();
        let targets: Vec<_> = Node::shadow_including_descendants(&host.document)
            .filter(|node| matches!(node.tag_name(), Some("img" | "image")))
            .filter(|node| page.image_url(node).as_deref() == Some(url.as_str()))
            .collect();
        drop(host);
        if result.is_ok() {
            frames.images.entry(document).or_default().decoded = page.images;
            outcome.render_requested = true;
        } else if let Err(error) = &result {
            child
                .host
                .borrow_mut()
                .diagnose(format!("iframe image: {error}"));
        }
        for target in targets {
            let (width, height) = dimensions.unwrap_or_default();
            let result = child.dispatch_user_input(UserInputEvent::ImageResource {
                target,
                event_type: if dimensions.is_some() {
                    "load"
                } else {
                    "error"
                },
                natural_width: width,
                natural_height: height,
            });
            documents::append(outcome, result.outcome);
        }
    }
}
