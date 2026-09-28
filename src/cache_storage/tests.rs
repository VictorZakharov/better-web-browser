use super::model::*;
use base64::Engine as _;
use serde_json::json;

mod persistence;

const ALPHA: &str = "https://alpha.example";
const BETA: &str = "https://beta.example";

#[test]
fn only_trustworthy_non_opaque_origins_can_access_cache_storage() {
    let store = CacheStorage::in_memory();
    for origin in ["http://example.test", "null", "data:text/plain,opaque"] {
        assert!(matches!(
            store.execute(origin, CacheCommand::Names),
            Err(CacheError::InvalidRequest)
        ));
    }
    assert_eq!(
        store
            .execute("http://127.0.0.1", CacheCommand::Names)
            .unwrap(),
        json!([])
    );
}

fn request(url: &str) -> CacheRequest {
    CacheRequest {
        url: url.into(),
        method: "GET".into(),
        headers: Vec::new(),
    }
}

fn response(body: &str) -> CacheResponse {
    CacheResponse {
        status: 200,
        status_text: "OK".into(),
        response_type: "default".into(),
        url: String::new(),
        redirected: false,
        headers: Vec::new(),
        body_base64: Some(base64::engine::general_purpose::STANDARD.encode(body)),
    }
}

fn put(name: &str, url: &str, body: &str) -> CacheCommand {
    CacheCommand::Put {
        name: name.into(),
        entries: vec![CacheEntry {
            request: request(url),
            response: response(body),
        }],
    }
}

fn match_entry(name: Option<&str>, url: &str, options: CacheQueryOptions) -> CacheCommand {
    CacheCommand::Match {
        name: name.map(str::to_string),
        request: request(url),
        options,
    }
}

#[test]
fn names_and_matches_are_origin_scoped_and_ordered() {
    let store = CacheStorage::in_memory();
    for name in ["first", "second"] {
        store
            .execute(ALPHA, CacheCommand::Open { name: name.into() })
            .unwrap();
    }
    store
        .execute(
            BETA,
            CacheCommand::Open {
                name: "private".into(),
            },
        )
        .unwrap();
    assert_eq!(
        store.execute(ALPHA, CacheCommand::Names).unwrap(),
        json!(["first", "second"])
    );
    assert_eq!(
        store.execute(BETA, CacheCommand::Names).unwrap(),
        json!(["private"])
    );
    store
        .execute(ALPHA, put("second", "https://alpha.example/item", "second"))
        .unwrap();
    store
        .execute(ALPHA, put("first", "https://alpha.example/item", "first"))
        .unwrap();
    let matched = store
        .execute(
            ALPHA,
            match_entry(None, "https://alpha.example/item", Default::default()),
        )
        .unwrap();
    assert_eq!(matched["bodyBase64"], json!("Zmlyc3Q="));
    assert_eq!(
        store
            .execute(
                BETA,
                match_entry(None, "https://alpha.example/item", Default::default())
            )
            .unwrap(),
        json!(null)
    );
}

