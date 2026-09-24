//! Document-scoped preload matching and bounded response reuse.
//! A preload is only consumed when URL, destination, mode, credentials, referrer policy, and
//! integrity metadata match the eventual resource request. Unused bytes never enter the page.
//! https://fetch.spec.whatwg.org/#consume-a-preloaded-resource

use super::*;
use crate::engine::page::PreloadAs;
use crate::fetch::{CredentialsMode, FetchResponse, ReferrerPolicy, RequestMode, ResponseType};
use crate::renderer_protocol::{
    BrowserFetchResponse, FetchResponseHead, FetchResponseResult, FetchResponseType,
};
use std::collections::HashSet;

const MAX_PRELOAD_RESPONSES: usize = 32;
const MAX_PRELOAD_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::renderer_process::child::document) struct PreloadKey {
    url: String,
    as_type: PreloadAs,
    mode: RequestMode,
    credentials: CredentialsMode,
    referrer_policy: ReferrerPolicy,
    integrity: String,
    nonce: Option<String>,
    module: bool,
}

impl PreloadKey {
    pub(super) fn for_resource(page: &Page, resource: &PageResource) -> Option<Self> {
        let default_policy = ReferrerPolicy::StrictOriginWhenCrossOrigin;
        let (url, as_type, mode, credentials, referrer_policy, integrity, nonce, module) =
            match resource {
                PageResource::Preload {
                    url,
                    as_type,
                    mode,
                    credentials,
                    referrer_policy,
                    integrity,
                    nonce,
                } => (
                    url.clone(),
                    if *as_type == PreloadAs::ModuleScript {
                        PreloadAs::Script
                    } else {
                        *as_type
                    },
                    *mode,
                    *credentials,
                    *referrer_policy,
                    integrity.clone(),
                    nonce.clone(),
                    *as_type == PreloadAs::ModuleScript,
                ),
                PageResource::Script {
                    url,
                    kind,
                    fetch_options,
                    script_source,
                } => (
                    url.clone(),
                    PreloadAs::Script,
                    fetch_options.mode,
                    fetch_options.credentials,
                    fetch_options.referrer_policy,
                    only_integrity(page, resource)?,
                    script_source.nonce.clone(),
                    *kind == crate::engine::ScriptKind::Module,
                ),
                PageResource::Stylesheet { url } => {
                    let (mode, credentials) = match page.stylesheet_crossorigin(url) {
                        Some(value) if value.eq_ignore_ascii_case("use-credentials") => {
                            (RequestMode::Cors, CredentialsMode::Include)
                        }
                        Some(_) => (RequestMode::Cors, CredentialsMode::SameOrigin),
                        None => (RequestMode::NoCors, CredentialsMode::SameOrigin),
                    };
                    (
                        url.clone(),
                        PreloadAs::Style,
                        mode,
                        credentials,
                        default_policy,
                        only_integrity(page, resource)?,
                        None,
                        false,
                    )
                }
                PageResource::Image { url } => (
                    url.clone(),
                    PreloadAs::Image,
                    RequestMode::NoCors,
                    CredentialsMode::SameOrigin,
                    default_policy,
                    String::new(),
                    None,
                    false,
                ),
                PageResource::Font { url, .. } => (
                    url.clone(),
                    PreloadAs::Font,
                    RequestMode::Cors,
                    CredentialsMode::SameOrigin,
                    default_policy,
                    String::new(),
                    None,
                    false,
                ),
                PageResource::Media { .. } => return None,
            };
        Some(Self {
            url,
            as_type,
            mode,
            credentials,
            referrer_policy,
            integrity,
            nonce,
            module,
        })
    }
}

fn only_integrity(page: &Page, resource: &PageResource) -> Option<String> {
    let metadata = page.resource_integrity(resource);
    match metadata.as_slice() {
        [] => Some(String::new()),
        [single] => Some(single.clone()),
        // A shared URL with different integrity expectations must use normal fetch admission.
        _ => None,
    }
}

impl DocumentRuntime {
    pub(super) fn admit_cached_preloads(
        &mut self,
        connection: &mut ChildConnection,
        resources: Vec<PageResource>,
    ) -> Result<Vec<PageResource>, String> {
        let planned = resources
            .iter()
            .filter(|resource| matches!(resource, PageResource::Preload { .. }))
            .filter_map(|resource| PreloadKey::for_resource(&self.page, resource))
            .collect::<HashSet<_>>();
        let mut to_fetch = Vec::new();
        for resource in resources {
            if matches!(resource, PageResource::Preload { .. }) {
                to_fetch.push(resource);
                continue;
            }
            if let Some(response) = self.take_preload(&resource) {
                let id = connection.allocate_request_id();
                let mut owner = HashMap::from([(id, resource.clone())]);
                let mut integrity = HashMap::new();
                let metadata = self.page.resource_integrity(&resource);
                if !metadata.is_empty() {
                    integrity.insert(id, metadata);
                }
                let render = self.install_resource_responses(
                    connection,
                    vec![into_wire(response, id)],
                    &mut owner,
                    &mut integrity,
                    true,
                )?;
                self.resource_render_pending |= render;
                self.resource_style_refresh_pending |=
                    render && matches!(resource, PageResource::Stylesheet { .. });
                continue;
            }
            let key = PreloadKey::for_resource(&self.page, &resource);
            if key.as_ref().is_some_and(|key| planned.contains(key))
                || self.preload_pending(&resource)
            {
                continue;
            }
            to_fetch.push(resource);
        }
        Ok(to_fetch)
    }

