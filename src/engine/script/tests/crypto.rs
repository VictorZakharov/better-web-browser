use super::*;
use std::sync::Arc;

const FIXTURE: &str = include_str!("../../../../tests/fixtures/crypto-randomness.js");

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
