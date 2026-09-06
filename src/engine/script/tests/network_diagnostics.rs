use super::network::{pending_runtime, test_response};

#[test]
fn handled_http_failure_is_diagnosed_without_response_secrets() {
    let (dom, mut runtime, id) = pending_runtime(
        "fetch('/private?token=secret').then(r => document.querySelector('div').textContent = r.status);",
    );
    let mut response = test_response(b"private response body");
    response.status = 403;
    let outcome = runtime.complete_fetch_with_loader(id, Ok(response), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "403"
    );
    assert!(
        outcome
            .console
            .iter()
            .any(|line| line == "warn: Fetch 1 failed: HTTP 403 origin=https://example.com")
    );
    assert!(
        outcome
            .console
            .iter()
            .all(|line| !line.contains("secret") && !line.contains("private"))
    );
}
