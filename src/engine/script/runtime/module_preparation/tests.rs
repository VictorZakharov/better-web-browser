use super::*;

#[test]
fn preparing_graphs_never_evaluates_and_accounts_each_source_once() {
    let page = crate::engine::Page::parse_scripted(
        "<script type=module src=/root.js></script>",
        "https://example.test/",
    );
    let mut runtime = ScriptRuntime::new(page.dom.document.clone(), &page.source_url);
    assert!(runtime.execute_initial(&[]).errors.is_empty());
    let url = "https://example.test/root.js";
    let source = "import './dep.js'; document.title='executed';";
    assert_eq!(
        runtime.prepare_module_graph(url, source).unwrap(),
        ["https://example.test/dep.js"]
    );
    runtime
        .install_module_dependency("https://example.test/dep.js", "export const x=1;")
        .unwrap();
    let charged = runtime.total_script_bytes;
    assert!(
        runtime
            .prepare_module_graph(url, source)
            .unwrap()
            .is_empty()
    );
    assert_eq!(runtime.total_script_bytes, charged);
    assert_ne!(page.dom.title(), "executed");
    let input = ScriptInput {
        node: page.scripts[0].node.clone(),
        source_url: url.into(),
        code: source.into(),
        kind: ScriptKind::Module,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Module),
        finish_lifecycle: false,
    };
    assert!(
        runtime
            .execute_additional_with_loader(&[input], None)
            .errors
            .is_empty()
    );
    assert_eq!(runtime.total_script_bytes, charged);
    assert_eq!(page.dom.title(), "executed");
}

#[test]
fn compilation_cannot_bypass_the_total_script_budget() {
    let document = crate::engine::dom::parse_with_scripting("<p>unchanged</p>", true);
    let mut runtime = ScriptRuntime::new(document.document, "https://example.test/");
    runtime.total_script_bytes = MAX_PAGE_SCRIPT_BYTES - 4;
    assert!(
        runtime
            .prepare_module_graph("https://example.test/large.js", "void 0;")
            .is_err()
    );
    assert!(
        !runtime
            .host
            .borrow()
            .module_loader
            .contains("https://example.test/large.js")
    );
}

#[test]
fn fetched_module_parse_link_and_evaluation_errors_are_not_resource_errors() {
    for source in [
        "export const = 1;",
        "import {missing} from './dep.js';",
        "throw new Error('evaluation failure');",
    ] {
        let page = crate::engine::Page::parse_scripted(
            "<script id=m type=module src=/root.js></script>",
            "https://example.test/",
        );
        let mut runtime = ScriptRuntime::new(page.dom.document.clone(), &page.source_url);
        let node = page.scripts[0].node.clone();
        let mut input = ScriptInput {node, source_url: "https://example.test/setup.js".into(),
            code: "document.getElementById('m').onload=()=>document.title='loaded';document.getElementById('m').onerror=()=>document.title='incorrect';".into(),
            kind: ScriptKind::Classic, fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic), finish_lifecycle: true};
        assert!(runtime.execute_initial(&[input.clone()]).errors.is_empty());
        input.kind = ScriptKind::Module;
        input.source_url = "https://example.test/root.js".into();
        input.code = source.into();
        input.fetch_options = ScriptFetchOptions::for_kind(ScriptKind::Module);
        runtime
            .install_module_dependency("https://example.test/dep.js", "export const present=1;")
            .unwrap();
        let outcome = runtime.execute_additional_with_loader(&[input], None);
        assert_eq!(outcome.errors.len(), 1, "{:?}", outcome.errors);
        assert_eq!(page.dom.title(), "loaded", "{source}");
    }
}
