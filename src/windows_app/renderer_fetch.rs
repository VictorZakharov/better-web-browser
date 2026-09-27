//! Browser-authoritative reconstruction and execution of renderer Fetch intents.

mod batch;
mod beacon;
mod clients;
mod database;
mod keepalive;
mod pump;
mod registry;
mod scheduler;
mod websocket;
mod worker;

use super::*;
use better_web_browser::fetch::{
    Body, CredentialsMode, FetchError, FetchErrorKind, FetchRequest, FetchSignal, FetchUrl,
    RedirectMode, Referrer, ReferrerPolicy, RequestCache, RequestDestination, RequestMode,
    ResponseType,
};
use better_web_browser::limits::MAX_RENDERER_FETCH_STREAM_BYTES;
use better_web_browser::renderer_process::FetchResponseSink;
use better_web_browser::renderer_protocol::{
    BrowserFetchError, BrowserFetchErrorKind, DocumentId, FetchCache, FetchCredentials,
    FetchInitiator, FetchMode, FetchRedirect, FetchReferrer, FetchReferrerPolicy,
    FetchResponseHead, FetchResponseResult, FetchResponseType, RendererFetchRequest,
    ResourceDestination, TransferChunk,
};

pub(super) struct RendererFetchCompletion {
    pub(super) document: DocumentId,
    pub(super) bytes: u64,
    pub(super) network_time: Duration,
}

pub(super) struct RendererFetchBatch {
    pub(super) tab_id: super::tabs::TabId,
    pub(super) document: DocumentId,
    pub(super) document_url: String,
    pub(super) requests: Vec<RendererFetchRequest>,
    pub(super) client: Arc<winhttp::HttpClient>,
    pub(super) signal: FetchSignal,
    pub(super) registry: RendererFetchRegistry,
    pub(super) sink: FetchResponseSink,
    pub(super) tab_router: super::browser_app::TabMessageRouter,
}

pub(super) use batch::spawn_fetch_batch;
pub(in crate::windows_app) use clients::Client as RendererFetchClient;
pub(super) use database::DatabaseWorker;
pub(super) use registry::RendererFetchRegistry;
pub(in crate::windows_app) use websocket::RendererWebSocketRegistry;

fn validate_document_identity(active: DocumentId, requested: DocumentId) -> Result<(), FetchError> {
    if requested == active {
        return Ok(());
    }
    Err(FetchError::new(
        FetchErrorKind::InvalidRequest,
        "renderer Fetch document does not match the active document",
    ))
}

