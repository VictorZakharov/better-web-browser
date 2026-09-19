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
        request.origin = Some(owner.origin);
        request.policy = policy;
        client.fetch_stream(request.with_signal(signal))
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
