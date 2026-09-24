use super::*;
use std::sync::Arc;

const FIXTURE: &str = include_str!("../../../../tests/fixtures/crypto-randomness.js");
const SUBTLE_FIXTURE: &str = include_str!("../../../../tests/fixtures/web-crypto.js");
const EC_FIXTURE: &str = include_str!("../../../../tests/fixtures/web-crypto-ec.js");
const RSA_FIXTURE: &str = include_str!("../../../../tests/fixtures/web-crypto-rsa.js");

#[cfg(target_os = "windows")]
#[test]
fn rsa_crypto_roundtrips_in_window_and_worker() {
    let (_, outcome) = execute_html(&format!(
        "<script>{RSA_FIXTURE}\nrunRsaCryptoFixture()</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: RSA crypto passed"]);
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        &format!("{RSA_FIXTURE}\nrunRsaCryptoFixture()"),
        "rsa",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: RSA crypto passed"]);
}

#[cfg(target_os = "windows")]
#[test]
fn elliptic_curve_crypto_vectors_in_window_and_worker() {
    let (_, outcome) = execute_html(&format!(
        "<script>{EC_FIXTURE}\nrunEllipticCurveFixture()</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: elliptic curves passed"]);
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        &format!("{EC_FIXTURE}\nrunEllipticCurveFixture()"),
        "ec",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: elliptic curves passed"]);
}

#[cfg(target_os = "windows")]
#[test]
fn subtle_crypto_vectors_and_key_policy_in_window() {
    let (_, outcome) = execute_html(&format!(
        "<script>{SUBTLE_FIXTURE}\nrunWebCryptoFixture()</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: web crypto passed"]);
}

#[cfg(target_os = "windows")]
#[test]
fn subtle_crypto_vectors_in_worker() {
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        &format!("{SUBTLE_FIXTURE}\nrunWebCryptoFixture()"),
        "crypto",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: web crypto passed"]);
}

#[test]
fn crypto_randomness_contract_in_window() {
    let (_, outcome) = execute_html(&format!(
        "<script>{FIXTURE}\nconst result = checkCryptoRandomness();\
         if (result.failures.length) throw new Error(JSON.stringify(result));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn crypto_randomness_contract_in_worker() {
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        &format!(
            "{FIXTURE}\nconst result = checkCryptoRandomness();\
            if (result.failures.length) throw new Error(JSON.stringify(result));"
        ),
        "crypto",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn crypto_uuid_is_secure_only_but_random_values_remain_available() {
    for (url, uuid) in [
        ("http://example.com/", "undefined"),
        ("https://example.com/", "function"),
        ("http://127.0.0.1/", "function"),
    ] {
        let code = format!(
            "if (typeof crypto.randomUUID !== '{uuid}') throw new Error('UUID exposure');\
            crypto.getRandomValues(new Uint8Array(8));"
        );
        let dom = dom::parse_with_scripting("<script></script>", true);
        let script = ScriptInput {
            source_url: url.into(),
            code: code.clone(),
            node: dom.elements_named("script").next().unwrap(),
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        };
        let outcome = execute(dom.document, url, &[script]);
        assert!(outcome.errors.is_empty(), "{url}: {:?}", outcome.errors);
        let (_, outcome) = WorkerRuntime::start(
            url,
            &code,
            "",
            ScriptKind::Classic,
            Arc::new(|_, _| Err("unexpected import".into())),
        );
        assert!(outcome.errors.is_empty(), "{url}: {:?}", outcome.errors);
    }
}
