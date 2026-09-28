use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);

fn https_origin() -> Origin {
    Origin::parse("https://example.test/").unwrap()
}

fn handler(scheme: &str, template: &str) -> Result<Handler, HandlerError> {
    normalize(
        scheme,
        template,
        "https://example.test/page",
        &https_origin(),
    )
}

#[test]
fn custom_schemes_and_safelisted_schemes_are_normalized() {
    let normalized = handler("WEB+Soup", "/handler?uri=%s").unwrap();
    assert_eq!(normalized.scheme, "web+soup");
    assert_eq!(normalized.template, "https://example.test/handler?uri=%s");
    assert_eq!(normalized.origin, "https://example.test");
    assert_eq!(handler("MAILTO", "/mail?uri=%s").unwrap().scheme, "mailto");
    for invalid in [
        "http",
        "https",
        "file",
        "web+",
        "web+123",
        "web+foo-bar",
        "mailto:",
        "web+abcdefghijklmnopqrstuvwxyzabcdef",
    ] {
        assert_eq!(handler(invalid, "/h?x=%s"), Err(HandlerError::Security));
    }
}

#[test]
fn template_must_contain_placeholder_on_same_origin() {
    assert_eq!(handler("web+soup", "/h"), Err(HandlerError::Syntax));
    assert_eq!(
        handler("web+soup", "https://other.test/?x=%s"),
        Err(HandlerError::Security)
    );
    assert_eq!(
        handler("web+soup", "ftp://example.test/?x=%s"),
        Err(HandlerError::Security)
    );
    assert_eq!(
        handler("web+soup", "https://%s.example.test/"),
        Err(HandlerError::Syntax)
    );
    let insecure = Origin::parse("http://example.test/").unwrap();
    assert_eq!(
        normalize("web+soup", "/h?x=%s", "http://example.test/", &insecure),
        Err(HandlerError::Security)
    );
    let loopback = Origin::parse("http://127.0.0.1:8123/").unwrap();
    assert!(normalize("web+soup", "/h?x=%s", "http://127.0.0.1:8123/", &loopback).is_ok());
}

#[test]
fn only_approved_handler_routes_and_escapes_entire_input_url() {
    let mut store = Registry::default();
    let soup = handler("web+soup", "/handler?uri=%s").unwrap();
    assert_eq!(store.navigate("web+soup:chicken-kiwi"), None);
    assert!(store.approve(soup.clone()));
    assert_eq!(
        store.navigate("web+soup:chicken-k%C3%AFwi"),
        Some("https://example.test/handler?uri=web%2Bsoup%3Achicken-k%25C3%25AFwi".into())
    );
    assert_eq!(store.navigate("web+other:secret"), None);
    assert_eq!(
        store.navigate("web+soup://alice:secret@example.test/box"),
        Some("https://example.test/handler?uri=web%2Bsoup%3A%2F%2Fexample.test%2Fbox".into())
    );
    assert!(store.unregister(&soup));
    assert_eq!(store.navigate("web+soup:chicken-kiwi"), None);
}

#[test]
fn consent_and_persistence_are_bounded_and_recoverable() {
    let profile = std::env::temp_dir().join(format!(
        "breeze-protocol-handlers-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut store = Registry::default();
    let soup = handler("web+soup", "/handler?uri=%s").unwrap();
    store.deny(soup.clone());
    assert!(store.was_denied(&soup));
    assert!(store.approve(soup.clone()));
    assert!(!store.was_denied(&soup));
    store.save(&profile).unwrap();
    let reopened = Registry::open(&profile).unwrap();
    assert!(reopened.is_approved(&soup));
    assert_eq!(
        reopened.navigate("web+soup:pie"),
        store.navigate("web+soup:pie")
    );
    let mut reopened = reopened;
    assert!(reopened.unregister(&soup));
    reopened.save(&profile).unwrap();
    assert!(!Registry::open(&profile).unwrap().is_approved(&soup));
    std::fs::remove_dir_all(profile).unwrap();
}

#[test]
fn malformed_or_ambiguous_persisted_grants_fail_closed() {
    let profile = std::env::temp_dir().join(format!(
        "breeze-protocol-handlers-invalid-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&profile).unwrap();
    let soup = handler("web+soup", "/handler?uri=%s").unwrap();
    let conflicting = handler("web+soup", "/other?uri=%s").unwrap();
    let store = serde_json::json!({ "approved": [soup, conflicting], "denied": [] });
    std::fs::write(store_path(&profile), store.to_string()).unwrap();
    assert!(Registry::open(&profile).is_err());
    std::fs::remove_dir_all(profile).unwrap();
}
