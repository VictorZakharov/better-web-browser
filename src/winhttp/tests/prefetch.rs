//! Complete link-prefetch reuse and partition boundaries.
use super::support::{LoopbackServer, TestResponse};
use crate::fetch::{
    CredentialsMode, FetchErrorKind, FetchRequest, HeaderList, ReferrerPolicy, RequestDestination,
    RequestMode, csp::PolicyContainer,
};
use crate::winhttp::HttpClient;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn client() -> HttpClient {
    HttpClient::with_access_type(super::WINHTTP_ACCESS_TYPE_NO_PROXY).unwrap()
}

fn source_and_target(server: &LoopbackServer) -> (String, String) {
    (server.url("/page"), server.url("/asset"))
}

#[test]
fn secure_document_prefetch_blocks_insecure_target_before_dispatch() {
    let request = FetchRequest::prefetch(
        "http://insecure.example.test/asset",
        "https://secure.example.test/page",
    )
    .unwrap();
    let error = client().fetch(request).unwrap_err();
    assert_eq!(error.kind(), FetchErrorKind::Network);
    assert_eq!(error.message(), "Mixed content blocked prefetch");
}

#[test]
fn secure_document_prefetch_blocks_insecure_redirect_target() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            TestResponse::new(302, Vec::new())
                .header("Location", "http://insecure.example.test/asset")
        }
    });
    // HTTP loopback is potentially trustworthy, so the first hop is allowed.
    let request =
        FetchRequest::prefetch(&server.url("/redirect"), "https://secure.example.test/page")
            .unwrap();
    let error = client().fetch(request).unwrap_err();
    assert_eq!(error.kind(), FetchErrorKind::Network);
    assert_eq!(error.message(), "Mixed content blocked prefetch");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn default_same_origin_image_reuses_complete_prefetch() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("asset-{count}")).header("Cache-Control", "max-age=60")
        }
    });
    let client = client();
    let (source, target) = source_and_target(&server);
    client
        .fetch(FetchRequest::prefetch(&target, &source).unwrap())
        .unwrap();
    let image = FetchRequest::subresource(&target, &source, RequestDestination::Image).unwrap();
    assert_eq!(client.fetch(image).unwrap().body.as_bytes(), b"asset-1");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn cors_prefetch_reuses_compatible_cross_origin_subresource() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |request| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("asset-{count}"))
                .header("Cache-Control", "max-age=60")
                .header(
                    "Access-Control-Allow-Origin",
                    request.headers.get("origin").cloned().unwrap_or_default(),
                )
        }
    });
    let client = client();
    let target = server.url("/asset");
    // Another port is a different origin even when both servers bind loopback.
    let source = "http://127.0.0.1:1/page";
    let mut hint = FetchRequest::prefetch(&target, source).unwrap();
    hint.mode = RequestMode::Cors;
    hint.credentials = CredentialsMode::SameOrigin;
    assert_eq!(client.fetch(hint).unwrap().body.as_bytes(), b"asset-1");
    let mut image = FetchRequest::subresource(&target, source, RequestDestination::Image).unwrap();
    image.mode = RequestMode::Cors;
    assert_eq!(client.fetch(image).unwrap().body.as_bytes(), b"asset-1");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn opaque_no_cors_prefetch_cannot_be_replayed_to_cors_consumer() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |request| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("asset-{count}"))
                .header("Cache-Control", "max-age=60")
                .header(
                    "Access-Control-Allow-Origin",
                    request.headers.get("origin").cloned().unwrap_or_default(),
                )
                .header("Access-Control-Allow-Credentials", "true")
        }
    });
    let client = client();
    let target = server.url("/asset");
    let source = "http://127.0.0.1:1/page";
    assert_eq!(
        client
            .fetch(FetchRequest::prefetch(&target, source).unwrap())
            .unwrap()
            .body
            .as_bytes(),
        b"asset-1"
    );
    let mut image = FetchRequest::subresource(&target, source, RequestDestination::Image).unwrap();
    image.mode = RequestMode::Cors;
    image.credentials = CredentialsMode::Include;
    assert_eq!(client.fetch(image).unwrap().body.as_bytes(), b"asset-2");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn cross_origin_credentials_mismatch_does_not_reuse_prefetch() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("asset-{count}")).header("Cache-Control", "max-age=60")
        }
    });
    let client = client();
    let target = server.url("/asset");
    let source = "http://127.0.0.1:1/page";
    client
        .fetch(FetchRequest::prefetch(&target, source).unwrap())
        .unwrap();
    // Unlike same-origin requests, these modes have different effective credentials.
    let image = FetchRequest::subresource(&target, source, RequestDestination::Image).unwrap();
    assert_eq!(client.fetch(image).unwrap().body.as_bytes(), b"asset-2");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn referrer_mismatch_does_not_reuse_prefetch() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("asset-{count}")).header("Cache-Control", "max-age=60")
        }
    });
    let client = client();
    let target = server.url("/asset");
    client
        .fetch(FetchRequest::prefetch(&target, &server.url("/page")).unwrap())
        .unwrap();
    let image =
        FetchRequest::subresource(&target, &server.url("/other"), RequestDestination::Image)
            .unwrap();
    assert_eq!(client.fetch(image).unwrap().body.as_bytes(), b"asset-2");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn vary_mismatch_does_not_reuse_prefetch() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("asset-{count}"))
                .header("Cache-Control", "max-age=60")
                .header("Vary", "Sec-Fetch-Dest")
        }
    });
    let client = client();
    let (source, target) = source_and_target(&server);
    client
        .fetch(FetchRequest::prefetch(&target, &source).unwrap())
        .unwrap();
    let image = FetchRequest::subresource(&target, &source, RequestDestination::Image).unwrap();
    assert_eq!(client.fetch(image).unwrap().body.as_bytes(), b"asset-2");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn prefetch_redirect_checks_csp_on_each_target_before_network_dispatch() {
    let calls = Arc::new(AtomicUsize::new(0));
    let cross_origin_target = Arc::new(Mutex::new(String::new()));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        let cross_origin_target = cross_origin_target.clone();
        move |request| {
            calls.fetch_add(1, Ordering::SeqCst);
            if request.path == "/cross" {
                TestResponse::new(302, Vec::new())
                    .header("Location", cross_origin_target.lock().unwrap().clone())
            } else if request.path == "/redirect" {
                TestResponse::new(302, Vec::new()).header("Location", "/final")
            } else {
                TestResponse::new(200, "final").header("Cache-Control", "max-age=60")
            }
        }
    });
    *cross_origin_target.lock().unwrap() = format!("http://localhost:{}/final", server.port());
    let client = client();
    let source = server.url("/page");
    let target = server.url("/redirect");
    let mut hint = FetchRequest::prefetch(&target, &source).unwrap();
    let mut headers = HeaderList::new();
    headers
        .append(
            "Content-Security-Policy",
            "default-src 'none'; img-src 'self'",
        )
        .unwrap();
    hint.policy = Arc::new(PolicyContainer::from_headers(&source, &headers).unwrap());
    assert_eq!(client.fetch(hint).unwrap().body.as_bytes(), b"final");
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let mut blocked = FetchRequest::prefetch(&server.url("/cross"), &source).unwrap();
    blocked.policy = Arc::new(PolicyContainer::from_headers(&source, &headers).unwrap());
    assert_eq!(
        client.fetch(blocked).unwrap_err().kind(),
        FetchErrorKind::Network
    );
    // The cross-origin redirect target never reached the network.
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn prefetch_referrer_policy_controls_outbound_referer() {
    let server = LoopbackServer::start(|request| {
        TestResponse::new(
            200,
            request.headers.get("referer").cloned().unwrap_or_default(),
        )
    });
    let client = client();
    let (source, target) = source_and_target(&server);
    let mut hint = FetchRequest::prefetch(&target, &source).unwrap();
    hint.referrer_policy = ReferrerPolicy::NoReferrer;
    assert!(client.fetch(hint).unwrap().body.as_bytes().is_empty());
}
