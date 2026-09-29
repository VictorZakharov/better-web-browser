//! DNS hints and link prefetches are browser-owned, bounded background work.
use super::*;

pub(super) fn resolve_network_hint(
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
            || request.head.cache != FetchCache::Default
            || request.head.redirect != FetchRedirect::Follow
            || request.head.referrer != FetchReferrer::Client
            || request.head.keepalive
        {
            return Err(invalid("invalid network hint intent"));
        }
        match request.head.destination {
            ResourceDestination::Fetch => {
                if request.head.mode != FetchMode::NoCors
                    || request.head.credentials != FetchCredentials::Omit
                    || request.head.referrer_policy
                        != FetchReferrerPolicy::StrictOriginWhenCrossOrigin
                {
                    return Err(invalid("invalid DNS hint settings"));
                }
                let should_resolve = registry
                    .clients
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .admit_network_hint(document, request.head.client, &request.head.url)?;
                if should_resolve {
                    client.warm_origin_dns(&request.head.url, &signal)?;
                }
            }
            // Document is a wire-level hint kind, not its actual Fetch destination.
            ResourceDestination::Document => {
                let owner = registry
                    .clients
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .admit_prefetch(document, &request.head)?;
                if let Some(owner) = owner {
                    let fetch = prefetch_request(&request.head, &owner, signal)?;
                    // Nothing, not even response headers, crosses into the renderer.
                    client.fetch_stream(fetch)?.into_buffered()?;
                }
            }
            _ => return Err(invalid("unsupported network hint destination")),
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

fn prefetch_request(
    head: &better_web_browser::renderer_protocol::FetchRequestHead,
    owner: &RendererFetchClient,
    signal: FetchSignal,
) -> Result<FetchRequest, FetchError> {
    let (mode, credentials) = match (head.mode, head.credentials) {
        (FetchMode::NoCors, FetchCredentials::Include) => {
            (RequestMode::NoCors, CredentialsMode::Include)
        }
        (FetchMode::Cors, FetchCredentials::SameOrigin) => {
            (RequestMode::Cors, CredentialsMode::SameOrigin)
        }
        (FetchMode::Cors, FetchCredentials::Include) => {
            (RequestMode::Cors, CredentialsMode::Include)
        }
        _ => return Err(invalid("invalid potential-CORS prefetch settings")),
    };
    let mut fetch = FetchRequest::prefetch(&head.url, &owner.url)?;
    fetch.origin = Some(owner.origin.clone());
    fetch.policy = owner.policy.clone();
    fetch.mode = mode;
    fetch.credentials = credentials;
    fetch.referrer_policy = super::super::referrer_policy(head.referrer_policy);
    Ok(fetch.with_signal(signal))
}

fn invalid(message: &str) -> FetchError {
    FetchError::new(FetchErrorKind::InvalidRequest, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefetch_uses_empty_fetch_destination_and_potential_cors_settings() {
        let source = "https://example.test/current";
        let owner = RendererFetchClient {
            policy: Default::default(),
            url: source.into(),
            origin: FetchUrl::parse(source).unwrap().origin(),
        };
        let document = DocumentId::new(1).unwrap();
        for (wire_mode, wire_credentials, mode, credentials) in [
            (
                FetchMode::NoCors,
                FetchCredentials::Include,
                RequestMode::NoCors,
                CredentialsMode::Include,
            ),
            (
                FetchMode::Cors,
                FetchCredentials::SameOrigin,
                RequestMode::Cors,
                CredentialsMode::SameOrigin,
            ),
            (
                FetchMode::Cors,
                FetchCredentials::Include,
                RequestMode::Cors,
                CredentialsMode::Include,
            ),
        ] {
            let mut head = crate::windows_app::renderer_fetch::tests::intent(
                document,
                "https://cdn.test/asset",
            )
            .head;
            head.mode = wire_mode;
            head.credentials = wire_credentials;
            head.referrer_policy = FetchReferrerPolicy::NoReferrer;
            let request = prefetch_request(&head, &owner, FetchSignal::default()).unwrap();
            assert_eq!(
                request.context,
                better_web_browser::fetch::RequestContext::Prefetch
            );
            assert_eq!(request.destination, RequestDestination::Fetch);
            assert_eq!(request.mode, mode);
            assert_eq!(request.credentials, credentials);
            assert_eq!(request.referrer_policy, ReferrerPolicy::NoReferrer);
            assert_eq!(
                request.referrer,
                Referrer::Url(FetchUrl::parse(source).unwrap())
            );
            assert_eq!(request.redirect, RedirectMode::Follow);
        }
    }
}
