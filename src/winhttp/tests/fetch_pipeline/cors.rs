use super::*;

#[test]
fn validates_and_filters_cross_origin_cors_responses() {
    let server = LoopbackServer::start(|request| match request.path.as_str() {
        "/allowed" => TestResponse::new(200, b"cors body".to_vec())
            .header("Access-Control-Allow-Origin", request_origin(&request))
            .header("Access-Control-Expose-Headers", "X-Public")
            .header("Content-Type", "text/plain")
            .header("X-Public", "visible")
            .header("X-Private", "hidden")
            .header("Set-Cookie", "secret=value"),
        "/credentialed" => TestResponse::new(200, b"credentialed".to_vec())
            .header("Access-Control-Allow-Origin", request_origin(&request))
            .header("Access-Control-Allow-Credentials", "true")
            .header("Set-Cookie", "visible=yes; Path=/"),
        "/wildcard" => TestResponse::new(200, b"blocked".to_vec())
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Credentials", "true"),
        _ => TestResponse::new(200, b"blocked".to_vec()),
    });
    let client = client();
    // Different ports keep this CORS-cross-origin while the shared host keeps it
    // schemefully same-site, isolating credentials policy from SameSite.
    let document_url = "http://127.0.0.1:1/app".to_string();
    let allowed = FetchRequest::script(&server.url("/allowed"), &document_url).unwrap();
    let response = client.fetch(allowed).unwrap();
    assert_eq!(response.response_type, ResponseType::Cors);
    assert_eq!(response.body.as_bytes(), b"cors body");
    assert_eq!(response.headers.get("x-public"), Some("visible"));
    assert_eq!(response.headers.get("x-private"), None);
    assert_eq!(response.headers.get("set-cookie"), None);
    assert!(
        client
            .document_cookie_header(&server.url("/allowed"))
            .unwrap()
            .is_empty(),
        "same-origin credentials mode must not accept cross-origin cookies"
    );

    let mut credentialed =
        FetchRequest::script(&server.url("/credentialed"), &document_url).unwrap();
    credentialed.credentials = crate::fetch::CredentialsMode::Include;
    assert_eq!(
        client.fetch(credentialed).unwrap().body.as_bytes(),
        b"credentialed"
    );
    assert_eq!(
        client
            .document_cookie_header(&server.url("/credentialed"))
            .unwrap(),
        "visible=yes"
    );

    let mut wildcard = FetchRequest::script(&server.url("/wildcard"), &document_url).unwrap();
    wildcard.credentials = crate::fetch::CredentialsMode::Include;
    assert_eq!(
        client.fetch(wildcard).unwrap_err().kind(),
        FetchErrorKind::Cors,
        "credentialed responses cannot use a wildcard origin"
    );

    let blocked = FetchRequest::script(&server.url("/blocked"), &document_url).unwrap();
    assert_eq!(
        client.fetch(blocked).unwrap_err().kind(),
        FetchErrorKind::Cors
    );
}

#[test]
fn performs_and_validates_cors_preflights() {
    let captured = Arc::new(Mutex::new(Vec::<TestRequest>::new()));
    let server_captured = Arc::clone(&captured);
    let server = LoopbackServer::start(move |request| {
        let origin = request_origin(&request);
        let response = if request.method == "OPTIONS" {
            TestResponse::new(204, Vec::new())
                .header("Access-Control-Allow-Origin", origin)
                .header("Access-Control-Allow-Methods", "PUT")
                .header("Access-Control-Allow-Headers", "X-Token")
        } else {
            TestResponse::new(200, request.body.clone())
                .header("Access-Control-Allow-Origin", origin)
        };
        server_captured.lock().unwrap().push(request);
        response
    });
    let client = client();
    let document_url = format!("http://localhost:{}/app", server.port());
    let mut request = FetchRequest::script(&server.url("/write"), &document_url).unwrap();
    request.set_method("PUT").unwrap();
    request.set_script_header("X-Token", "allowed").unwrap();
    request.body = Some(Body::from_bytes(b"payload".to_vec()));
    let response = client.fetch(request).unwrap();
    assert_eq!(response.body.as_bytes(), b"payload");

    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, "OPTIONS");
    assert_eq!(
        requests[0].headers.get("access-control-request-method"),
        Some(&"PUT".to_string())
    );
    assert_eq!(
        requests[0].headers.get("access-control-request-headers"),
        Some(&"x-token".to_string())
    );
    assert_eq!(requests[1].method, "PUT");
}

#[test]
fn rejects_preflights_that_do_not_allow_a_script_header() {
    let request_count = Arc::new(AtomicUsize::new(0));
    let server_count = Arc::clone(&request_count);
    let server = LoopbackServer::start(move |request| {
        server_count.fetch_add(1, Ordering::Relaxed);
        TestResponse::new(204, Vec::new())
            .header("Access-Control-Allow-Origin", request_origin(&request))
            .header("Access-Control-Allow-Methods", "PUT")
    });
    let client = client();
    let document_url = format!("http://localhost:{}/app", server.port());
    let mut request = FetchRequest::script(&server.url("/write"), &document_url).unwrap();
    request.set_method("PUT").unwrap();
    request.set_script_header("X-Token", "denied").unwrap();
    assert_eq!(
        client.fetch(request).unwrap_err().kind(),
        FetchErrorKind::Cors
    );
    assert_eq!(request_count.load(Ordering::Relaxed), 1);
}
