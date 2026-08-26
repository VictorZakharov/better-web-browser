//! Browser-authoritative reconstruction and execution of renderer Fetch intents.

use super::*;
use better_web_browser::fetch::{
    Body, CredentialsMode, FetchError, FetchErrorKind, FetchRequest, FetchSignal, FetchUrl,
    RedirectMode, Referrer, ReferrerPolicy, RequestCache, RequestDestination, RequestMode,
    ResponseType,
};
use better_web_browser::limits::MAX_PARALLEL_RENDERER_FETCHES;
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
    pub(super) sink: FetchResponseSink,
    pub(super) tab_router: super::browser_app::TabMessageRouter,
}

pub(super) fn spawn_fetch_batch(batch: RendererFetchBatch) -> Result<(), String> {
    let RendererFetchBatch {
        tab_id,
        document,
        document_url,
        requests,
        client,
        signal,
        sink,
        tab_router,
    } = batch;
    std::thread::Builder::new()
        .name(format!("breeze-renderer-fetch-{}", tab_id.get()))
        .spawn(move || {
            let started = Instant::now();
            let mut bytes = 0_u64;
            for batch in requests.chunks(MAX_PARALLEL_RENDERER_FETCHES) {
                let fetched = std::thread::scope(|scope| {
                    batch
                        .iter()
                        .cloned()
                        .map(|request| {
                            let request_id = request.head.request_id;
                            let client = Arc::clone(&client);
                            let signal = signal.clone();
                            let sink = sink.clone();
                            let document_url = &document_url;
                            (
                                request_id,
                                scope.spawn(move || {
                                    execute(
                                        &client,
                                        &signal,
                                        &sink,
                                        document,
                                        document_url,
                                        request,
                                    )
                                }),
                            )
                        })
                        .collect::<Vec<_>>()
                        .into_iter()
                        .map(|(request_id, worker)| {
                            worker.join().unwrap_or_else(|_| {
                                let error = FetchError::new(
                                    FetchErrorKind::Network,
                                    "browser Fetch worker panicked",
                                );
                                let _ = send_failure(&sink, request_id, &error);
                                0
                            })
                        })
                        .collect::<Vec<_>>()
                });
                for count in fetched {
                    bytes = bytes.saturating_add(count);
                }
            }
            let completion = Box::new(RendererFetchCompletion {
                document,
                bytes,
                network_time: started.elapsed(),
            });
            let pointer = Box::into_raw(completion);
            let posted = tab_router.destination(tab_id).is_some_and(|window| unsafe {
                PostMessageW(
                    window as Hwnd,
                    WM_APP_RENDERER_FETCH_COMPLETE,
                    tab_id.get() as usize,
                    pointer as isize,
                ) != 0
            });
            if !posted {
                unsafe { drop(Box::from_raw(pointer)) };
            }
        })
        .map(|_| ())
        .map_err(|error| format!("start renderer Fetch worker: {error}"))
}

fn execute(
    client: &winhttp::HttpClient,
    signal: &FetchSignal,
    sink: &FetchResponseSink,
    document: DocumentId,
    document_url: &str,
    request: RendererFetchRequest,
) -> u64 {
    let request_id = request.head.request_id;
    let result = (|| {
        request
            .validate()
            .map_err(|error| FetchError::new(FetchErrorKind::InvalidRequest, error.to_string()))?;
        validate_document_identity(document, request.head.document)?;
        let request = reconstruct(document_url, request)?.with_signal(signal.clone());
        client.fetch_stream(request)
    })();
    match result {
        Ok(response) => stream_response(sink, request_id, response),
        Err(error) => {
            let _ = send_failure(sink, request_id, &error);
            0
        }
    }
}

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
        FetchInitiator::Subresource | FetchInitiator::ClassicScript => FetchRequest::subresource(
            &head.url,
            authoritative_document_url,
            destination(head.destination),
        )?,
        FetchInitiator::ModuleScript
        | FetchInitiator::ClassicWorker
        | FetchInitiator::ModuleWorker => {
            FetchRequest::script(&head.url, authoritative_document_url)?
        }
        FetchInitiator::ScriptApi => {
            let mut request = FetchRequest::script(&head.url, authoritative_document_url)?;
            request.destination = destination(head.destination);
            request.set_method(&head.method)?;
            for (name, value) in &head.headers {
                request.set_script_header(name, value)?;
            }
            request.body = (!body_is_empty).then(|| Body::from_bytes(renderer_body));
            request.mode = mode(head.mode);
            request.credentials = credentials(head.credentials);
            request.cache = cache(head.cache);
            request.redirect = redirect(head.redirect);
            request.referrer_policy = referrer_policy(head.referrer_policy);
            request.referrer = trusted_referrer(authoritative_document_url, head.referrer)?;
            request
        }
    };

    if head.initiator != FetchInitiator::ScriptApi {
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
    }
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

fn stream_response(
    sink: &FetchResponseSink,
    request_id: u64,
    mut response: winhttp::StreamingFetchResponse,
) -> u64 {
    let head = FetchResponseHead {
        request_id,
        result: FetchResponseResult::Success {
            response_type: response_type(response.response_type),
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
    };
    if sink.start(head).is_err() {
        return 0;
    }
    let mut total = 0_u32;
    loop {
        match response.next_chunk() {
            Ok(Some(bytes)) => {
                let length = bytes.len() as u32;
                if sink
                    .chunk(TransferChunk {
                        transfer_id: request_id,
                        offset: total,
                        bytes,
                    })
                    .is_err()
                {
                    return u64::from(total);
                }
                total = total.saturating_add(length);
            }
            Ok(None) => {
                let _ = sink.end(request_id, total);
                return u64::from(total);
            }
            Err(error) => {
                let _ = sink.abort(request_id, wire_error(&error));
                return u64::from(total);
            }
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
        ResourceDestination::Style => RequestDestination::Style,
        ResourceDestination::Image => RequestDestination::Image,
        ResourceDestination::Script => RequestDestination::Script,
        ResourceDestination::Font => RequestDestination::Font,
        ResourceDestination::Fetch => RequestDestination::Fetch,
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