fn reconstruct(
    authoritative_document_url: &str,
    renderer: RendererFetchRequest,
) -> Result<FetchRequest, FetchError> {
    let head = renderer.head;
    let body_is_empty = renderer.body.is_empty();
    let renderer_body = renderer.body;
    let mut request = match head.initiator {
        FetchInitiator::NetworkHint => {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                "DNS hints do not construct a Fetch request",
            ));
        }
        FetchInitiator::ChildNavigation => {
            if head.resulting_client.id == 0 || head.destination != ResourceDestination::Document {
                return Err(FetchError::new(
                    FetchErrorKind::InvalidRequest,
                    "invalid child navigation intent",
                ));
            }
            let mut request = FetchRequest::navigation(&head.url)?;
            request.origin = Some(FetchUrl::parse(authoritative_document_url)?.origin());
            request.referrer = trusted_referrer(authoritative_document_url, head.referrer.clone())?;
            request.response_body_limit = better_web_browser::limits::MAX_HTML_INPUT_BYTES;
            request
        }
        FetchInitiator::Subresource
        | FetchInitiator::ClassicScript
        | FetchInitiator::ChildResource => FetchRequest::subresource(
            &head.url,
            authoritative_document_url,
            destination(head.destination),
        )?,
        FetchInitiator::ModuleScript => {
            FetchRequest::script(&head.url, authoritative_document_url)?
        }
        FetchInitiator::ClassicWorker | FetchInitiator::ModuleWorker => {
            worker::reconstruct(&head, authoritative_document_url)?
        }
        FetchInitiator::ScriptApi | FetchInitiator::Beacon => {
            let mut request = FetchRequest::script(&head.url, authoritative_document_url)?;
            request.response_body_limit = MAX_RENDERER_FETCH_STREAM_BYTES;
            request.destination = destination(head.destination);
            request.set_method(&head.method)?;
            for (name, value) in &head.headers {
                request.set_script_header(name, value)?;
            }
            request.body = (!body_is_empty).then(|| Body::from_bytes(renderer_body));
            request.keepalive = head.keepalive;
            request.mode = mode(head.mode);
            request.credentials = credentials(head.credentials);
            request.cache = cache(head.cache);
            request.redirect = redirect(head.redirect);
            request.referrer_policy = referrer_policy(head.referrer_policy);
            request.referrer = trusted_referrer(authoritative_document_url, head.referrer)?;
            request
        }
    };

    if !matches!(
        head.initiator,
        FetchInitiator::ScriptApi | FetchInitiator::Beacon
    ) {
        if head.method != "GET" || !head.headers.is_empty() || !body_is_empty {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                "renderer subresource request changed fixed browser policy",
            ));
        }
        if matches!(
            head.initiator,
            FetchInitiator::ModuleScript
                | FetchInitiator::ClassicWorker
                | FetchInitiator::ModuleWorker
        ) {
            request.destination = destination(head.destination);
        }
        if matches!(
            head.initiator,
            FetchInitiator::ClassicScript
                | FetchInitiator::ModuleScript
                | FetchInitiator::ChildResource
                | FetchInitiator::ClassicWorker
                | FetchInitiator::ModuleWorker
        ) {
            request.mode = mode(head.mode);
            request.credentials = credentials(head.credentials);
            request.referrer_policy = referrer_policy(head.referrer_policy);
        }
        if head.initiator == FetchInitiator::Subresource
            && matches!(
                head.destination,
                ResourceDestination::Audio | ResourceDestination::Video
            )
        {
            // HTML media uses the potential-CORS setting. A CORS response is checked before
            // its bytes are streamed to the renderer. A no-CORS response, however, is opaque
            // only to script; the renderer still receives encoded bytes for its contained
            // decoder. Until that path can keep credentialed opaque bytes outside the renderer,
            // do not let a compromised renderer turn a media intent into a cross-origin
            // cookie-bearing read. Same-origin credentials continue to work.
            match (head.mode, head.credentials) {
                (FetchMode::NoCors, FetchCredentials::Include) => {
                    request.mode = RequestMode::NoCors;
                    request.credentials = CredentialsMode::SameOrigin;
                }
                (FetchMode::Cors, FetchCredentials::SameOrigin | FetchCredentials::Include) => {
                    request.mode = RequestMode::Cors;
                    request.credentials = credentials(head.credentials);
                }
                _ => {
                    return Err(FetchError::new(
                        FetchErrorKind::InvalidRequest,
                        "invalid media CORS setting",
                    ));
                }
            }
        }
    }
    request.script_source = head.script_source;
    Ok(request)
}

fn trusted_referrer(document_url: &str, requested: FetchReferrer) -> Result<Referrer, FetchError> {
    let document = FetchUrl::parse(document_url)?;
    match requested {
        FetchReferrer::None => Ok(Referrer::NoReferrer),
        FetchReferrer::Client => Ok(Referrer::Url(document)),
        FetchReferrer::Url(url) => {
            let requested = FetchUrl::parse(&url)?;
            if !requested.origin().is_same_origin(&document.origin()) {
                return Err(FetchError::new(
                    FetchErrorKind::InvalidRequest,
                    "renderer supplied a cross-origin referrer",
                ));
            }
            Ok(Referrer::Url(requested))
        }
    }
}

fn send_failure(
    sink: &FetchResponseSink,
    request_id: u64,
    error: &FetchError,
) -> Result<(), String> {
    sink.start(FetchResponseHead {
        request_id,
        result: FetchResponseResult::Failure(wire_error(error)),
    })?;
    sink.end(request_id, 0)
}

fn wire_error(error: &FetchError) -> BrowserFetchError {
    BrowserFetchError {
        kind: error_kind(error.kind()),
        message: error.message().chars().take(4_096).collect(),
    }
}

fn destination(value: ResourceDestination) -> RequestDestination {
    match value {
        ResourceDestination::Document => RequestDestination::Document,
        ResourceDestination::Style => RequestDestination::Style,
        ResourceDestination::Image => RequestDestination::Image,
        ResourceDestination::Script => RequestDestination::Script,
        ResourceDestination::Worker => RequestDestination::Worker,
        ResourceDestination::Font => RequestDestination::Font,
        ResourceDestination::Fetch => RequestDestination::Fetch,
        ResourceDestination::Video => RequestDestination::Video,
        ResourceDestination::Audio => RequestDestination::Audio,
    }
}

