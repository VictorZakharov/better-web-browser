use super::*;
use std::collections::HashSet;

const URL: &str = "https://example.test/ahem.ttf";

fn start() -> (dom::Dom, ScriptRuntime) {
    start_with_completion(None)
}

fn start_with_completion(completed: Option<bool>) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    runtime.set_document_stylesheets(&[crate::engine::css::StylesheetSource::injected(
        "https://example.test/fonts.css",
        format!("@font-face{{font-family:Automatic;src:url('{URL}')}}"),
    )]);
    let identity = runtime.host.borrow_mut().connected_font_faces()[0].loading_identity();
    runtime.set_pending_css_fonts(HashSet::from([identity]));
    if let Some(success) = completed {
        if success {
            install_css_font(&mut runtime);
        }
        runtime.set_pending_css_fonts(HashSet::new());
    }
    let outcome = runtime.execute_initial_before_document_completion(&[ScriptInput {
        node: dom.elements_named("script").next().unwrap(),
        source_url: "https://example.test/#fonts".into(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
        code: r#"
            const events=[],set=document.fonts,face=[...set][0];
            const record=value=>{events.push(value);document.body.setAttribute('data-log',events.join('|'));};
            record(face.status+':'+set.status);
            set.onloading=event=>record('loading:'+event.isTrusted);
            face.loaded.then(()=>record('face:'+face.status),e=>record('face:'+e.name));
            set.ready.then(()=>record('ready:'+set.status));
            set.onloadingdone=event=>record('done:'+event.fontfaces.length+':'+event.isTrusted);
            set.onloadingerror=event=>record('error:'+event.fontfaces.length+':'+event.isTrusted);
        "#.into(),
    }], None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome.fetch_actions.is_empty(),
        "renderer font requests must not duplicate JS fetches"
    );
    assert!(runtime.finish_document_lifecycle().errors.is_empty());
    settle(&mut runtime);
    (dom, runtime)
}

fn settle(runtime: &mut ScriptRuntime) {
    for _ in 0..8 {
        let outcome = runtime.advance_time(Duration::ZERO, 1);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert!(outcome.fetch_actions.is_empty());
        assert!(
            outcome.font_actions.is_empty(),
            "CSS-installed faces are not installed a second time by JS"
        );
    }
}

fn log(dom: &dom::Dom) -> String {
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr("data-log")
        .unwrap_or_default()
}

fn install_css_font(runtime: &mut ScriptRuntime) {
    let mut page = crate::engine::Page::parse("", "https://example.test/");
    page.add_font(
        URL.into(),
        "Automatic".into(),
        400,
        false,
        include_bytes!("../../../../tests/canvas/fonts/ahem.ttf"),
    )
    .unwrap();
    runtime.set_loaded_font_urls(&page.fonts);
}

#[test]
fn automatically_requested_css_face_has_a_real_loading_period_and_task_completion() {
    let (dom, mut runtime) = start();
    assert_eq!(log(&dom), "loading:loading|loading:true");
    assert_eq!(runtime.next_timer_delay(), None);
    install_css_font(&mut runtime);
    runtime.set_pending_css_fonts(HashSet::new());
    assert_eq!(
        log(&dom),
        "loading:loading|loading:true",
        "publication itself cannot run author promises"
    );
    settle(&mut runtime);
    assert_eq!(
        log(&dom),
        "loading:loading|loading:true|face:loaded|ready:loaded|done:1:true"
    );
}

#[test]
fn fast_css_success_keeps_loading_events_when_transport_finishes_before_js_starts() {
    let (dom, mut runtime) = start_with_completion(Some(true));
    assert_eq!(
        log(&dom),
        "loading:loading|loading:true|face:loaded|ready:loaded|done:1:true"
    );
    assert_eq!(runtime.next_timer_delay(), None);
}

#[test]
fn fast_css_failure_rejects_loaded_and_reports_error_even_before_first_checkpoint() {
    let (dom, mut runtime) = start_with_completion(Some(false));
    assert_eq!(
        log(&dom),
        "loading:loading|loading:true|face:NetworkError|ready:loaded|done:0:true|error:1:true"
    );
    assert_eq!(runtime.next_timer_delay(), None);
}

#[test]
fn exhausted_css_font_rejects_the_face_but_fulfills_ready_and_reports_failed_faces() {
    let (dom, mut runtime) = start();
    runtime.set_pending_css_fonts(HashSet::new());
    settle(&mut runtime);
    assert_eq!(
        log(&dom),
        "loading:loading|loading:true|face:NetworkError|ready:loaded|done:0:true|error:1:true"
    );
    assert_eq!(runtime.next_timer_delay(), None);
}
