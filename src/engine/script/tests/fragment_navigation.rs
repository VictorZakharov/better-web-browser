use super::*;

#[test]
fn fragment_navigation_updates_url_scroll_and_queued_hashchange_without_reloading() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><body><h2 id=section>Section</h2><output></output><script>
        const seen = [];
        addEventListener('hashchange', event => seen.push([
            event instanceof HashChangeEvent, event.isTrusted, event.oldURL, event.newURL
        ].join('|')));
    </script>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_viewport(800.0, 600.0);
    runtime.set_layout_content_height(2000.0);
    let heading = dom.elements_named("h2").next().unwrap();
    runtime.set_layout_geometry(&HashMap::from([(
        heading.id(),
        RectF {
            y: 900.0,
            width: 300.0,
            height: 40.0,
            ..RectF::default()
        },
    )]));
    let script = dom.elements_named("script").next().unwrap();
    let input = |code: String| ScriptInput {
        source_url: "https://example.com/".into(),
        code,
        node: script.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    };
    assert!(
        runtime
            .execute_initial(&[input(script.text_content())])
            .errors
            .is_empty()
    );
    let outcome = runtime.execute_additional_with_loader(&[input("location.hash = 'section'; document.querySelector('output').textContent = [location.href, document.URL, scrollY, history.length, seen.length].join('|');".into())], None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.navigation_url.is_none());
    assert_eq!(outcome.viewport_scroll_y, Some(900.0));
    assert_eq!(outcome.history_actions.len(), 1);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "https://example.com/#section|https://example.com/#section|900|2|0"
    );
    let outcome = runtime.execute_additional_with_loader(
        &[input(
            "location.hash = 'section'; location.replace('#missing');".into(),
        )],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.history_actions.len(), 1);
    assert!(outcome.history_actions[0].replace);
    assert!(
        outcome.viewport_scroll_y.is_none(),
        "unmatched fragments must not reset scroll"
    );
    for _ in 0..8 {
        assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
    }
    runtime.execute_additional_with_loader(
        &[input(
            "document.querySelector('output').textContent = seen.join(';');".into(),
        )],
        None,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|https://example.com/|https://example.com/#section;true|true|https://example.com/#section|https://example.com/#missing"
    );
    let outcome =
        runtime.execute_additional_with_loader(&[input("location.hash = '';".into())], None);
    assert_eq!(outcome.viewport_scroll_y, Some(0.0));
    assert!(outcome.navigation_url.is_none());
}