fn mode(value: FetchMode) -> RequestMode {
    match value {
        FetchMode::SameOrigin => RequestMode::SameOrigin,
        FetchMode::NoCors => RequestMode::NoCors,
        FetchMode::Cors => RequestMode::Cors,
    }
}

fn credentials(value: FetchCredentials) -> CredentialsMode {
    match value {
        FetchCredentials::Omit => CredentialsMode::Omit,
        FetchCredentials::SameOrigin => CredentialsMode::SameOrigin,
        FetchCredentials::Include => CredentialsMode::Include,
    }
}

fn cache(value: FetchCache) -> RequestCache {
    match value {
        FetchCache::Default => RequestCache::Default,
        FetchCache::NoStore => RequestCache::NoStore,
        FetchCache::Reload => RequestCache::Reload,
        FetchCache::NoCache => RequestCache::NoCache,
        FetchCache::ForceCache => RequestCache::ForceCache,
        FetchCache::OnlyIfCached => RequestCache::OnlyIfCached,
    }
}

fn redirect(value: FetchRedirect) -> RedirectMode {
    match value {
        FetchRedirect::Follow => RedirectMode::Follow,
        FetchRedirect::Error => RedirectMode::Error,
        FetchRedirect::Manual => RedirectMode::Manual,
    }
}

fn referrer_policy(value: FetchReferrerPolicy) -> ReferrerPolicy {
    match value {
        FetchReferrerPolicy::NoReferrer => ReferrerPolicy::NoReferrer,
        FetchReferrerPolicy::NoReferrerWhenDowngrade => ReferrerPolicy::NoReferrerWhenDowngrade,
        FetchReferrerPolicy::SameOrigin => ReferrerPolicy::SameOrigin,
        FetchReferrerPolicy::Origin => ReferrerPolicy::Origin,
        FetchReferrerPolicy::StrictOrigin => ReferrerPolicy::StrictOrigin,
        FetchReferrerPolicy::OriginWhenCrossOrigin => ReferrerPolicy::OriginWhenCrossOrigin,
        FetchReferrerPolicy::StrictOriginWhenCrossOrigin => {
            ReferrerPolicy::StrictOriginWhenCrossOrigin
        }
        FetchReferrerPolicy::UnsafeUrl => ReferrerPolicy::UnsafeUrl,
    }
}

fn response_type(value: ResponseType) -> FetchResponseType {
    match value {
        ResponseType::Basic => FetchResponseType::Basic,
        ResponseType::Cors => FetchResponseType::Cors,
        ResponseType::Opaque => FetchResponseType::Opaque,
        ResponseType::OpaqueRedirect => FetchResponseType::OpaqueRedirect,
    }
}

fn error_kind(value: FetchErrorKind) -> BrowserFetchErrorKind {
    match value {
        FetchErrorKind::InvalidRequest => BrowserFetchErrorKind::InvalidRequest,
        FetchErrorKind::Network => BrowserFetchErrorKind::Network,
        FetchErrorKind::Aborted => BrowserFetchErrorKind::Aborted,
        FetchErrorKind::Cors => BrowserFetchErrorKind::Cors,
        FetchErrorKind::Redirect => BrowserFetchErrorKind::Redirect,
        FetchErrorKind::BodyTooLarge => BrowserFetchErrorKind::BodyTooLarge,
    }
}

impl BrowserState {
    pub(super) unsafe fn finish_renderer_fetch_completion(
        &mut self,
        completion: RendererFetchCompletion,
    ) {
        if !self.navigation.owns_document(completion.document) {
            return;
        }
        let bytes = completion.bytes;
        let network_time = completion.network_time;
        if let Some(metrics) = self.renderer_load_metrics.as_mut() {
            metrics.bytes = metrics.bytes.saturating_add(bytes);
            metrics.network_time += network_time;
        }
        self.record_performance_activity(PerformanceActivity::Resource, network_time);
    }
}

#[cfg(test)]
#[path = "renderer_fetch/tests.rs"]
mod tests;