    pub(super) fn store_preload(
        &mut self,
        resource: &PageResource,
        response: FetchResponse,
    ) -> bool {
        let Some(key) = PreloadKey::for_resource(&self.page, resource) else {
            return false;
        };
        let size = response.body.len();
        let replaced_size = self.preload_cache.get(&key).map_or(0, |old| old.body.len());
        if (self.preload_cache.len() >= MAX_PRELOAD_RESPONSES
            && !self.preload_cache.contains_key(&key))
            || size
                > MAX_PRELOAD_BYTES
                    .saturating_sub(self.preload_cache_bytes.saturating_sub(replaced_size))
        {
            return false;
        }
        if let Some(previous) = self.preload_cache.insert(key, response) {
            self.preload_cache_bytes = self.preload_cache_bytes.saturating_sub(previous.body.len());
        }
        self.preload_cache_bytes += size;
        true
    }

    pub(super) fn take_preload(&mut self, resource: &PageResource) -> Option<FetchResponse> {
        let key = PreloadKey::for_resource(&self.page, resource)?;
        let response = self.preload_cache.remove(&key)?;
        self.preload_cache_bytes = self.preload_cache_bytes.saturating_sub(response.body.len());
        Some(response)
    }

    pub(super) fn preload_pending(&self, resource: &PageResource) -> bool {
        let Some(key) = PreloadKey::for_resource(&self.page, resource) else {
            return false;
        };
        self.pending_resource_preloads.iter().any(|pending| {
            pending.by_request.values().any(|candidate| {
                matches!(candidate, PageResource::Preload { .. })
                    && PreloadKey::for_resource(&self.page, candidate).as_ref() == Some(&key)
            })
        })
    }
}

pub(super) fn into_wire(response: FetchResponse, request_id: u64) -> BrowserFetchResponse {
    BrowserFetchResponse {
        head: FetchResponseHead {
            request_id,
            result: FetchResponseResult::Success {
                response_type: match response.response_type {
                    ResponseType::Basic => FetchResponseType::Basic,
                    ResponseType::Cors => FetchResponseType::Cors,
                    ResponseType::Opaque => FetchResponseType::Opaque,
                    ResponseType::OpaqueRedirect => FetchResponseType::OpaqueRedirect,
                },
                urls: response
                    .url_list
                    .iter()
                    .map(|url| url.as_str().to_string())
                    .collect(),
                status: response.status,
                headers: response
                    .headers
                    .iter()
                    .map(|header| (header.name().to_string(), header.value().to_string()))
                    .collect(),
            },
        },
        body: response.body.into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_requires_same_fetch_settings_and_integrity() {
        let page = Page::parse_scripted(
            "<link rel=preload as=style href=/a.css integrity='sha256-test' crossorigin>\
             <link rel=stylesheet href=/a.css integrity='sha256-test' crossorigin>",
            "https://example.test/",
        );
        let preload = page
            .resources
            .iter()
            .find(|resource| matches!(resource, PageResource::Preload { .. }))
            .unwrap();
        let style = PageResource::Stylesheet {
            url: "https://example.test/a.css".into(),
        };
        assert_eq!(
            PreloadKey::for_resource(&page, preload),
            PreloadKey::for_resource(&page, &style)
        );
        let different = PageResource::Preload {
            url: "https://example.test/a.css".into(),
            as_type: PreloadAs::Style,
            mode: RequestMode::NoCors,
            credentials: CredentialsMode::SameOrigin,
            referrer_policy: ReferrerPolicy::StrictOriginWhenCrossOrigin,
            integrity: "sha256-test".into(),
            nonce: None,
        };
        assert_ne!(
            PreloadKey::for_resource(&page, &different),
            PreloadKey::for_resource(&page, &style)
        );
    }

    #[test]
    fn different_owner_hashes_cannot_consume_each_others_preload_bytes() {
        let page = Page::parse_scripted(
            "<link rel=preload as=style href=/shared.css integrity='sha256-first'>\
             <link rel=stylesheet href=/shared.css integrity='sha256-second'>",
            "https://example.test/",
        );
        let preload = page
            .resources
            .iter()
            .find(|resource| matches!(resource, PageResource::Preload { .. }))
            .unwrap();
        let stylesheet = PageResource::Stylesheet {
            url: "https://example.test/shared.css".into(),
        };
        assert_ne!(
            PreloadKey::for_resource(&page, preload),
            PreloadKey::for_resource(&page, &stylesheet)
        );
    }
}
