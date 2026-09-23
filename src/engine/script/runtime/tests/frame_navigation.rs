use super::*;
use crate::engine::script::ScriptFetchAction;
mod cross_origin;
mod effects;
mod failures;
mod file_reader;
mod history;
mod images;
mod names;
mod policy;
mod ports;
mod scripts;
mod streaming;
mod styles;
mod workers;

fn start(source: &str) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(source, true);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/parent/index");
    let result = runtime.execute_initial_before_document_completion(&script_inputs(&dom), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    (dom, runtime)
}

fn evaluate(
    runtime: &mut ScriptRuntime,
    dom: &dom::Dom,
    code: &str,
) -> crate::engine::script::ScriptOutcome {
    let script = dom.elements_named("script").next().unwrap();
    let result =
        runtime.execute_additional_with_loader(&[input(&script, "test.js", code, false)], None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    result
}

fn drain(runtime: &mut ScriptRuntime) {
    for _ in 0..100 {
        let result = runtime.advance_time(Duration::ZERO, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(
            !result
                .console
                .iter()
                .any(|message| message.starts_with("error:")),
            "{:?}",
            result.console
        );
    }
}

fn response(url: &str, source: &str) -> crate::fetch::FetchResponse {
    crate::fetch::FetchResponse {
        response_type: crate::fetch::ResponseType::Basic,
        url_list: vec![crate::fetch::FetchUrl::parse(url).unwrap()],
        status: 200,
        headers: crate::fetch::HeaderList::new(),
        body: crate::fetch::Body::from_bytes(source.as_bytes().to_vec()),
    }
}

#[test]
fn child_load_precedes_iframe_load_and_parent_window_load() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.order = [];
        document.addEventListener('DOMContentLoaded', () => order.push('parent-dcl'));
        addEventListener('load', () => order.push('parent-load'));
        const f = document.createElement('iframe');
        f.srcdoc = '<script>parent.order.push("child-script"); addEventListener("load", () => parent.order.push("child-load"))<\/script>';
        f.onload = () => order.push('iframe-load');
        document.body.append(f);
    </script>"#,
    );
    assert!(runtime.finish_document_lifecycle().errors.is_empty());
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if (order.join(',') !== 'parent-dcl,child-script,child-load,iframe-load,parent-load') throw Error(order);",
    );
    assert!(runtime.document_load_finished());
}

#[test]
fn superseded_navigation_is_aborted_and_cannot_replace_the_new_document() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f = document.createElement('iframe'); f.src = '/first'; document.body.append(f);
    </script>"#,
    );
    let first = runtime
        .advance_time(Duration::ZERO, 1)
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, .. } => Some(id),
            _ => None,
        })
        .unwrap();
    let changed = evaluate(&mut runtime, &dom, "f.src = '/second';");
    assert!(
        changed
            .fetch_actions
            .iter()
            .any(|action| matches!(action, ScriptFetchAction::Abort { id } if *id == first))
    );
    let second = runtime
        .advance_time(Duration::ZERO, 1)
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, .. } => Some(id),
            _ => None,
        })
        .unwrap();
    assert!(
        runtime
            .complete_fetch_with_loader(
                second,
                Ok(response("https://example.com/second", "<p>new</p>")),
                None
            )
            .errors
            .is_empty()
    );
    assert!(
        runtime
            .complete_fetch_with_loader(
                first,
                Ok(response("https://example.com/first", "<p>old</p>")),
                None
            )
            .errors
            .is_empty()
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if (f.contentDocument.body.textContent !== 'new') throw Error('stale navigation committed');",
    );
}

#[test]
fn changing_relative_src_uses_embedding_document_base_not_previous_frame_url() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f = document.createElement('iframe'); f.src = '/other/frame'; document.body.append(f);
    </script>"#,
    );
    let first = runtime
        .advance_time(Duration::ZERO, 1)
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, .. } => Some(id),
            _ => None,
        })
        .unwrap();
    assert!(
        runtime
            .complete_fetch_with_loader(
                first,
                Ok(response("https://example.com/other/frame", "<p>one</p>")),
                None
            )
            .errors
            .is_empty()
    );
    drain(&mut runtime);
    evaluate(&mut runtime, &dom, "f.src = 'next';");
    let next = runtime
        .advance_time(Duration::ZERO, 1)
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { request, .. } => Some(request),
            _ => None,
        })
        .unwrap();
    assert_eq!(next.url.as_str(), "https://example.com/parent/next");
}

#[test]
fn sandbox_without_allow_scripts_parses_but_does_not_execute() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f = document.createElement('iframe'); f.sandbox = 'allow-same-origin';
        f.srcdoc = '<p>inert</p><script>parent.document.body.dataset.bad = "executed"<\/script>';
        document.body.append(f);
    </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if (document.body.dataset.bad || f.contentDocument.querySelector('p').textContent !== 'inert') throw Error('sandbox scripts');",
    );
    evaluate(
        &mut runtime,
        &dom,
        "for(const code of [()=>f.contentWindow.eval('1'),()=>new f.contentWindow.Function('return 1')]) {try {code();throw Error('codegen escaped')} catch(e){if(e.name!=='EvalError') throw e}}",
    );
}

#[test]
fn parser_blocking_child_script_holds_back_the_tail_until_fetch_completion() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f = document.createElement('iframe');
        f.srcdoc = '<script src="/blocking.js"><\/script><p id="tail">tail</p>';
        document.body.append(f);
    </script>"#,
    );
    let mut request = None;
    for _ in 0..12 {
        let result = runtime.advance_time(Duration::ZERO, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        request = result
            .fetch_actions
            .into_iter()
            .find_map(|action| match action {
                ScriptFetchAction::Start { id, request } => Some((id, request)),
                _ => None,
            })
            .or(request);
    }
    evaluate(
        &mut runtime,
        &dom,
        "if (f.contentDocument.getElementById('tail')) throw Error('parser passed blocking script');",
    );
    let (id, request) = request.expect("external child script request");
    assert_eq!(request.url.as_str(), "https://example.com/blocking.js");
    let result = runtime.complete_fetch_with_loader(id, Ok(response(request.url.as_str(), "if(document.getElementById('tail')) throw Error('tail'); document.write('<b id=written>ok</b>');")), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if (f.contentDocument.body.textContent !== 'oktail') throw Error(f.contentDocument.body.textContent);",
    );
}
