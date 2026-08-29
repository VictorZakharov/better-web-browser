use super::*;
use crate::engine::MediaEnvironment;

fn media_runtime(
    html: &str,
    width: f32,
    height: f32,
    dppx: f32,
    dark: bool,
) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(html, true);
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#media".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_media_environment(MediaEnvironment::new(width, height, dppx, dark));
    let outcome = runtime.execute_initial(&[input]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}

#[test]
fn match_media_exposes_cssom_view_objects_and_serialization() {
    let (dom, _runtime) = media_runtime(
        r#"<body><script>
            const query = matchMedia('all and (max-width:199px), (min-width: 200px)');
            const invalid = matchMedia('::');
            const event = new MediaQueryListEvent('change', { matches: true, media: 'screen' });
            document.body.setAttribute('data-result', [
                query.media, query.matches,
                query instanceof MediaQueryList, query instanceof EventTarget,
                typeof query.addListener, typeof query.removeListener,
                matchMedia('').matches, matchMedia('all').matches,
                invalid.media, invalid.matches,
                event.matches, event.media
            ].join('|'));
        </script></body>"#,
        800.0,
        600.0,
        1.0,
        false,
    );

    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-result"))
            .as_deref(),
        Some(
            "(max-width: 199px), (min-width: 200px)|true|true|true|function|function|true|true|not all|false|true|screen"
        )
    );
}

#[test]
fn viewport_changes_update_all_lists_before_firing_ordered_change_events() {
    let (dom, mut runtime) = media_runtime(
        r#"<body><script>
            const first = matchMedia('(min-width: 900px)');
            const second = matchMedia('(min-width: 900px)');
            const resolution = matchMedia('(min-resolution: 2dppx)');
            const color = matchMedia('(prefers-color-scheme: dark)');
            const events = [];
            const record = (name, event) => {
                events.push([name, event.matches, second.matches, event.isTrusted].join(':'));
                document.body.setAttribute('data-events', events.join(','));
            };
            first.addEventListener('change', event => record('first', event));
            second.addListener(event => record('second', event));
            resolution.onchange = event => record('resolution', event);
            color.onchange = event => record('color', event);
        </script></body>"#,
        800.0,
        600.0,
        1.0,
        false,
    );

    runtime.set_media_environment(MediaEnvironment::new(1000.0, 700.0, 2.0, true));
    let first_resize = runtime.dispatch_user_input(UserInputEvent::Viewport {
        width: 1000.0,
        height: 700.0,
        scale: 2.0,
    });
    assert!(
        first_resize.outcome.errors.is_empty(),
        "{:?}",
        first_resize.outcome.errors
    );
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some(
            "first:true:true:true,second:true:true:true,resolution:true:true:true,color:true:true:true"
        )
    );

    runtime.set_media_environment(MediaEnvironment::new(1000.0, 700.0, 2.0, true));
    runtime.dispatch_user_input(UserInputEvent::Viewport {
        width: 1000.0,
        height: 700.0,
        scale: 2.0,
    });
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some(
            "first:true:true:true,second:true:true:true,resolution:true:true:true,color:true:true:true"
        )
    );
}
