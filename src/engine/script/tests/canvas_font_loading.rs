use super::*;

#[test]
fn canvas_assignment_empty_text_and_invalid_paint_do_not_start_font_loads() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
        const face=new FontFace('Untouched','url(/unused.ttf)');document.fonts.add(face);
        const ctx=document.createElement('canvas').getContext('2d');ctx.font='20px Untouched';
        ctx.measureText('');ctx.fillText('',0,20);ctx.strokeText('',0,20);
        ctx.fillText('AAAA',NaN,20);ctx.strokeText('AAAA',0,Infinity);
        ctx.fillText('AAAA',0,20,0);ctx.strokeText('AAAA',0,20,-1);
        document.querySelector('div').textContent=face.status+':'+document.fonts.status;
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.fetch_actions.is_empty());
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "unloaded:loaded"
    );
}

#[test]
fn canvas_font_matching_requests_only_the_matching_weight_and_unicode_range() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
        const faces=[
          new FontFace('Selected','url(/latin-bold.ttf)',{weight:'700',unicodeRange:'U+41'}),
          new FontFace('Selected','url(/greek-bold.ttf)',{weight:'700',unicodeRange:'U+391'}),
          new FontFace('Selected','url(/latin-normal.ttf)',{weight:'400',unicodeRange:'U+41'})
        ];faces.forEach(face=>document.fonts.add(face));
        const ctx=document.createElement('canvas').getContext('2d');ctx.font='bold 20px Selected';
        ctx.measureText('AAAA');ctx.measureText('AAAA');ctx.fillText('AAAA',0,20);
        document.querySelector('div').textContent=faces.map(face=>face.status).join(':');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "loading:unloaded:unloaded"
    );
    assert_eq!(outcome.fetch_actions.len(), 1);
    let ScriptFetchAction::Start { request, .. } = &outcome.fetch_actions[0] else {
        panic!("expected a font request");
    };
    assert_eq!(request.url.as_str(), "https://example.com/latin-bold.ttf");
    assert_eq!(request.destination, crate::fetch::RequestDestination::Font);
    assert_eq!(request.context, crate::fetch::RequestContext::Subresource);
    assert_eq!(request.mode, crate::fetch::RequestMode::Cors);
    assert_eq!(
        request.credentials,
        crate::fetch::CredentialsMode::SameOrigin
    );
}

#[test]
fn offscreen_canvas_in_a_worker_uses_its_own_font_set() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-fonts.js",
        r#"const face=new FontFace('WorkerFont','url(/worker.ttf)');self.fonts.add(face);
        const ctx=new OffscreenCanvas(80,40).getContext('2d');ctx.font='20px WorkerFont';
        ctx.measureText('AAAA');ctx.measureText('AAAA');postMessage(face.status);
        face.loaded.catch(error=>postMessage(error.name));"#,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected script import {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.messages.contains(&"\"loading\"".into()));
    let mut runtime = runtime.unwrap();
    assert_eq!(initial.fetch_actions.len(), 1);
    let ScriptFetchAction::Start { id, request } = &initial.fetch_actions[0] else {
        panic!("expected worker font request");
    };
    assert_eq!(request.url.as_str(), "https://example.test/worker.ttf");
    assert_eq!(request.destination, crate::fetch::RequestDestination::Font);
    let failed = runtime.deliver_fetch_event(
        *id,
        ScriptFetchEvent::Head(Err(crate::fetch::FetchError::network(
            "fixture font unavailable",
        ))),
    );
    assert!(failed.errors.is_empty(), "{:?}", failed.errors);
    let mut messages = initial.messages;
    messages.extend(failed.messages);
    for _ in 0..12 {
        let outcome = runtime.advance_time(Duration::ZERO, 8);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert!(
            outcome.fetch_actions.is_empty(),
            "measurement must not retry"
        );
        messages.extend(outcome.messages);
    }
    assert!(
        messages.contains(&"\"NetworkError\"".into()),
        "{messages:?}"
    );
}
