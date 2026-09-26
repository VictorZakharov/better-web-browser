//! Resumable response bodies: backpressure never occupies a network scheduling slot.
use super::*;
use scheduler::Step;

pub(super) enum Job {
    Request(Box<RendererFetchRequest>, FetchSignal),
    Body(Box<BodyJob>),
}

pub(super) struct BodyJob {
    id: u64,
    response: winhttp::StreamingFetchResponse,
    total: u32,
    pending: Option<TransferChunk>,
}

impl Job {
    pub(super) fn id(&self) -> u64 {
        match self {
            Self::Request(request, _) => request.head.request_id,
            Self::Body(body) => body.id,
        }
    }

    pub(super) fn started(&self) -> bool {
        matches!(self, Self::Body(_))
    }

    pub(super) fn step(
        self,
        client: &winhttp::HttpClient,
        sink: &FetchResponseSink,
        document: DocumentId,
        document_url: &str,
        registry: &RendererFetchRegistry,
    ) -> Step<Self> {
        match self {
            Self::Request(request, signal) => start(
                *request,
                signal,
                client,
                sink,
                document,
                document_url,
                registry,
            ),
            Self::Body(mut body) => match body.advance(sink) {
                Ok(true) => Step::Ready(Self::Body(body)),
                Ok(false) => Step::Parked(Self::Body(body)),
                Err(()) => Step::Done(u64::from(body.total)),
            },
        }
    }
}

fn start(
    request: RendererFetchRequest,
    signal: FetchSignal,
    client: &winhttp::HttpClient,
    sink: &FetchResponseSink,
    document: DocumentId,
    document_url: &str,
    registry: &RendererFetchRegistry,
) -> Step<Job> {
    let id = request.head.request_id;
    if request.head.initiator == FetchInitiator::NetworkHint {
        return resolve_network_hint(request, signal, client, sink, document, registry);
    }
    let target = request.head.resulting_client;
    let result = (|| {
        request
            .validate()
            .map_err(|error| FetchError::new(FetchErrorKind::InvalidRequest, error.to_string()))?;
        validate_document_identity(document, request.head.document)?;
        let (owner, policy) = {
            let mut clients = registry
                .clients
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let owner = clients.resolve(document, document_url, request.head.client)?;
            let policy = if request.head.initiator == FetchInitiator::ChildNavigation {
                clients
                    .resolve(document, document_url, request.head.embedding_client)?
                    .policy
            } else {
                owner.policy.clone()
            };
            clients.reserve(document, &request.head)?;
            (owner, policy)
        };
        let mut request = reconstruct(&owner.url, request)?;
        let creator_origin = owner.origin.clone();
        let worker_entry = request.destination == RequestDestination::Worker;
        request.origin = Some(owner.origin);
        request.policy = policy;
        let response = client.fetch_stream(request.with_signal(signal))?;
        if worker_entry
            && !response
                .url_list
                .last()
                .is_some_and(|url| url.origin().is_same_origin(&creator_origin))
        {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                "Worker entry redirect changed origin",
            ));
        }
        Ok(response)
    })();
    let response = match result {
        Ok(response) => response,
        Err(error) => {
            let _ = send_failure(sink, id, &error);
            return Step::Done(0);
        }
    };
    // A final response head establishes the child client before streamed parser
    // scripts can request subresources. Waiting for body EOF would race those requests.
    if target.id != 0 {
        let result = registry
            .clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .commit(
                document,
                target,
                response
                    .url_list
                    .last()
                    .expect("Fetch response URL")
                    .as_str(),
                &response.headers,
            );
        if let Err(error) = result {
            let _ = send_failure(sink, id, &error);
            return Step::Done(0);
        }
    }
    let head = FetchResponseHead {
        request_id: id,
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
        return Step::Done(0);
    }
    Step::Ready(Job::Body(Box::new(BodyJob {
        id,
        response,
        total: 0,
        pending: None,
    })))
}

