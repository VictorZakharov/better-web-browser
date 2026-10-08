use super::*;

#[test]
fn offscreen_webgl_numeric_unions_use_the_same_idl_and_validation_contract() {
    let source = format!(
        "{}\ntestNumericUnions(()=>new OffscreenCanvas(8,4));postMessage('passed');",
        include_str!("../../../../tests/webgl/numeric-unions.js")
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/numeric-unions.js",
        &source,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
}
