use super::*;

pub(super) fn intent(document: DocumentId, url: &str) -> RendererFetchRequest {
    RendererFetchRequest {
        head: better_web_browser::renderer_protocol::FetchRequestHead {
            client: Default::default(),
            resulting_client: Default::default(),
            embedding_client: Default::default(),
            request_id: 7,
            document,
            initiator: FetchInitiator::ScriptApi,
            destination: ResourceDestination::Fetch,
            script_source: None,
            url: url.into(),
            method: "GET".into(),
            headers: Vec::new(),
            mode: FetchMode::Cors,
            credentials: FetchCredentials::SameOrigin,
            cache: FetchCache::Default,
            redirect: FetchRedirect::Follow,
            referrer: FetchReferrer::Client,
            referrer_policy: FetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            body_length: 0,
        },
        body: Vec::new(),
    }
}

#[test]
fn reconstructs_client_referrer_from_the_authoritative_document() {
    let document = DocumentId::new(1).unwrap();
    let request = reconstruct(
        "https://example.test/page",
        intent(document, "https://api.example.test/data"),
    )
    .unwrap();
    assert_eq!(
        request.referrer,
        Referrer::Url(FetchUrl::parse("https://example.test/page").unwrap())
    );
    assert_eq!(request.response_body_limit, MAX_RENDERER_FETCH_STREAM_BYTES);
}

#[test]
fn rejects_renderer_supplied_cross_origin_referrers() {
    let document = DocumentId::new(1).unwrap();
    let mut request = intent(document, "https://example.test/data");
    request.head.referrer = FetchReferrer::Url("https://attacker.test/leak".into());
    let error = reconstruct("https://example.test/page", request).unwrap_err();
    assert_eq!(error.kind(), FetchErrorKind::InvalidRequest);
}

#[test]
fn rejects_script_forbidden_headers_during_reconstruction() {
    let document = DocumentId::new(1).unwrap();
    let mut request = intent(document, "https://example.test/data");
    request
        .head
        .headers
        .push(("cookie".into(), "stolen=1".into()));
    let error = reconstruct("https://example.test/page", request).unwrap_err();
    assert_eq!(error.kind(), FetchErrorKind::InvalidRequest);
}

#[test]
fn execute_rejects_a_stale_document_identity() {
    let active = DocumentId::new(1).unwrap();
    let stale = DocumentId::new(2).unwrap();
    let error = validate_document_identity(active, stale).unwrap_err();
    assert_eq!(error.kind(), FetchErrorKind::InvalidRequest);
}

#[test]
fn worker_script_bytes_are_internal_and_request_modes_are_fixed() {
    let document = DocumentId::new(1).unwrap();
    let mut request = intent(document, "https://cdn.example.test/import.js");
    request.head.initiator = FetchInitiator::ClassicWorker;
    request.head.destination = ResourceDestination::Script;
    request.head.mode = FetchMode::NoCors;
    let accepted = reconstruct("https://example.test/worker.js", request.clone()).unwrap();
    assert_eq!(
        accepted.context,
        better_web_browser::fetch::RequestContext::WorkerScript
    );
    assert_eq!(accepted.destination, RequestDestination::Script);
    request.head.mode = FetchMode::Cors;
    assert_eq!(
        reconstruct("https://example.test/worker.js", request.clone())
            .unwrap_err()
            .kind(),
        FetchErrorKind::InvalidRequest
    );
    request.head.mode = FetchMode::NoCors;
    request.head.destination = ResourceDestination::Fetch;
    assert_eq!(
        reconstruct("https://example.test/worker.js", request)
            .unwrap_err()
            .kind(),
        FetchErrorKind::InvalidRequest
    );
}

#[test]
fn module_worker_entry_cannot_claim_a_cross_origin_response_client() {
    let document = DocumentId::new(1).unwrap();
    let mut request = intent(document, "https://foreign.test/worker.js");
    request.head.initiator = FetchInitiator::ModuleWorker;
    request.head.destination = ResourceDestination::Worker;
    request.head.resulting_client.id = (1_u64 << 63) | 1;
    assert_eq!(
        reconstruct("https://example.test/page", request)
            .unwrap_err()
            .kind(),
        FetchErrorKind::InvalidRequest
    );
}