#[test]
fn matching_uses_fragment_search_method_and_vary_rules() {
    let store = CacheStorage::in_memory();
    store
        .execute(ALPHA, CacheCommand::Open { name: "v1".into() })
        .unwrap();
    let mut entry = CacheEntry {
        request: request("https://alpha.example/data?kind=a#old"),
        response: response("one"),
    };
    entry
        .request
        .headers
        .push(("Accept-Language".into(), "en".into()));
    entry
        .response
        .headers
        .push(("Vary".into(), "Accept-Language".into()));
    store
        .execute(
            ALPHA,
            CacheCommand::Put {
                name: "v1".into(),
                entries: vec![entry],
            },
        )
        .unwrap();
    let mut query = request("https://alpha.example/data?kind=a#new");
    query.headers.push(("accept-language".into(), "fr".into()));
    assert_eq!(
        store
            .execute(
                ALPHA,
                CacheCommand::Match {
                    name: Some("v1".into()),
                    request: query.clone(),
                    options: Default::default()
                }
            )
            .unwrap(),
        json!(null)
    );
    let options = CacheQueryOptions {
        ignore_vary: true,
        ..Default::default()
    };
    assert_ne!(
        store
            .execute(
                ALPHA,
                CacheCommand::Match {
                    name: Some("v1".into()),
                    request: query.clone(),
                    options
                }
            )
            .unwrap(),
        json!(null)
    );
    query.url = "https://alpha.example/data?kind=b".into();
    assert_eq!(
        store
            .execute(
                ALPHA,
                CacheCommand::Match {
                    name: Some("v1".into()),
                    request: query.clone(),
                    options
                }
            )
            .unwrap(),
        json!(null)
    );
    let options = CacheQueryOptions {
        ignore_search: true,
        ignore_vary: true,
        ..Default::default()
    };
    assert_ne!(
        store
            .execute(
                ALPHA,
                CacheCommand::Match {
                    name: Some("v1".into()),
                    request: query.clone(),
                    options
                }
            )
            .unwrap(),
        json!(null)
    );
    query.method = "POST".into();
    assert_eq!(
        store
            .execute(
                ALPHA,
                CacheCommand::Match {
                    name: Some("v1".into()),
                    request: query.clone(),
                    options
                }
            )
            .unwrap(),
        json!(null)
    );
    let options = CacheQueryOptions {
        ignore_method: true,
        ignore_search: true,
        ignore_vary: true,
    };
    assert_ne!(
        store
            .execute(
                ALPHA,
                CacheCommand::Match {
                    name: Some("v1".into()),
                    request: query,
                    options
                }
            )
            .unwrap(),
        json!(null)
    );
}

#[test]
fn invalid_or_duplicate_batch_cannot_modify_existing_entries() {
    let store = CacheStorage::in_memory();
    store
        .execute(ALPHA, CacheCommand::Open { name: "v1".into() })
        .unwrap();
    store
        .execute(ALPHA, put("v1", "https://alpha.example/item", "old"))
        .unwrap();
    let good = CacheEntry {
        request: request("https://alpha.example/item"),
        response: response("new"),
    };
    let mut bad = good.clone();
    bad.request.method = "POST".into();
    assert!(matches!(
        store.execute(
            ALPHA,
            CacheCommand::Put {
                name: "v1".into(),
                entries: vec![good.clone(), bad]
            }
        ),
        Err(CacheError::InvalidRequest)
    ));
    assert!(matches!(
        store.execute(
            ALPHA,
            CacheCommand::Put {
                name: "v1".into(),
                entries: vec![good.clone(), good]
            }
        ),
        Err(CacheError::Duplicate)
    ));
    let matched = store
        .execute(
            ALPHA,
            match_entry(Some("v1"), "https://alpha.example/item", Default::default()),
        )
        .unwrap();
    assert_eq!(matched["bodyBase64"], json!("b2xk"));
}

#[test]
fn rejects_partial_content_and_vary_star() {
    let store = CacheStorage::in_memory();
    store
        .execute(ALPHA, CacheCommand::Open { name: "v1".into() })
        .unwrap();
    let mut partial = CacheEntry {
        request: request("https://alpha.example/item"),
        response: response("x"),
    };
    partial.response.status = 206;
    assert!(matches!(
        store.execute(
            ALPHA,
            CacheCommand::Put {
                name: "v1".into(),
                entries: vec![partial.clone()]
            }
        ),
        Err(CacheError::InvalidResponse)
    ));
    partial.response.status = 200;
    partial.response.headers.push(("Vary".into(), "*".into()));
    assert!(matches!(
        store.execute(
            ALPHA,
            CacheCommand::Put {
                name: "v1".into(),
                entries: vec![partial]
            }
        ),
        Err(CacheError::VaryStar)
    ));
}
