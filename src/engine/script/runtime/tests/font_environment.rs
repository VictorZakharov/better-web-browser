use super::*;
use crate::engine::script::{ScriptFetchOptions, ScriptKind};
use std::collections::{HashMap, HashSet};

fn start(layout: bool) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    if layout {
        runtime.set_layout_flush_callback(Box::new(|_, _| Some(HashMap::new())));
    }
    let outcome = runtime.execute_initial_before_document_completion(&[ScriptInput {
        node: dom.elements_named("script").next().unwrap(),
        source_url: "https://example.test/#ready".into(),
        code: "document.fonts.ready.then(value=>{document.body.setAttribute('data-ready',String(value===document.fonts)+':'+document.readyState);});if('__fontEnvironmentChanged' in globalThis)throw Error('private font hook leaked');".into(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    }], None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}

fn ready(dom: &dom::Dom) -> Option<String> {
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr("data-ready")
}

fn settle(runtime: &mut ScriptRuntime) {
    for _ in 0..8 {
        let outcome = runtime.advance_time(Duration::ZERO, 1);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
}

#[test]
fn initial_ready_is_pending_during_parsing_even_without_any_font_faces() {
    let (dom, mut runtime) = start(false);
    assert_eq!(
        ready(&dom),
        None,
        "initial readiness is not Promise.resolve(set)"
    );
    settle(&mut runtime);
    assert_eq!(ready(&dom), None);
    assert_eq!(
        runtime.next_timer_delay(),
        None,
        "parsing has no font polling timer"
    );
    assert!(runtime.finish_document_lifecycle().errors.is_empty());
    settle(&mut runtime);
    assert_eq!(ready(&dom).as_deref(), Some("true:complete"));
    assert_eq!(runtime.next_timer_delay(), None);
}

#[test]
fn pending_stylesheet_environment_can_outlive_window_load_without_busy_polling() {
    let (dom, mut runtime) = start(false);
    runtime.set_font_stylesheets_pending(true);
    assert!(runtime.finish_document_lifecycle().errors.is_empty());
    settle(&mut runtime);
    assert!(runtime.document_load_finished());
    assert_eq!(ready(&dom), None);
    assert_eq!(runtime.next_timer_delay(), None);
    runtime.set_font_stylesheets_pending(false);
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    settle(&mut runtime);
    assert_eq!(ready(&dom).as_deref(), Some("true:complete"));
    assert_eq!(runtime.next_timer_delay(), None);
}

#[test]
fn published_layout_releases_initial_ready_once_without_requiring_an_author_timer() {
    let (dom, mut runtime) = start(true);
    assert!(runtime.finish_document_lifecycle().errors.is_empty());
    settle(&mut runtime);
    assert_eq!(ready(&dom), None);
    assert_eq!(
        runtime.next_timer_delay(),
        None,
        "the renderer owns the next layout"
    );
    runtime.set_layout_geometry(&HashMap::new());
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    settle(&mut runtime);
    assert_eq!(ready(&dom).as_deref(), Some("true:complete"));
    // The ready callback itself mutates the document. That does not retract a
    // fulfilled promise or require recurring notifications at the same state.
    runtime.set_layout_geometry(&HashMap::new());
    settle(&mut runtime);
    assert_eq!(runtime.next_timer_delay(), None);
}

#[test]
fn cancelling_a_document_does_not_schedule_environment_settlement() {
    let (dom, mut runtime) = start(true);
    runtime.cancel_document();
    runtime.set_font_stylesheets_pending(false);
    runtime.set_layout_geometry(&HashMap::new());
    assert_eq!(runtime.next_timer_delay(), None);
    assert_eq!(ready(&dom), None);
}

#[test]
fn admitted_css_sources_survive_a_fast_response_but_not_removed_rules() {
    let (_, mut runtime) = start(false);
    let sheets = [crate::engine::css::StylesheetSource::injected(
        "https://example.test/fonts.css",
        "@font-face{font-family:Fast;src:url(/fast.ttf)}".into(),
    )];
    runtime.set_document_stylesheets(&sheets);
    let face = runtime.host.borrow_mut().connected_font_faces()[0].loading_identity();
    runtime.set_pending_css_fonts(HashSet::from([face.clone()]));
    runtime.set_pending_css_fonts(HashSet::new());
    assert_eq!(
        runtime.host.borrow().font_environment.requested_css_fonts,
        HashSet::from([face.clone()])
    );
    runtime.set_document_stylesheets(&[]);
    runtime.set_pending_css_fonts(HashSet::from([face]));
    assert!(
        runtime
            .host
            .borrow()
            .font_environment
            .requested_css_fonts
            .is_empty()
    );
}

#[test]
fn duplicate_css_rules_do_not_hide_later_admitted_faces() {
    let (_, mut runtime) = start(false);
    let duplicate = "@font-face{font-family:Repeated;src:url(/repeated.ttf)}";
    runtime.set_document_stylesheets(&[crate::engine::css::StylesheetSource::injected(
        "https://example.test/fonts.css",
        format!(
            "{}@font-face{{font-family:Last;src:url(/last.ttf)}}",
            duplicate.repeat(60)
        ),
    )]);
    let last = runtime.host.borrow_mut().connected_font_faces()[1].loading_identity();
    runtime.set_pending_css_fonts(HashSet::from([last.clone()]));
    assert!(
        runtime
            .host
            .borrow()
            .font_environment
            .requested_css_fonts
            .contains(&last)
    );
    assert_eq!(runtime.host.borrow_mut().connected_font_faces().len(), 2);
}

#[test]
fn admission_history_uses_the_same_bounded_face_list_as_javascript() {
    let (_, mut runtime) = start(false);
    let css = (0..80)
        .map(|index| format!("@font-face{{font-family:Face{index};src:url(/face{index}.ttf)}}"))
        .collect::<String>();
    runtime.set_document_stylesheets(&[crate::engine::css::StylesheetSource::injected(
        "https://example.test/fonts.css",
        css,
    )]);
    let sources = runtime
        .host
        .borrow_mut()
        .document_font_faces()
        .into_iter()
        .map(|face| face.loading_identity())
        .collect::<HashSet<_>>();
    runtime.set_pending_css_fonts(sources);
    runtime.set_pending_css_fonts(HashSet::new());
    let mut host = runtime.host.borrow_mut();
    let exposed = host
        .connected_font_faces()
        .into_iter()
        .map(|face| face.loading_identity())
        .collect();
    assert_eq!(host.font_environment.requested_css_fonts.len(), 64);
    assert_eq!(host.font_environment.requested_css_fonts, exposed);
}
