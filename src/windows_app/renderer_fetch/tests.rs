use super::*;

fn intent(document: DocumentId, url: &str) -> RendererFetchRequest {
    RendererFetchRequest {
        head: better_web_browser::renderer_protocol::FetchRequestHead {
            request_id: 7,
            document,
            initiator: FetchInitiator::ScriptApi,
            destination: ResourceDestination::Fetch,
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
