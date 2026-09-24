use super::*;

fn headers(fields: &[(&str, &str)]) -> HeaderList {
    let mut headers = HeaderList::new();
    for (name, value) in fields {
        headers.append(name, value).unwrap();
    }
    headers
}

#[test]
fn policy_uses_explicit_freshness_and_age() {
    let response = headers(&[
        ("date", "Sun, 06 Nov 1994 08:49:37 GMT"),
        ("cache-control", "max-age=60"),
        ("age", "10"),
    ]);
    let at = parse_http_date("Sun, 06 Nov 1994 08:49:42 GMT").unwrap();
    let policy = CachePolicy::for_response(&response, at).unwrap();
    assert_eq!(policy.freshness_lifetime, Duration::from_secs(60));
    assert_eq!(policy.initial_age, Duration::from_secs(10));
}

#[test]
fn vary_star_and_no_store_are_not_admitted() {
    assert!(vary_fields(&headers(&[("vary", "*")])).is_none());
    assert!(
        CachePolicy::for_response(
            &headers(&[("cache-control", "no-store, max-age=60")]),
            SystemTime::now()
        )
        .is_none()
    );
}

#[test]
fn expires_uses_date_as_freshness_origin() {
    let response = headers(&[
        ("date", "Sun, 06 Nov 1994 08:49:37 GMT"),
        ("expires", "Sun, 06 Nov 1994 08:50:37 GMT"),
        ("age", "10"),
    ]);
    let at = parse_http_date("Sun, 06 Nov 1994 08:49:42 GMT").unwrap();
    let policy = CachePolicy::for_response(&response, at).unwrap();
    assert_eq!(policy.freshness_lifetime, Duration::from_secs(60));
    assert_eq!(policy.initial_age, Duration::from_secs(10));
}

#[test]
fn no_cache_retains_validator_but_cannot_serve_fresh() {
    let request = FetchRequest::navigation("https://example.test/").unwrap();
    let headers = headers(&[
        ("cache-control", "no-cache, max-age=60"),
        ("etag", "\"v1\""),
    ]);
    let policy = CachePolicy::for_response(&headers, SystemTime::now()).unwrap();
    assert!(policy.no_cache);
    let cached = CachedResponse {
        partition: Partition::new(&request, &HeaderList::new()),
        vary: Vec::new(),
        request_headers: HeaderList::new(),
        status: 200,
        headers,
        body: b"body".to_vec(),
        stored_at: Instant::now(),
        initial_age: Duration::ZERO,
        freshness_lifetime: policy.freshness_lifetime,
        no_cache: policy.no_cache,
        must_revalidate: policy.must_revalidate,
    };
    assert!(!cached.fresh());
    assert!(!cached.can_serve_stale());
    assert!(cached.has_validator());
}
