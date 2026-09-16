use super::*;
use crate::engine::script::{ScriptFetchOptions, ScriptKind};
use crate::limits::MAX_PAGE_SCRIPT_BYTES;

#[test]
fn inserted_source_bytes_share_the_runtime_page_budget() {
    let dom = dom::parse_with_scripting("<!doctype html><body><script></script>", true);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.total_script_bytes.set(MAX_PAGE_SCRIPT_BYTES - 1000);
    let node = dom.elements_named("script").next().unwrap();
    let code = "const s=document.createElement('script');s.text=' '.repeat(2000);try{document.body.append(s)}catch(e){document.body.dataset.result=e.name}";
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: code.into(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("RangeError")
    );
    assert_eq!(
        runtime.total_script_bytes.get(),
        MAX_PAGE_SCRIPT_BYTES - 1000 + code.len()
    );
}
