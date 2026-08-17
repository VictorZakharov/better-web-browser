use super::*;
use std::time::Duration;

#[test]
fn module_graph_resolves_relative_imports_and_import_meta_url() {
    let dom = crate::engine::dom::parse_with_scripting(
        "<body><div>pending</div><script type=module></script></body>",
        true,
    );
    let input = ScriptInput {
        node: dom.elements_named("script").next().unwrap(),
        source_url: "https://example.com/scripts/main.js".into(),
        code: r#"
            import { answer } from './dependency.js';
            document.querySelector('div').textContent =
                answer + '|' + import.meta.url + '|' + (document.currentScript === null);
        "#
        .into(),
        kind: ScriptKind::Module,
        finish_lifecycle: true,
    };
    let mut requests = Vec::new();
    let mut loader = |url: &str, kind: ScriptKind| {
        requests.push((url.to_string(), kind));
        match url {
            "https://example.com/scripts/dependency.js" => {
                Ok("export { answer } from './answer.js';".into())
            }
            "https://example.com/scripts/answer.js" => Ok("export const answer = 42;".into()),
            _ => Err(format!("unexpected module URL: {url}")),
        }
    };

    let outcome = execute_with_loader(
        dom.document.clone(),
        "https://example.com/",
        &[input],
        &mut loader,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    assert_eq!(
        requests,
        [
            (
                "https://example.com/scripts/dependency.js".into(),
                ScriptKind::Module
            ),
            (
                "https://example.com/scripts/answer.js".into(),
                ScriptKind::Module
            )
        ]
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "42|https://example.com/scripts/main.js|true"
    );
}

#[test]
fn bare_module_specifiers_fail_without_running_the_module_body() {
    let dom = crate::engine::dom::parse_with_scripting(
        "<body><div>unchanged</div><script type=module></script></body>",
        true,
    );
    let input = ScriptInput {
        node: dom.elements_named("script").next().unwrap(),
        source_url: "https://example.com/main.js".into(),
        code: "import value from 'unmapped-package'; document.querySelector('div').textContent = value;".into(),
        kind: ScriptKind::Module,
        finish_lifecycle: true,
    };
    let outcome = execute(dom.document.clone(), "https://example.com/", &[input]);

    assert_eq!(outcome.executed, 0);
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("bare module specifier")),
        "{:?}",
        outcome.errors
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "unchanged"
    );
}

#[test]
fn top_level_await_delays_document_lifecycle_until_the_module_settles() {
    let dom = crate::engine::dom::parse_with_scripting(
        "<body><div>pending</div><script type=module></script></body>",
        true,
    );
    let input = ScriptInput {
        node: dom.elements_named("script").next().unwrap(),
        source_url: "https://example.com/pending.js".into(),
        code: r#"
            const output = document.querySelector('div');
            document.addEventListener('DOMContentLoaded', () => output.textContent += '|ready');
            await new Promise(resolve => setTimeout(resolve, 2000));
            output.textContent = 'module';
        "#
        .into(),
        kind: ScriptKind::Module,
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");

    let initial = runtime.execute_initial(&[input]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.executed, 0);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "pending"
    );

    let settled = runtime.advance_time(Duration::from_millis(500), 8);
    assert!(settled.errors.is_empty(), "{:?}", settled.errors);
    assert_eq!(settled.executed, 1);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "module|ready"
    );
}

#[test]
fn pending_additional_module_dispatches_load_only_after_evaluation() {
    let dom = crate::engine::dom::parse_with_scripting(
        "<body><script id=module type=module></script><div>pending</div></body>",
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let setup = ScriptInput {
        node: node.clone(),
        source_url: "https://example.com/#setup".into(),
        code: "document.getElementById('module').addEventListener('load', () => document.querySelector('div').textContent = 'loaded');".into(),
        kind: ScriptKind::Classic,
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[setup]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);

    let module = ScriptInput {
        node,
        source_url: "https://example.com/additional.js".into(),
        code: "await new Promise(resolve => setTimeout(resolve, 250));".into(),
        kind: ScriptKind::Module,
        finish_lifecycle: false,
    };
    let started = runtime.execute_additional_with_loader(&[module], None);
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    assert_eq!(started.executed, 0);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "pending"
    );

    let settled = runtime.advance_time(Duration::from_millis(250), 8);
    assert!(settled.errors.is_empty(), "{:?}", settled.errors);
    assert_eq!(settled.executed, 1);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "loaded"
    );
}
