//! Translation between engine Fetch values and the authority-free renderer wire intent.

use crate::engine::page::PreloadAs;
use crate::engine::{PageResource, ScriptKind};
use crate::fetch::{
    Body, CredentialsMode, FetchError, FetchErrorKind, FetchRequest, FetchResponse, FetchUrl,
    HeaderList, RedirectMode, Referrer, ReferrerPolicy, RequestCache, RequestDestination,
    RequestMode, ResponseType,
};
use crate::renderer_protocol::{
    BrowserFetchErrorKind, BrowserFetchResponse, DocumentId, FetchCache, FetchCredentials,
    FetchInitiator, FetchMode, FetchRedirect, FetchReferrer, FetchReferrerPolicy, FetchRequestHead,
    FetchResponseHead, FetchResponseResult, FetchResponseType, RendererFetchRequest,
    ResourceDestination,
};

pub(super) fn page_resource_request(
    request_id: u64,
    document: DocumentId,
    resource: &PageResource,
) -> RendererFetchRequest {
    let (url, initiator, destination, mode) = match resource {
        PageResource::OriginHint { origin } => (
            origin,
            FetchInitiator::NetworkHint,
            ResourceDestination::Fetch,
            FetchMode::NoCors,
        ),
        PageResource::Prefetch { url } => (
            url,
            FetchInitiator::NetworkHint,
            ResourceDestination::Document,
            FetchMode::NoCors,
        ),
        PageResource::Preload {
            url,
            as_type,
            mode: request_mode,
            ..
        } => (
            url,
            if *as_type == PreloadAs::ModuleScript {
                FetchInitiator::ModuleScript
            } else {
                FetchInitiator::Subresource
            },
            match as_type {
                PreloadAs::Script | PreloadAs::ModuleScript => ResourceDestination::Script,
                PreloadAs::Style => ResourceDestination::Style,
                PreloadAs::Image => ResourceDestination::Image,
                PreloadAs::Font => ResourceDestination::Font,
            },
            mode(*request_mode),
        ),
        PageResource::Stylesheet { url } => (
            url,
            FetchInitiator::Subresource,
            ResourceDestination::Style,
            FetchMode::NoCors,
        ),
        PageResource::Image { url } => (
            url,
            FetchInitiator::Subresource,
            ResourceDestination::Image,
            FetchMode::NoCors,
        ),
        PageResource::Media {
            url,
            kind,
            mode: request_mode,
            ..
        } => (
            url,
            FetchInitiator::Subresource,
            match kind {
                crate::engine::page::MediaElementKind::Audio => ResourceDestination::Audio,
                crate::engine::page::MediaElementKind::Video => ResourceDestination::Video,
            },
            mode(*request_mode),
        ),
        PageResource::Script {
            url,
            kind,
            fetch_options,
            ..
        } => (
            url,
            match kind {
                ScriptKind::Classic => FetchInitiator::ClassicScript,
                ScriptKind::Module => FetchInitiator::ModuleScript,
            },
            ResourceDestination::Script,
            mode(fetch_options.mode),
        ),
        PageResource::Font { url, .. } => (
            url,
            FetchInitiator::Subresource,
            ResourceDestination::Font,
            FetchMode::Cors,
        ),
    };
    RendererFetchRequest {
        head: FetchRequestHead {
            client: Default::default(),
            resulting_client: Default::default(),
            embedding_client: Default::default(),
            request_id,
            document,
            initiator,
            destination,
            script_source: match resource {
                PageResource::Script { script_source, .. } => Some(script_source.clone()),
                PageResource::Preload {
                    as_type: PreloadAs::Script | PreloadAs::ModuleScript,
                    nonce,
                    ..
                } => Some(crate::fetch::csp::ScriptSource {
                    nonce: nonce.clone(),
                    parser_inserted: false,
                }),
                _ => None,
            },
            url: url.clone(),
            method: "GET".into(),
            headers: Vec::new(),
            mode,
            credentials: match resource {
                PageResource::OriginHint { .. } | PageResource::Prefetch { .. } => {
                    FetchCredentials::Omit
                }
                PageResource::Preload {
                    credentials: value, ..
                } => credentials(*value),
                PageResource::Script { fetch_options, .. } => {
                    credentials(fetch_options.credentials)
                }
                PageResource::Media {
                    credentials: value, ..
                } => credentials(*value),
                _ => FetchCredentials::SameOrigin,
            },
            cache: FetchCache::Default,
            redirect: FetchRedirect::Follow,
            referrer: FetchReferrer::Client,
            referrer_policy: match resource {
                PageResource::Preload {
                    referrer_policy: value,
                    ..
                } => referrer_policy(*value),
                PageResource::Script { fetch_options, .. } => {
                    referrer_policy(fetch_options.referrer_policy)
                }
                _ => FetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            },
            body_length: 0,
            keepalive: false,
        },
        body: Vec::new(),
    }
}

