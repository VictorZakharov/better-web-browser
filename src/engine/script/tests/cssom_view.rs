use super::*;

fn execute_viewport_script(
    dom: &dom::Dom,
    runtime: &mut ScriptRuntime,
    fragment: &str,
) -> ScriptOutcome {
    let script = dom.elements_named("script").next().unwrap();
    runtime.execute_initial(&[ScriptInput {
        source_url: format!("https://example.com/#{fragment}"),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }])
}

#[test]
fn window_and_root_client_viewports_have_distinct_scrollbar_geometry() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><html><body><script>
            document.body.setAttribute('data-result', [
                innerWidth, innerHeight,
                document.documentElement.clientWidth,
                document.documentElement.clientHeight,
                document.body.clientWidth,
                document.compatMode
            ].join(','));
        </script></body></html>"#,
        true,
    );
    let root = dom.elements_named("html").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_media_environment(crate::engine::MediaEnvironment::new(
        1000.0, 700.0, 1.0, false,
    ));
    runtime.set_layout_viewport(985.0, 680.0);
    runtime.set_layout_geometry(&HashMap::from([
        (
            root.id(),
            RectF {
                width: 1200.0,
                height: 900.0,
                ..RectF::default()
            },
        ),
        (
            body.id(),
            RectF {
                width: 600.0,
                height: 500.0,
                ..RectF::default()
            },
        ),
    ]));

    let outcome = execute_viewport_script(&dom, &mut runtime, "viewport");

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        body.attr("data-result").as_deref(),
        Some("1000,700,985,680,600,CSS1Compat")
    );
}

#[test]
fn quirks_mode_uses_body_for_viewport_client_dimensions() {
    let dom = dom::parse_with_scripting(
        r#"<html><body><script>
            document.body.setAttribute('data-result', [
                document.documentElement.clientWidth,
                document.body.clientWidth,
                document.body.clientHeight,
                document.compatMode
            ].join(','));
        </script></body></html>"#,
        true,
    );
    let root = dom.elements_named("html").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_viewport(985.0, 680.0);
    runtime.set_quirks_mode(true);
    runtime.set_layout_geometry(&HashMap::from([(
        root.id(),
        RectF {
            width: 1200.0,
            height: 900.0,
            ..RectF::default()
        },
    )]));

    let outcome = execute_viewport_script(&dom, &mut runtime, "quirks");

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        body.attr("data-result").as_deref(),
        Some("1200,985,680,BackCompat")
    );
}

#[test]
fn resize_updates_window_and_layout_viewports_before_dispatch() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><html><body><script>
            addEventListener('resize', event => document.body.setAttribute('data-result', [
                innerWidth, innerHeight,
                document.documentElement.clientWidth,
                document.documentElement.clientHeight,
                event.isTrusted
            ].join(',')));
        </script></body></html>"#,
        true,
    );
    let body = dom.elements_named("body").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = execute_viewport_script(&dom, &mut runtime, "resize");
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);

    let result = runtime.dispatch_user_input(UserInputEvent::Viewport {
        width: 1200.0,
        height: 800.0,
        layout_width: 1185.0,
        layout_height: 780.0,
        scale: 1.25,
    });

    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert_eq!(
        body.attr("data-result").as_deref(),
        Some("1200,800,1185,780,true")
    );
}
