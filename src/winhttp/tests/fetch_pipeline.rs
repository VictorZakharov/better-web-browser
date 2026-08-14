use super::WINHTTP_ACCESS_TYPE_NO_PROXY;
use super::support::{LoopbackServer, TestRequest, TestResponse};
use crate::fetch::{
    Body, FetchController, FetchErrorKind, FetchRequest, RedirectMode, RequestDestination,
    ResponseType,
};
use crate::winhttp::HttpClient;
use std::net::TcpListener;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

fn client() -> HttpClient {
    HttpClient::with_access_type(WINHTTP_ACCESS_TYPE_NO_PROXY).unwrap()
}

#[test]
fn applies_redirect_modes_and_cookies_between_hops() {
    let server = LoopbackServer::start(|request| match request.path.as_str() {
        "/start" => TestResponse::new(302, Vec::new())
            .header("Location", "/final")
            .header("Set-Cookie", "redirect-token=present; Path=/; HttpOnly"),
        "/final" => TestResponse::new(
            200,
            request.headers.get("cookie").cloned().unwrap_or_default(),
        ),
        "/rewrite" => TestResponse::new(302, Vec::new()).header("Location", "/rewritten"),
        "/rewritten" => TestResponse::new(
            200,
            format!(
                "{}:{}",
                request.method,
                String::from_utf8_lossy(&request.body)
            )
            .into_bytes(),
        ),
        "/preserve" => TestResponse::new(307, Vec::new()).header("Location", "/preserved"),
        "/preserved" => TestResponse::new(
            200,
            format!(
                "{}:{}",
                request.method,
                String::from_utf8_lossy(&request.body)
            )
            .into_bytes(),
        ),
        _ => TestResponse::new(404, Vec::new()),
    });
    let client = client();
    let start = server.url("/start");

    let followed = client
        .fetch(FetchRequest::navigation(&start).unwrap())
        .unwrap();
    assert_eq!(followed.status, 200);
    assert_eq!(followed.url_list.len(), 2);
    assert_eq!(followed.final_url().as_str(), server.url("/final"));
    assert_eq!(followed.body.as_bytes(), b"redirect-token=present");
    assert!(
        client
            .document_cookie_header(&server.url("/final"))
            .unwrap()
            .is_empty(),
        "HttpOnly cookies must not be exposed through document.cookie"
    );
    client
        .set_cookie(&server.url("/final"), "redirect-token=overwritten; Path=/")
        .unwrap();
    assert_eq!(
        client.get(&server.url("/final")).unwrap().body.as_bytes(),
        b"redirect-token=present",
        "document.cookie must not overwrite an HttpOnly cookie"
    );

    let mut rejected = FetchRequest::navigation(&start).unwrap();
    rejected.redirect = RedirectMode::Error;
    assert_eq!(
        client.fetch(rejected).unwrap_err().kind(),
        FetchErrorKind::Redirect
    );

    let mut manual = FetchRequest::navigation(&start).unwrap();
    manual.redirect = RedirectMode::Manual;
    let manual = client.fetch(manual).unwrap();
    assert_eq!(manual.status, 302);
    assert_eq!(manual.url_list.len(), 1);

    let mut script_manual = FetchRequest::script(&start, &server.url("/document")).unwrap();
    script_manual.redirect = RedirectMode::Manual;
    let script_manual = client.fetch(script_manual).unwrap();
    assert_eq!(script_manual.response_type, ResponseType::OpaqueRedirect);
    assert_eq!(script_manual.status, 0);
    assert!(script_manual.body.is_empty());

    let mut rewritten = FetchRequest::navigation(&server.url("/rewrite")).unwrap();
    rewritten.set_method("POST").unwrap();
    rewritten.body = Some(Body::from_bytes(b"payload".to_vec()));
    assert_eq!(client.fetch(rewritten).unwrap().body.as_bytes(), b"GET:");

    let mut preserved = FetchRequest::navigation(&server.url("/preserve")).unwrap();
    preserved.set_method("POST").unwrap();
    preserved.body = Some(Body::from_bytes(b"payload".to_vec()));
    assert_eq!(
        client.fetch(preserved).unwrap().body.as_bytes(),
        b"POST:payload"
    );
}

