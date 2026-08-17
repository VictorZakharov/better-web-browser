use super::*;
use crate::fetch::{CredentialsMode, FetchRequest, FetchUrl};
use crate::winhttp::ffi::WINHTTP_ACCESS_TYPE_NO_PROXY;
use std::time::{Duration, UNIX_EPOCH};

fn client() -> HttpClient {
    HttpClient::with_access_type(WINHTTP_ACCESS_TYPE_NO_PROXY).unwrap()
}

#[test]
fn consumer_parser_accepts_legacy_pairs_and_rejects_only_unsafe_input() {
    let origin = ParsedUrl::parse("https://www.example.com/docs/page").unwrap();
    let (nameless, _) = parse_cookie(&origin, "  legacy value  ; Path=/").unwrap();
    assert_eq!(nameless.name, "");
    assert_eq!(nameless.value, "legacy value");

    let (permissive, _) = parse_cookie(&origin, " odd name = a,b c ").unwrap();
    assert_eq!(permissive.name, "odd name");
    assert_eq!(permissive.value, "a,b c");
    assert!(parse_cookie(&origin, "safe=value\r\nInjected: yes").is_none());
    assert!(parse_cookie(&origin, "= ").is_none());
}

#[test]
fn applies_public_suffix_and_prefix_storage_rules() {
    let origin = ParsedUrl::parse("https://www.example.co.uk/docs/page").unwrap();
    assert!(parse_cookie(&origin, "bad=value; Domain=co.uk").is_none());
    assert!(parse_cookie(&origin, "bad=value; Domain=\u{00e9}xample.co.uk").is_none());

    let (domain, _) = parse_cookie(&origin, "good=value; Domain=.example.co.uk").unwrap();
    assert_eq!(domain.domain, "example.co.uk");
    assert!(!domain.host_only);

    assert!(parse_cookie(&origin, "__sEcUrE-token=value").is_none());
    assert!(parse_cookie(&origin, "__sEcUrE-token=value; Secure").is_some());
    assert!(parse_cookie(&origin, "__hOsT-token=value; Secure").is_none());
    assert!(
        parse_cookie(
            &origin,
            "__hOsT-token=value; Secure; Path=/; Domain=example.co.uk"
        )
        .is_none()
    );
    assert!(parse_cookie(&origin, "__hOsT-token=value; Secure; Path=/").is_some());
    assert!(parse_cookie(&origin, "__Secure-token; Secure").is_none());
}

#[test]
fn max_age_wins_and_persistent_expiry_is_capped_at_four_hundred_days() {
    let origin = ParsedUrl::parse("https://example.com/").unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let (max_age, expired) = parse::parse_cookie_internal(
        &origin,
        "token=value; Expires=Sun, 06 Nov 1994 08:49:37 GMT; Max-Age=10",
        true,
        now,
    )
    .unwrap();
    assert!(!expired);
    assert_eq!(max_age.expires_at, Some(now + Duration::from_secs(10)));

    let (capped, _) = parse::parse_cookie_internal(
        &origin,
        "token=value; Expires=Fri, 31 Dec 9999 23:59:59 GMT",
        true,
        now,
    )
    .unwrap();
    assert_eq!(
        capped.expires_at,
        Some(now + Duration::from_secs(400 * 24 * 60 * 60))
    );

    let (_, expired) = parse::parse_cookie_internal(
        &origin,
        "token=value; Max-Age=-999999999999999999999999",
        true,
        now,
    )
    .unwrap();
    assert!(expired);
}

#[test]
fn insecure_sources_cannot_overlay_secure_cookies() {
    let client = client();
    client
        .set_cookie(
            "https://example.com/login",
            "session=secure; Secure; Path=/login",
        )
        .unwrap();
    client
        .set_cookie(
            "http://example.com/login/en",
            "session=insecure; Path=/login/en",
        )
        .unwrap();
    assert_eq!(
        client
            .document_cookie_header("https://example.com/login/en")
            .unwrap(),
        "session=secure"
    );

    client
        .set_cookie("http://example.com/", "session=allowed; Path=/")
        .unwrap();
    assert_eq!(
        client
            .document_cookie_header("https://example.com/login/en")
            .unwrap(),
        "session=secure; session=allowed"
    );
}

#[test]
fn cookie_identity_includes_the_host_only_flag() {
    let client = client();
    client
        .set_cookie("https://example.com/", "id=host; Path=/")
        .unwrap();
    client
        .set_cookie(
            "https://example.com/",
            "id=domain; Domain=example.com; Path=/",
        )
        .unwrap();
    assert_eq!(
        client
            .document_cookie_header("https://example.com/")
            .unwrap(),
        "id=host; id=domain"
    );
}

#[test]
fn same_site_filters_cross_site_subresources_and_navigation_methods() {
    let client = client();
    let cookie_url = "https://www.example.com/";
    for assignment in [
        "strict=yes; Domain=example.com; Secure; SameSite=Strict",
        "lax=yes; Domain=example.com; Secure; SameSite=Lax",
        "default=yes; Domain=example.com; Secure; SameSite=invalid",
        "none=yes; Domain=example.com; Secure; SameSite=None",
    ] {
        client.set_cookie(cookie_url, assignment).unwrap();
    }

    let mut subresource =
        FetchRequest::script("https://api.example.com/data", "https://elsewhere.test/").unwrap();
    subresource.credentials = CredentialsMode::Include;
    assert_eq!(
        client.cookie_header_value(&subresource).unwrap().as_deref(),
        Some("none=yes")
    );

    let mut navigation = FetchRequest::navigation("https://api.example.com/data").unwrap();
    navigation.origin = Some(FetchUrl::parse("https://elsewhere.test/").unwrap().origin());
    assert_eq!(
        client.cookie_header_value(&navigation).unwrap().as_deref(),
        Some("lax=yes; default=yes; none=yes")
    );
    navigation.set_method("POST").unwrap();
    assert_eq!(
        client.cookie_header_value(&navigation).unwrap().as_deref(),
        Some("none=yes")
    );

    let same_site =
        FetchRequest::script("https://api.example.com/data", "https://app.example.com/").unwrap();
    assert_eq!(
        client.cookie_header_value(&same_site).unwrap().as_deref(),
        Some("strict=yes; lax=yes; default=yes; none=yes")
    );
}

#[test]
fn nameless_cookie_serialization_omits_the_equals_sign() {
    let client = client();
    client
        .set_cookie("https://example.com/", "legacy-value; Path=/")
        .unwrap();
    assert_eq!(
        client
            .document_cookie_header("https://example.com/")
            .unwrap(),
        "legacy-value"
    );
}
