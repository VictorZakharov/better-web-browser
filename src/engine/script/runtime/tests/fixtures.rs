//! Shared document-runtime test fixtures and script-input construction.
use super::{ScriptInput, ScriptRuntime, StorageAreaSnapshot, StorageEntry, dom};

pub(super) fn retained_fixture(label: &str) -> (dom::Dom, ScriptRuntime) {
    let html = format!(
        r#"<body><div>waiting</div><script>
            window.realmLabel = '{label}';
            setTimeout(() => {{
                document.querySelector('div').textContent = window.realmLabel;
            }}, 2000);
        </script></body>"#
    );
    let dom = dom::parse_with_scripting(&html, true);
    let scripts = script_inputs(&dom);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&scripts);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}

pub(super) fn snapshot(version: u64, key: &str, value: &str) -> StorageAreaSnapshot {
    StorageAreaSnapshot {
        version,
        entries: vec![StorageEntry {
            key: key.into(),
            value: value.into(),
        }],
    }
}

pub(super) fn script_inputs(dom: &dom::Dom) -> Vec<ScriptInput> {
    dom.elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: crate::engine::script::ScriptKind::Classic,
            fetch_options: crate::engine::script::ScriptFetchOptions::for_kind(
                crate::engine::script::ScriptKind::Classic,
            ),
            finish_lifecycle: true,
        })
        .collect()
}

pub(super) fn input(
    node: &crate::engine::dom::NodeRef,
    source: &str,
    code: &str,
    finish_lifecycle: bool,
) -> ScriptInput {
    ScriptInput {
        node: node.clone(),
        source_url: format!("https://example.com/{source}"),
        code: code.into(),
        kind: crate::engine::script::ScriptKind::Classic,
        fetch_options: crate::engine::script::ScriptFetchOptions::for_kind(
            crate::engine::script::ScriptKind::Classic,
        ),
        finish_lifecycle,
    }
}