#[test]
fn rejects_forbidden_headers_even_if_the_script_guard_is_bypassed() {
    let client = client();
    let mut request =
        FetchRequest::script("http://127.0.0.1:1/", "http://127.0.0.1:1/document").unwrap();
    request.headers.append("Cookie", "injected=yes").unwrap();
    let error = client.fetch(request).unwrap_err();
    assert_eq!(error.kind(), FetchErrorKind::InvalidRequest);
}

#[test]
fn scopes_response_cookies_by_path() {
    let server = LoopbackServer::start(|request| match request.path.as_str() {
        "/set" => TestResponse::new(200, b"stored".to_vec())
            .header("Set-Cookie", "scoped=yes; Path=/allowed"),
        _ => TestResponse::new(
            200,
            request.headers.get("cookie").cloned().unwrap_or_default(),
        ),
    });
    let client = client();
    client.get(&server.url("/set")).unwrap();
    assert_eq!(
        client
            .document_cookie_header(&server.url("/allowed/child"))
            .unwrap(),
        "scoped=yes"
    );
    assert_eq!(
        client
            .get(&server.url("/allowed/child"))
            .unwrap()
            .body
            .as_bytes(),
        b"scoped=yes"
    );
    assert!(client.get(&server.url("/outside")).unwrap().body.is_empty());
}

#[test]
fn shared_document_abort_stops_outstanding_subresources() {
    let (started_tx, started_rx) = mpsc::channel();
    let server = LoopbackServer::start(move |_| {
        started_tx.send(()).unwrap();
        TestResponse::new(200, vec![b'x'; 512 * 1024]).streamed(1024, Duration::from_millis(3))
    });
    let client = Arc::new(client());
    let controller = FetchController::new();
    let document_url = server.url("/document");
    let mut workers = Vec::new();
    for path in ["/slow-a", "/slow-b"] {
        let client = Arc::clone(&client);
        let signal = controller.signal();
        let request =
            FetchRequest::subresource(&server.url(path), &document_url, RequestDestination::Image)
                .unwrap()
                .with_signal(signal);
        workers.push(std::thread::spawn(move || client.fetch(request)));
    }
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let aborted_at = Instant::now();
    controller.abort();
    for worker in workers {
        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(error.kind(), FetchErrorKind::Aborted);
    }
    assert!(aborted_at.elapsed() < Duration::from_secs(2));
}

#[test]
fn keeps_http_errors_distinct_from_network_failures() {
    let server = LoopbackServer::start(|_| TestResponse::new(503, b"retry later".to_vec()));
    let client = client();
    let response = client.get(&server.url("/unavailable")).unwrap();
    assert_eq!(response.status, 503);
    assert_eq!(response.body.as_bytes(), b"retry later");

    let unused = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = unused.local_addr().unwrap().port();
    drop(unused);
    let request = FetchRequest::navigation(&format!("http://127.0.0.1:{port}/closed")).unwrap();
    let error = client.fetch(request).unwrap_err();
    assert_eq!(error.kind(), FetchErrorKind::Network);
}

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
    let document_url = format!("http://localhost:{}/app", server.port());
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

#[test]
fn applies_referrer_reduction_and_response_body_budgets() {
    let server = LoopbackServer::start(|request| {
        if request.path == "/large" {
            TestResponse::new(200, b"12345".to_vec())
        } else {
            TestResponse::new(
                200,
                request.headers.get("referer").cloned().unwrap_or_default(),
            )
        }
    });
    let client = client();
    let same_origin_document = server.url("/source?q=1");
    let same_origin = FetchRequest::subresource(
        &server.url("/referrer"),
        &same_origin_document,
        RequestDestination::Image,
    )
    .unwrap();
    assert_eq!(
        client.fetch(same_origin).unwrap().body.as_bytes(),
        same_origin_document.as_bytes()
    );

    let cross_origin_document = format!("http://localhost:{}/source", server.port());
    let cross_origin = FetchRequest::subresource(
        &server.url("/referrer"),
        &cross_origin_document,
        RequestDestination::Image,
    )
    .unwrap();
    assert_eq!(
        client.fetch(cross_origin).unwrap().body.as_bytes(),
        format!("http://localhost:{}/", server.port()).as_bytes()
    );

    let limited = FetchRequest::navigation(&server.url("/large"))
        .unwrap()
        .with_response_body_limit(4);
    assert_eq!(
        client.fetch(limited).unwrap_err().kind(),
        FetchErrorKind::BodyTooLarge
    );
}

fn request_origin(request: &TestRequest) -> String {
    request.headers.get("origin").cloned().unwrap_or_default()
}