pub(super) fn script_api_request(
    request_id: u64,
    document: DocumentId,
    request: FetchRequest,
) -> RendererFetchRequest {
    let body = request
        .body
        .as_ref()
        .map(|body| body.as_bytes().to_vec())
        .unwrap_or_default();
    RendererFetchRequest {
        head: FetchRequestHead {
            client: request.client,
            resulting_client: request.resulting_client,
            embedding_client: request.embedding_client,
            request_id,
            document,
            initiator: if request.context == crate::fetch::RequestContext::WorkerScript {
                if request.mode == RequestMode::Cors {
                    FetchInitiator::ModuleWorker
                } else {
                    FetchInitiator::ClassicWorker
                }
            } else if request.resulting_client.id != 0 {
                FetchInitiator::ChildNavigation
            } else if request.context == crate::fetch::RequestContext::Subresource {
                FetchInitiator::ChildResource
            } else {
                FetchInitiator::ScriptApi
            },
            destination: destination(request.destination),
            script_source: request.script_source,
            url: request.url.as_str().to_string(),
            method: request.method,
            headers: request
                .headers
                .iter()
                .map(|header| (header.name().to_string(), header.value().to_string()))
                .collect(),
            mode: mode(request.mode),
            credentials: credentials(request.credentials),
            cache: cache(request.cache),
            redirect: redirect(request.redirect),
            referrer: referrer(request.referrer),
            referrer_policy: referrer_policy(request.referrer_policy),
            body_length: body.len() as u32,
            keepalive: request.keepalive,
        },
        body,
    }
}

pub(super) fn script_beacon_request(
    request_id: u64,
    document: DocumentId,
    request: FetchRequest,
) -> RendererFetchRequest {
    let mut wire = script_api_request(request_id, document, request);
    wire.head.initiator = FetchInitiator::Beacon;
    wire
}

pub(super) fn into_fetch_result(
    response: BrowserFetchResponse,
) -> Result<FetchResponse, FetchError> {
    match response.head.result {
        FetchResponseResult::Success {
            response_type,
            urls,
            status,
            headers,
        } => {
            let urls = urls
                .into_iter()
                .map(|url| FetchUrl::parse(&url))
                .collect::<Result<Vec<_>, _>>()?;
            let mut header_list = HeaderList::new();
            for (name, value) in headers {
                header_list.append(&name, &value)?;
            }
            Ok(FetchResponse {
                response_type: response_type_from_wire(response_type),
                url_list: urls,
                status,
                headers: header_list,
                body: Body::from_bytes(response.body),
            })
        }
        FetchResponseResult::Failure(error) => Err(FetchError::new(
            error_kind_from_wire(error.kind),
            error.message,
        )),
    }
}

pub(super) fn into_fetch_head_result(head: FetchResponseHead) -> Result<FetchResponse, FetchError> {
    into_fetch_result(BrowserFetchResponse {
        head,
        body: Vec::new(),
    })
}

pub(super) fn into_fetch_error(error: crate::renderer_protocol::BrowserFetchError) -> FetchError {
    FetchError::new(error_kind_from_wire(error.kind), error.message)
}

pub(super) use crate::engine::script::network::response::validate_script_response;

