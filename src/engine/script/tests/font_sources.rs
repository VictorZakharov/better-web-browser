use super::network::{pending_runtime, test_response};
use super::*;
use crate::engine::script::ScriptFetchEvent;

const AHEM: &[u8] = include_bytes!("../../../../tests/canvas/fonts/ahem.ttf");

fn body(dom: &dom::Dom) -> String {
    dom.elements_named("div").next().unwrap().text_content()
}

fn respond(runtime: &mut ScriptRuntime, id: u32, status: u16, bytes: &[u8]) {
    let mut response = test_response(b"");
    response.status = status;
    let head =
        runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Head(Ok(response)), None);
    assert!(head.errors.is_empty(), "{:?}", head.errors);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(bytes.to_vec()), None);
    let end = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert!(end.errors.is_empty(), "{:?}", end.errors);
}

fn advance(runtime: &mut ScriptRuntime) -> ScriptOutcome {
    let outcome = runtime.advance_time(Duration::ZERO, 64);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    outcome
}

fn next_request(outcome: &ScriptOutcome, wanted: &str) -> u32 {
    let starts: Vec<_> = outcome
        .fetch_actions
        .iter()
        .filter_map(|action| match action {
            ScriptFetchAction::Start { id, request } => Some((*id, request)),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 1, "one candidate, not parallel downloads");
    assert_eq!(starts[0].1.url.as_str(), wanted);
    assert_eq!(starts[0].1.mode, crate::fetch::RequestMode::Cors);
    starts[0].0
}

#[test]
fn download_and_decode_fallback_keep_one_promise_and_one_loading_period() {
    let (dom, mut runtime, first) = pending_runtime(
        r#"
        const face = new FontFace('FallbackAhem',
            "local('Missing'), url('/one') format(woff2), url('/two'), url('/three')");
        const events=[];
        const record=value=>{events.push(value);document.body.setAttribute('data-events',events.join('|'));};
        document.fonts.add(face);
        document.fonts.onloading=()=>record('loading');
        document.fonts.onloadingdone=e=>record('done:'+e.fontfaces.length);
        document.fonts.onloadingerror=()=>record('error');
        const loaded=face.load();
        if(loaded!==face.loaded||face.load()!==loaded)throw Error('stable loading promise');
        document.querySelector('div').textContent=face.status;
        loaded.then(()=>{
            const ctx=document.createElement('canvas').getContext('2d');
            ctx.font='20px FallbackAhem';
            document.querySelector('div').textContent='loaded:'+ctx.measureText('ABC').width;
        },e=>document.querySelector('div').textContent=e.name);
        document.fonts.ready.then(()=>record('ready'));
    "#,
    );
    assert_eq!(body(&dom), "loading");
    respond(&mut runtime, first, 404, b"not a font");
    assert_eq!(body(&dom), "loading", "no microtask completion");
    let second = next_request(&advance(&mut runtime), "https://example.com/two");
    assert_eq!(body(&dom), "loading");
    respond(&mut runtime, second, 200, b"invalid font bytes");
    let third = next_request(&advance(&mut runtime), "https://example.com/three");
    assert_eq!(body(&dom), "loading");
    respond(&mut runtime, third, 200, AHEM);
    let finished = advance(&mut runtime);
    assert!(finished.fetch_actions.is_empty());
    assert_eq!(
        body(&dom),
        "loaded:60",
        "fallback installs actual decoded Ahem bytes"
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-events")
            .as_deref(),
        Some("loading|ready|done:1")
    );
}

#[test]
fn all_candidates_failing_reports_only_the_final_failure() {
    let (dom, mut runtime, first) = pending_runtime(
        r#"
        const face=new FontFace('Broken','url(/one), url(/two)');
        document.fonts.add(face);
        let errors=0;
        document.fonts.onloadingerror=e=>{
            errors++;document.querySelector('div').textContent=face.status+':'+errors+':'+e.fontfaces.length;
        };
        face.load().catch(()=>{});
    "#,
    );
    respond(&mut runtime, first, 200, b"bad font");
    let second = next_request(&advance(&mut runtime), "https://example.com/two");
    assert_eq!(body(&dom), "pending");
    respond(&mut runtime, second, 500, b"server error");
    advance(&mut runtime);
    assert_eq!(body(&dom), "error:1:1");
}

#[test]
fn unsupported_hints_are_never_downloaded_and_urls_are_css_decoded() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
        const face=new FontFace('Escaped',String.raw`url('/bad.ttf')format(svg),
            url('/no\2d extension\2c name')format(w\6f ff2)`);
        face.load().catch(()=>{});
        document.querySelector('div').textContent=face.status;
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    next_request(&outcome, "https://example.com/no-extension,name");
    assert_eq!(body(&dom), "loading");
}

