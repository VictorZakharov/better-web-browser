use super::*;

pub(super) fn requests(runtime: &mut ScriptRuntime) -> Vec<(u32, Box<crate::fetch::FetchRequest>)> {
    let mut requests = Vec::new();
    for _ in 0..30 {
        let result = runtime.advance_time(Duration::ZERO, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        for action in result.fetch_actions {
            if let ScriptFetchAction::Start { id, request } = action {
                requests.push((id, request));
            }
        }
    }
    requests
}

#[test]
fn child_scripts_wait_for_css_import_closure_and_see_cssom_before_load() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f=document.createElement('iframe');
        f.srcdoc='<link rel="stylesheet" href="/root.css"><script>parent.rules=document.styleSheets[0].cssRules.length;parent.ran=true;<\/script><p>tail</p>';
        document.body.append(f);
    </script>"#,
    );
    runtime.finish_document_lifecycle();
    let pending = requests(&mut runtime);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1.url.as_str(), "https://example.com/root.css");
    evaluate(
        &mut runtime,
        &dom,
        "if(window.ran || f.contentDocument.querySelector('p')) throw Error('script passed CSS');",
    );
    let result = runtime.complete_fetch_with_loader(
        pending[0].0,
        Ok(response(
            "https://example.com/root.css",
            "@import '/child.css'; p { color: red }",
        )),
        None,
    );
    let imported = result
        .fetch_actions
        .into_iter()
        .find_map(|a| match a {
            ScriptFetchAction::Start { id, request } => Some((id, request)),
            _ => None,
        })
        .or_else(|| requests(&mut runtime).pop())
        .expect("CSS import");
    assert_eq!(imported.1.url.as_str(), "https://example.com/child.css");
    assert!(!runtime.document_load_finished());
    evaluate(
        &mut runtime,
        &dom,
        "if(window.ran) throw Error('script passed import');",
    );
    runtime.complete_fetch_with_loader(
        imported.0,
        Ok(response(
            "https://example.com/child.css",
            "p { display: block }",
        )),
        None,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(!ran || rules!==2 || f.contentDocument.body.textContent!=='tail') throw Error('CSSOM/order');",
    );
    assert!(runtime.document_load_finished());
}

#[test]
fn child_failed_stylesheet_releases_scripts_and_reports_error_once() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f=document.createElement('iframe');window.failed=0;
        f.srcdoc='<link rel="stylesheet" href="/bad.css" onerror="parent.failed++"><script>parent.ran=true<\/script>';
        document.body.append(f);
    </script>"#,
    );
    runtime.finish_document_lifecycle();
    let pending = requests(&mut runtime);
    let mut invalid = response("https://example.com/bad.css", "not a sheet");
    invalid.headers.append("Content-Type", "text/html").unwrap();
    runtime.complete_fetch_with_loader(pending[0].0, Ok(invalid), None);
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(!ran || failed!==1) throw Error('stylesheet failure lifecycle');",
    );
    assert!(runtime.document_load_finished());
}

#[test]
fn fetch_identifiers_cannot_be_used_to_abort_another_document_request() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        fetch('/root');const f=document.createElement('iframe');document.body.append(f);
    </script>"#,
    );
    let result = evaluate(
        &mut runtime,
        &dom,
        "f.contentWindow.eval('__hostCall(\"fetchAbort\",1);__hostCall(\"fetchConsumed\",1,300)');",
    );
    assert!(
        result.fetch_actions.is_empty(),
        "{:?}",
        result.fetch_actions
    );
}
