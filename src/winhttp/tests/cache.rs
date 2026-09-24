use super::support::{LoopbackServer, TestResponse};
use crate::fetch::{FetchErrorKind, FetchRequest, RequestCache, RequestMode};
use crate::winhttp::HttpClient;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn client() -> HttpClient {
    HttpClient::with_access_type(super::WINHTTP_ACCESS_TYPE_NO_PROXY).unwrap()
}

#[test]
fn fresh_get_is_reused_and_reload_replaces_it() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("body-{count}"))
                .header("Cache-Control", "private, max-age=60")
        }
    });
    let client = client();
    let url = server.url("/resource");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"body-1");
    let cached = client.get(&url).unwrap();
    assert_eq!(cached.body.as_bytes(), b"body-1");
    assert!(cached.headers.get("age").is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut reload = FetchRequest::navigation(&url).unwrap();
    reload.cache = RequestCache::Reload;
    assert_eq!(client.fetch(reload).unwrap().body.as_bytes(), b"body-2");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"body-2");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn vary_selects_correct_representation() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |request| {
            calls.fetch_add(1, Ordering::SeqCst);
            TestResponse::new(
                200,
                request.headers.get("x-theme").cloned().unwrap_or_default(),
            )
            .header("Cache-Control", "max-age=60")
            .header("Vary", "X-Theme")
        }
    });
    let client = client();
    let url = server.url("/vary");
    let themed = |theme| {
        let mut request = FetchRequest::navigation(&url).unwrap();
        request.headers.set("x-theme", theme).unwrap();
        client.fetch(request).unwrap().body.into_bytes()
    };
    assert_eq!(themed("light"), b"light");
    assert_eq!(themed("dark"), b"dark");
    assert_eq!(themed("light"), b"light");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn stale_validator_uses_304_body_and_freshens_entry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |request| {
            calls.fetch_add(1, Ordering::SeqCst);
            if request.headers.get("if-none-match").map(String::as_str) == Some("\"v1\"") {
                TestResponse::new(304, Vec::new()).header("Cache-Control", "max-age=60")
            } else {
                TestResponse::new(200, "original")
                    .header("ETag", "\"v1\"")
                    .header("Cache-Control", "max-age=0")
            }
        }
    });
    let client = client();
    let url = server.url("/validator");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"original");
    let validated = client.get(&url).unwrap();
    assert_eq!(validated.status, 200);
    assert_eq!(validated.body.as_bytes(), b"original");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"original");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn no_store_and_set_cookie_responses_are_never_reused() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |request| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            let response =
                TestResponse::new(200, format!("{count}")).header("Cache-Control", "max-age=60");
            if request.path == "/cookie" {
                response.header("Set-Cookie", "session=one; Path=/")
            } else {
                response
            }
        }
    });
    let client = client();
    let cookie = server.url("/cookie");
    assert_eq!(client.get(&cookie).unwrap().body.as_bytes(), b"1");
    assert_eq!(client.get(&cookie).unwrap().body.as_bytes(), b"2");
    let url = server.url("/plain");
    let mut request = FetchRequest::navigation(&url).unwrap();
    request.cache = RequestCache::NoStore;
    assert_eq!(client.fetch(request).unwrap().body.as_bytes(), b"3");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"4");
}