fn destination(value: RequestDestination) -> ResourceDestination {
    match value {
        RequestDestination::Style => ResourceDestination::Style,
        RequestDestination::Image => ResourceDestination::Image,
        RequestDestination::Script => ResourceDestination::Script,
        RequestDestination::Worker => ResourceDestination::Worker,
        RequestDestination::Font => ResourceDestination::Font,
        RequestDestination::Document => ResourceDestination::Document,
        RequestDestination::Fetch => ResourceDestination::Fetch,
        RequestDestination::Video => ResourceDestination::Video,
        RequestDestination::Audio => ResourceDestination::Audio,
    }
}

fn mode(value: RequestMode) -> FetchMode {
    match value {
        RequestMode::SameOrigin => FetchMode::SameOrigin,
        RequestMode::NoCors => FetchMode::NoCors,
        RequestMode::Cors => FetchMode::Cors,
        RequestMode::Navigate => FetchMode::Cors,
    }
}

fn credentials(value: CredentialsMode) -> FetchCredentials {
    match value {
        CredentialsMode::Omit => FetchCredentials::Omit,
        CredentialsMode::SameOrigin => FetchCredentials::SameOrigin,
        CredentialsMode::Include => FetchCredentials::Include,
    }
}

fn cache(value: RequestCache) -> FetchCache {
    match value {
        RequestCache::Default => FetchCache::Default,
        RequestCache::NoStore => FetchCache::NoStore,
        RequestCache::Reload => FetchCache::Reload,
        RequestCache::NoCache => FetchCache::NoCache,
        RequestCache::ForceCache => FetchCache::ForceCache,
        RequestCache::OnlyIfCached => FetchCache::OnlyIfCached,
    }
}

fn redirect(value: RedirectMode) -> FetchRedirect {
    match value {
        RedirectMode::Follow => FetchRedirect::Follow,
        RedirectMode::Error => FetchRedirect::Error,
        RedirectMode::Manual => FetchRedirect::Manual,
    }
}

fn referrer(value: Referrer) -> FetchReferrer {
    match value {
        Referrer::NoReferrer => FetchReferrer::None,
        Referrer::Url(url) => FetchReferrer::Url(url.as_str().to_string()),
    }
}

fn referrer_policy(value: ReferrerPolicy) -> FetchReferrerPolicy {
    match value {
        ReferrerPolicy::NoReferrer => FetchReferrerPolicy::NoReferrer,
        ReferrerPolicy::NoReferrerWhenDowngrade => FetchReferrerPolicy::NoReferrerWhenDowngrade,
        ReferrerPolicy::SameOrigin => FetchReferrerPolicy::SameOrigin,
        ReferrerPolicy::Origin => FetchReferrerPolicy::Origin,
        ReferrerPolicy::StrictOrigin => FetchReferrerPolicy::StrictOrigin,
        ReferrerPolicy::OriginWhenCrossOrigin => FetchReferrerPolicy::OriginWhenCrossOrigin,
        ReferrerPolicy::StrictOriginWhenCrossOrigin => {
            FetchReferrerPolicy::StrictOriginWhenCrossOrigin
        }
        ReferrerPolicy::UnsafeUrl => FetchReferrerPolicy::UnsafeUrl,
    }
}

fn response_type_from_wire(value: FetchResponseType) -> ResponseType {
    match value {
        FetchResponseType::Basic => ResponseType::Basic,
        FetchResponseType::Cors => ResponseType::Cors,
        FetchResponseType::Opaque => ResponseType::Opaque,
        FetchResponseType::OpaqueRedirect => ResponseType::OpaqueRedirect,
    }
}

fn error_kind_from_wire(value: BrowserFetchErrorKind) -> FetchErrorKind {
    match value {
        BrowserFetchErrorKind::InvalidRequest => FetchErrorKind::InvalidRequest,
        BrowserFetchErrorKind::Network => FetchErrorKind::Network,
        BrowserFetchErrorKind::Aborted => FetchErrorKind::Aborted,
        BrowserFetchErrorKind::Cors => FetchErrorKind::Cors,
        BrowserFetchErrorKind::Redirect => FetchErrorKind::Redirect,
        BrowserFetchErrorKind::BodyTooLarge => FetchErrorKind::BodyTooLarge,
    }
}

#[cfg(test)]
mod tests;