fn resolve_network_hint(
    request: RendererFetchRequest,
    signal: FetchSignal,
    client: &winhttp::HttpClient,
    sink: &FetchResponseSink,
    document: DocumentId,
    registry: &RendererFetchRegistry,
) -> Step<Job> {
    let id = request.head.request_id;
    let result = (|| {
        request
            .validate()
            .map_err(|error| FetchError::new(FetchErrorKind::InvalidRequest, error.to_string()))?;
        validate_document_identity(document, request.head.document)?;
        if request.head.method != "GET"
            || !request.head.headers.is_empty()
            || !request.body.is_empty()
            || request.head.resulting_client.id != 0
            || request.head.script_source.is_some()
            || request.head.mode != FetchMode::NoCors
            || request.head.credentials != FetchCredentials::Omit
            || request.head.cache != FetchCache::Default
            || request.head.redirect != FetchRedirect::Follow
            || request.head.referrer != FetchReferrer::Client
            || request.head.referrer_policy != FetchReferrerPolicy::StrictOriginWhenCrossOrigin
        {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                "invalid network hint intent",
            ));
        }
        match request.head.destination {
            ResourceDestination::Fetch => {
                let should_resolve = registry
                    .clients
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .admit_network_hint(document, request.head.client, &request.head.url)?;
                if should_resolve {
                    client.warm_origin_dns(&request.head.url, &signal)?;
                }
            }
            ResourceDestination::Document => {
                let owner = registry
                    .clients
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .admit_prefetch(document, request.head.client, &request.head.url)?;
                if let Some(owner) = owner {
                    let fetch = navigation_prefetch_request(&request.head.url, &owner, signal)?;
                    // Speculative bytes never cross into the renderer. Only a complete stream may
                    // enter the browser's private HTTP cache for a matching future navigation.
                    client.fetch_stream(fetch)?.into_buffered()?;
                }
            }
            _ => {
                return Err(FetchError::new(
                    FetchErrorKind::InvalidRequest,
                    "unsupported network hint destination",
                ));
            }
        }
        Ok(())
    })();
    let response = FetchResponseHead {
        request_id: id,
        result: match result {
            Ok(()) => FetchResponseResult::Success {
                response_type: FetchResponseType::Basic,
                urls: vec![request.head.url],
                status: 204,
                headers: Vec::new(),
            },
            Err(error) => FetchResponseResult::Failure(wire_error(&error)),
        },
    };
    let _ = sink.start(response);
    let _ = sink.end(id, 0);
    Step::Done(0)
}

fn navigation_prefetch_request(
    url: &str,
    owner: &RendererFetchClient,
    signal: FetchSignal,
) -> Result<FetchRequest, FetchError> {
    // Preserve the navigation partition (including credentials). A no-CORS subresource fetch
    // would warm a different cache entry and could not safely be aliased to a navigation.
    let mut fetch = FetchRequest::navigation(url)?;
    fetch.origin = Some(owner.origin.clone());
    fetch.referrer = Referrer::Url(FetchUrl::parse(&owner.url)?);
    fetch.redirect = RedirectMode::Error;
    fetch.response_body_limit = 2 * 1024 * 1024;
    Ok(fetch.with_signal(signal))
}

impl BodyJob {
    fn advance(&mut self, sink: &FetchResponseSink) -> Result<bool, ()> {
        if self.pending.is_none() {
            match sink.has_chunk_capacity(self.id) {
                Ok(false) => return Ok(false),
                Err(_) => return self.cancel(sink),
                Ok(true) => {}
            }
            match self.response.next_chunk() {
                Ok(Some(bytes)) => {
                    self.pending = Some(TransferChunk {
                        transfer_id: self.id,
                        offset: self.total,
                        bytes,
                    })
                }
                Ok(None) => {
                    let _ = sink.end(self.id, self.total);
                    return Err(());
                }
                Err(error) => {
                    let _ = sink.abort(self.id, wire_error(&error));
                    return Err(());
                }
            }
        }
        let chunk = self.pending.take().unwrap();
        let length = chunk.bytes.len() as u32;
        match sink.try_chunk(chunk) {
            Ok(Some(chunk)) => {
                self.pending = Some(chunk);
                Ok(false)
            }
            Ok(None) => {
                self.total += length;
                Ok(true)
            }
            Err(_) => self.cancel(sink),
        }
    }

    fn cancel(&self, sink: &FetchResponseSink) -> Result<bool, ()> {
        let _ = sink.abort(
            self.id,
            BrowserFetchError {
                kind: BrowserFetchErrorKind::Aborted,
                message: "Fetch response was cancelled or retired".into(),
            },
        );
        Err(())
    }
}

#[cfg(test)]
mod prefetch_tests {
    use super::*;

    #[test]
    fn prefetch_has_future_navigation_cache_identity_without_following_redirects() {
        let source = "https://example.test/current";
        let owner = RendererFetchClient {
            policy: Default::default(),
            url: source.into(),
            origin: FetchUrl::parse(source).unwrap().origin(),
        };
        let request = navigation_prefetch_request(
            "https://example.test/next",
            &owner,
            FetchSignal::default(),
        )
        .unwrap();
        let navigation = FetchRequest::navigation("https://example.test/next").unwrap();
        assert_eq!(request.context, navigation.context);
        assert_eq!(request.destination, navigation.destination);
        assert_eq!(request.mode, navigation.mode);
        assert_eq!(request.credentials, navigation.credentials);
        assert_eq!(request.cache, navigation.cache);
        assert_eq!(request.origin, Some(owner.origin));
        assert_eq!(
            request.referrer,
            Referrer::Url(FetchUrl::parse(source).unwrap())
        );
        assert_eq!(request.redirect, RedirectMode::Error);
    }
}