#[test]
fn incomplete_stream_is_not_admitted_and_only_if_cached_fails_closed() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            TestResponse::new(200, vec![b'x'; 32_768]).header("Cache-Control", "max-age=60")
        }
    });
    let client = client();
    let url = server.url("/large");
    let mut stream = client
        .fetch_stream(FetchRequest::navigation(&url).unwrap())
        .unwrap();
    assert!(stream.next_chunk().unwrap().is_some());
    drop(stream);
    assert_eq!(client.get(&url).unwrap().body.len(), 32_768);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let mut request = FetchRequest::subresource(
        &server.url("/miss"),
        &server.url("/document"),
        crate::fetch::RequestDestination::Fetch,
    )
    .unwrap();
    request.mode = RequestMode::SameOrigin;
    request.cache = RequestCache::OnlyIfCached;
    assert_eq!(
        client.fetch(request).unwrap_err().kind(),
        FetchErrorKind::Network
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn explicit_request_no_cache_validates_even_fresh_response() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |request| {
            calls.fetch_add(1, Ordering::SeqCst);
            if request.headers.get("if-none-match").map(String::as_str) == Some("\"v1\"") {
                TestResponse::new(304, Vec::new()).header("Cache-Control", "max-age=60")
            } else {
                TestResponse::new(200, "original")
                    .header("ETag", "\"v1\"")
                    .header("Cache-Control", "max-age=60")
            }
        }
    });
    let client = client();
    let url = server.url("/validate");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"original");
    let mut request = FetchRequest::navigation(&url).unwrap();
    request.headers.set("Cache-Control", "no-cache").unwrap();
    assert_eq!(client.fetch(request).unwrap().body.as_bytes(), b"original");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn max_age_zero_and_pragma_no_cache_bypass_a_fresh_entry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("body-{count}")).header("Cache-Control", "max-age=60")
        }
    });
    let client = client();
    let url = server.url("/fresh");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"body-1");
    let mut max_age_zero = FetchRequest::navigation(&url).unwrap();
    max_age_zero
        .headers
        .set("Cache-Control", "max-age=0")
        .unwrap();
    assert_eq!(
        client.fetch(max_age_zero).unwrap().body.as_bytes(),
        b"body-2"
    );
    let mut pragma = FetchRequest::navigation(&url).unwrap();
    pragma.headers.set("Pragma", "no-cache").unwrap();
    assert_eq!(client.fetch(pragma).unwrap().body.as_bytes(), b"body-3");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"body-3");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn response_no_cache_requires_validation_on_every_reuse() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |request| {
            calls.fetch_add(1, Ordering::SeqCst);
            if request.headers.get("if-none-match").map(String::as_str) == Some("\"stable\"") {
                TestResponse::new(304, Vec::new()).header("Cache-Control", "no-cache, max-age=60")
            } else {
                TestResponse::new(200, "content")
                    .header("ETag", "\"stable\"")
                    .header("Cache-Control", "no-cache, max-age=60")
            }
        }
    });
    let client = client();
    let url = server.url("/always-validate");
    for _ in 0..3 {
        assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"content");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn changing_vary_policy_discards_incompatible_old_variants() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |request| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            let response = TestResponse::new(200, format!("body-{count}"))
                .header("Cache-Control", "max-age=60");
            if count == 1 {
                response
            } else {
                let _ = request;
                response.header("Vary", "X-Theme")
            }
        }
    });
    let client = client();
    let url = server.url("/vary-change");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"body-1");
    let mut reload = FetchRequest::navigation(&url).unwrap();
    reload.cache = RequestCache::Reload;
    reload.headers.set("X-Theme", "dark").unwrap();
    assert_eq!(client.fetch(reload).unwrap().body.as_bytes(), b"body-2");
    // The old unvarying representation must not match this different theme.
    let mut light = FetchRequest::navigation(&url).unwrap();
    light.headers.set("X-Theme", "light").unwrap();
    assert_eq!(client.fetch(light).unwrap().body.as_bytes(), b"body-3");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn force_cache_can_reuse_stale_but_default_refetches() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("body-{count}"))
                .header("Cache-Control", "max-age=1")
                .header("Age", "2")
        }
    });
    let client = client();
    let url = server.url("/stale");
    let mut request = FetchRequest::navigation(&url).unwrap();
    assert_eq!(
        client.fetch(request.clone()).unwrap().body.as_bytes(),
        b"body-1"
    );
    request.cache = RequestCache::ForceCache;
    assert_eq!(client.fetch(request).unwrap().body.as_bytes(), b"body-1");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"body-2");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn only_if_cached_reuses_matching_same_origin_partition_without_network() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            TestResponse::new(200, "cached").header("Cache-Control", "max-age=60")
        }
    });
    let client = client();
    let url = server.url("/partitioned");
    let document = server.url("/document");
    let request = || {
        let mut request =
            FetchRequest::subresource(&url, &document, crate::fetch::RequestDestination::Fetch)
                .unwrap();
        request.mode = RequestMode::SameOrigin;
        request
    };
    assert_eq!(client.fetch(request()).unwrap().body.as_bytes(), b"cached");
    let mut cached = request();
    cached.cache = RequestCache::OnlyIfCached;
    assert_eq!(client.fetch(cached).unwrap().body.as_bytes(), b"cached");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // A different destination is a separate cache partition, even at the same URL.
    let mut other =
        FetchRequest::subresource(&url, &document, crate::fetch::RequestDestination::Image)
            .unwrap();
    other.mode = RequestMode::SameOrigin;
    other.cache = RequestCache::OnlyIfCached;
    assert_eq!(
        client.fetch(other).unwrap_err().kind(),
        FetchErrorKind::Network
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn request_no_store_does_not_replace_existing_fresh_representation() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            TestResponse::new(200, format!("body-{count}")).header("Cache-Control", "max-age=60")
        }
    });
    let client = client();
    let url = server.url("/retained");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"body-1");
    let mut bypass = FetchRequest::navigation(&url).unwrap();
    bypass.cache = RequestCache::NoStore;
    assert_eq!(client.fetch(bypass).unwrap().body.as_bytes(), b"body-2");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"body-1");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn authorization_request_neither_reads_nor_writes_an_anonymous_cache_entry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |request| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            let identity = if request.headers.contains_key("authorization") {
                "signed"
            } else {
                "guest"
            };
            TestResponse::new(200, format!("{identity}-{count}"))
                .header("Cache-Control", "private, max-age=60")
        }
    });
    let client = client();
    let url = server.url("/identity");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"guest-1");
    let mut signed = FetchRequest::navigation(&url).unwrap();
    signed
        .headers
        .set("Authorization", "Bearer example")
        .unwrap();
    assert_eq!(client.fetch(signed).unwrap().body.as_bytes(), b"signed-2");
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"guest-1");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn responses_above_the_per_entry_budget_are_not_cached() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = LoopbackServer::start({
        let calls = calls.clone();
        move |_| {
            let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
            let mut body = vec![b'x'; 2 * 1024 * 1024 + 1];
            body[0] = b'0' + count as u8;
            TestResponse::new(200, body).header("Cache-Control", "max-age=60")
        }
    });
    let client = client();
    let url = server.url("/too-large");
    assert_eq!(client.get(&url).unwrap().body.as_bytes()[0], b'1');
    assert_eq!(client.get(&url).unwrap().body.as_bytes()[0], b'2');
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
