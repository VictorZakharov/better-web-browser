use super::*;

fn response(content_type: &str) -> FetchResponse {
    let mut headers = HeaderList::new();
    if !content_type.is_empty() {
        headers.append("content-type", content_type).unwrap();
    }
    FetchResponse {
        response_type: ResponseType::Basic,
        url_list: vec![FetchUrl::parse("https://example.com/module.js").unwrap()],
        status: 200,
        headers,
        body: Body::from_bytes(b"export default 1".to_vec()),
    }
}

#[test]
fn module_script_responses_require_a_javascript_mime_type() {
    for mime in [
        "text/javascript",
        "TEXT/JAVASCRIPT; charset=utf-8",
        "application/ecmascript",
        "text/javascript1.5",
    ] {
        assert!(validate_script_response(&response(mime), ScriptKind::Module).is_ok());
    }
    for mime in ["", "text/plain", "text/html", "application/json"] {
        assert!(validate_script_response(&response(mime), ScriptKind::Module).is_err());
        assert!(validate_script_response(&response(mime), ScriptKind::Classic).is_ok());
    }
}

#[test]
fn script_elements_preserve_cors_credentials_and_referrer_policy() {
    let document = DocumentId::new(1).unwrap();
    let options = crate::engine::ScriptFetchOptions::for_element(
        ScriptKind::Module,
        Some("use-credentials"),
        Some("no-referrer"),
    );
    let request = page_resource_request(
        7,
        document,
        &PageResource::Script {
            url: "https://cdn.example/module.js".into(),
            kind: ScriptKind::Module,
            fetch_options: options,
            script_source: crate::fetch::csp::ScriptSource::default(),
        },
    );
    assert_eq!(request.head.mode, FetchMode::Cors);
    assert_eq!(request.head.credentials, FetchCredentials::Include);
    assert_eq!(
        request.head.referrer_policy,
        FetchReferrerPolicy::NoReferrer
    );

    let classic = page_resource_request(
        8,
        document,
        &PageResource::Script {
            url: "https://example.com/classic.js".into(),
            kind: ScriptKind::Classic,
            fetch_options: crate::engine::ScriptFetchOptions::for_kind(ScriptKind::Classic),
            script_source: crate::fetch::csp::ScriptSource::default(),
        },
    );
    assert_eq!(classic.head.mode, FetchMode::NoCors);
    assert_eq!(classic.head.credentials, FetchCredentials::Include);
}

#[test]
fn script_preloads_keep_their_initiator_and_csp_metadata_on_the_wire() {
    let page = crate::engine::Page::parse_scripted(
        "<link rel=preload as=script href=/classic.js nonce=classic>\
         <link rel=modulepreload href=/module.mjs nonce=module>\
         <link rel=preload as=style href=/style.css>",
        "https://example.com/",
    );
    let document = DocumentId::new(1).unwrap();
    for (as_type, initiator, nonce) in [
        (PreloadAs::Script, FetchInitiator::Subresource, "classic"),
        (
            PreloadAs::ModuleScript,
            FetchInitiator::ModuleScript,
            "module",
        ),
    ] {
        let resource = page
            .resources
            .iter()
            .find(|resource| {
                matches!(resource, PageResource::Preload { as_type: kind, .. } if *kind == as_type)
            })
            .expect("script preload discovered");
        let request = page_resource_request(7, document, resource);
        assert_eq!(request.head.initiator, initiator);
        assert_eq!(request.head.destination, ResourceDestination::Script);
        let source = request.head.script_source.as_ref().unwrap();
        assert_eq!(source.nonce.as_deref(), Some(nonce));
        assert!(!source.parser_inserted);
        request.head.validate().unwrap();
        let mut malformed = request.head.clone();
        malformed.script_source.as_mut().unwrap().nonce = Some("bad nonce".into());
        assert!(matches!(
            malformed.validate(),
            Err(crate::renderer_protocol::ProtocolError::InvalidPayload(
                "renderer script CSP metadata"
            ))
        ));
    }

    let style = page
        .resources
        .iter()
        .find(|resource| {
            matches!(
                resource,
                PageResource::Preload {
                    as_type: PreloadAs::Style,
                    ..
                }
            )
        })
        .expect("style preload discovered");
    let mut request = page_resource_request(8, document, style);
    assert!(request.head.script_source.is_none());
    request.head.script_source = Some(crate::fetch::csp::ScriptSource::default());
    assert!(matches!(
        request.head.validate(),
        Err(crate::renderer_protocol::ProtocolError::InvalidPayload(
            "renderer script CSP metadata"
        ))
    ));
}

#[test]
fn media_elements_preserve_potential_cors_mode_and_credentials_on_the_wire() {
    let document = DocumentId::new(1).unwrap();
    for (crossorigin, expected_mode, expected_credentials) in [
        ("", FetchMode::NoCors, FetchCredentials::Include),
        (
            " crossorigin",
            FetchMode::Cors,
            FetchCredentials::SameOrigin,
        ),
        (
            " crossorigin=use-credentials",
            FetchMode::Cors,
            FetchCredentials::Include,
        ),
    ] {
        let html = format!("<video{crossorigin} src='https://cdn.example/video.mp4'></video>");
        let page = crate::engine::Page::parse(&html, "https://example.com/page");
        let resource = page
            .resources
            .iter()
            .find(|resource| matches!(resource, PageResource::Media { .. }))
            .expect("media source discovered");
        let request = page_resource_request(7, document, resource);
        assert_eq!(request.head.mode, expected_mode);
        assert_eq!(request.head.credentials, expected_credentials);
        assert_eq!(request.head.destination, ResourceDestination::Video);
    }
}