#[test]
fn downloaded_invalid_font_bytes_reject_with_network_error_not_buffer_syntax_error() {
    let (dom, mut runtime, first) = pending_runtime(
        r#"
        const face=new FontFace('InvalidDownload','url(/one),url(/two)');
        document.fonts.add(face);
        face.loaded.catch(error=>document.querySelector('div').textContent=face.status+':'+error.name);
        face.load();
    "#,
    );
    respond(&mut runtime, first, 200, b"invalid first candidate");
    let second = next_request(&advance(&mut runtime), "https://example.com/two");
    assert_eq!(body(&dom), "pending", "intermediate decode is not terminal");
    respond(&mut runtime, second, 200, b"invalid last candidate");
    let finished = advance(&mut runtime);
    assert!(finished.fetch_actions.is_empty());
    assert_eq!(body(&dom), "error:NetworkError");
}

#[test]
fn local_only_source_is_valid_but_rejects_as_unavailable_not_bad_syntax() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
        const face=new FontFace('LocalOnly','local("Not Installed")');
        const before=face.status;
        face.load().catch(e=>document.querySelector('div').textContent=before+':'+face.status+':'+e.name);
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.fetch_actions.is_empty());
    assert_eq!(body(&dom), "unloaded:error:NetworkError");
}

#[test]
fn cancellation_discards_fallback_without_starting_another_download() {
    let (dom, mut runtime, first) = pending_runtime(
        r#"
        const face=new FontFace('Canceled','url(/one),url(/two)');
        face.load().then(()=>document.querySelector('div').textContent='loaded',()=>{});
    "#,
    );
    respond(&mut runtime, first, 200, b"bad font");
    runtime.cancel_document();
    assert_eq!(runtime.next_timer_delay(), None);
    let canceled = runtime.advance_time(Duration::ZERO, 64);
    assert!(canceled.fetch_actions.is_empty());
    assert_eq!(canceled.errors.len(), 1);
    assert!(canceled.errors[0].contains("document was cancelled"));
    assert_eq!(body(&dom), "pending");
}

#[test]
fn worker_download_fallback_uses_worker_fonts_and_the_worker_task_queue() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/font-worker.js",
        r#"
        const face=new FontFace('WorkerFallback','url(/one),url(/two)');
        fonts.add(face);
        self.fetch=()=>{throw Error('author override must not intercept platform loading');};
        face.load().then(()=>{
            const ctx=new OffscreenCanvas(80,60).getContext('2d');
            ctx.font='20px WorkerFallback';
            postMessage(face.status+':'+ctx.measureText('ABC').width+':'+fonts.size);
        },e=>postMessage(e.name));
        "#,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected import {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let Some(ScriptFetchAction::Start { id: first, .. }) = initial.fetch_actions.first() else {
        panic!("expected first source");
    };
    let mut runtime = runtime.unwrap();
    runtime.deliver_fetch_event(*first, ScriptFetchEvent::Head(Ok(test_response(b""))));
    runtime.deliver_fetch_event(*first, ScriptFetchEvent::Chunk(b"bad font".to_vec()));
    let ended = runtime.deliver_fetch_event(*first, ScriptFetchEvent::End);
    assert!(ended.messages.is_empty());
    let turn = runtime.advance_time(Duration::ZERO, 64);
    assert!(turn.errors.is_empty(), "{:?}", turn.errors);
    let Some(ScriptFetchAction::Start {
        id: second,
        request,
    }) = turn.fetch_actions.first()
    else {
        panic!("expected fallback source: {:?}", turn.errors);
    };
    assert_eq!(request.url.as_str(), "https://example.com/two");
    assert!(turn.messages.is_empty());
    runtime.deliver_fetch_event(*second, ScriptFetchEvent::Head(Ok(test_response(b""))));
    runtime.deliver_fetch_event(*second, ScriptFetchEvent::Chunk(AHEM.to_vec()));
    runtime.deliver_fetch_event(*second, ScriptFetchEvent::End);
    let turn = runtime.advance_time(Duration::ZERO, 64);
    assert!(turn.errors.is_empty(), "{:?}", turn.errors);
    assert_eq!(turn.messages, ["\"loaded:60:1\""]);
    assert!(turn.fetch_actions.is_empty());
}
