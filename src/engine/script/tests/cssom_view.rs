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
        1000.4, 700.4, 1.0, false,
    ));
    runtime.set_layout_viewport(985.4, 680.4);
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
        width: 1200.4,
        height: 800.6,
        layout_width: 1185.4,
        layout_height: 780.4,
        scale: 1.25,
    });

    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert_eq!(
        body.attr("data-result").as_deref(),
        Some("1200,801,1185,780,true")
    );
}

#[test]
fn geometry_reads_flush_layout_once_per_dom_mutation_version() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><body><div id="target"></div><script>
            const target = document.getElementById('target');
            const first = target.getBoundingClientRect().width;
            const cached = target.getBoundingClientRect().width;
            target.style.width = '200px';
            const changed = target.getBoundingClientRect().width;
            document.body.dataset.result = [first, cached, changed].join(',');
        </script></body>"#,
        true,
    );
    let target = dom.elements_named("div").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let calls = std::rc::Rc::new(std::cell::Cell::new(0_usize));
    let observed_calls = std::rc::Rc::clone(&calls);
    let target_id = target.id();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_flush_callback(Box::new(move || {
        let next = observed_calls.get() + 1;
        observed_calls.set(next);
        HashMap::from([(
            target_id,
            RectF {
                width: if next == 1 { 100.0 } else { 200.0 },
                height: 20.0,
                ..RectF::default()
            },
        )])
    }));

    let outcome = execute_viewport_script(&dom, &mut runtime, "synchronous-layout");

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(body.attr("data-result").as_deref(), Some("100,100,200"));
    assert_eq!(calls.get(), 2);
}

#[test]
fn fixed_offset_parent_uses_computed_containing_block_triggers() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><style>
            .fixed { position: fixed }
            #transform { transform: translateX(10px) }
            #perspective { perspective: 10px }
            #filter { filter: opacity(25%) }
            #transform-style { transform-style: preserve-3d }
            #contain { contain: paint }
            #will-change { will-change: transform }
        </style><body>
            <div id=ordinary><div id=a class=fixed></div></div>
            <div id=transform><div id=b class=fixed></div></div>
            <div id=perspective><div id=c class=fixed></div></div>
            <div id=filter><div id=d class=fixed></div></div>
            <div id=transform-style><div id=e class=fixed></div></div>
            <div id=contain><div id=f class=fixed></div></div>
            <div id=will-change><div id=g class=fixed></div></div>
            <output>no</output><script>
                const byId = id => document.getElementById(id);
                const accepted = byId('a').offsetParent === null &&
                    byId('b').offsetParent === byId('transform') &&
                    byId('c').offsetParent === byId('perspective') &&
                    byId('d').offsetParent === byId('filter') &&
                    byId('e').offsetParent === byId('transform-style') &&
                    byId('f').offsetParent === byId('contain') &&
                    byId('g').offsetParent === byId('will-change') &&
                    getComputedStyle(byId('transform-style')).transformStyle === 'preserve-3d';
                if (accepted) document.querySelector('output').textContent = 'yes';
            </script>
        </body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn offset_geometry_is_html_only_and_hidden_subtrees_have_no_offset_parent() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><style>#hidden { display: none }</style><body>
            <div id=hidden><p id=descendant></p></div><svg id=foreign></svg>
            <output>no</output><script>
                const accepted = document.getElementById('hidden').offsetParent === null &&
                    document.getElementById('descendant').offsetParent === null &&
                    document.getElementById('foreign').offsetParent === undefined;
                if (accepted) document.querySelector('output').textContent = 'yes';
            </script>
        </body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn body_has_no_offset_parent_and_non_root_html_is_computed_normally() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><style>html, body { position: relative }</style><body>
            <output>no</output><script>
                const root = document.documentElement;
                const nestedHtml = document.createElement('html');
                const nestedBody = document.createElement('body');
                nestedHtml.appendChild(nestedBody);
                root.appendChild(nestedHtml);
                const accepted = root.offsetParent === null &&
                    nestedHtml.offsetParent === root && nestedBody.offsetParent === null;
                if (accepted) document.querySelector('output').textContent = 'yes';
            </script>
        </body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn offset_coordinates_are_relative_to_the_nearest_offset_parent_border_edge() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><style>#parent {
            position: absolute; border-left: 2px solid; border-top: 3px solid
        }</style><body>
            <div id=parent><span id=target></span></div><script>
                const target = document.getElementById('target');
                const parent = document.getElementById('parent');
                document.body.dataset.result = [
                    target.offsetParent === parent,
                    target.offsetLeft, target.offsetTop, parent.clientLeft, parent.clientTop
                ].join(',');
            </script></body>"#,
        true,
    );
    let parent = dom.elements_named("div").next().unwrap();
    let target = dom.elements_named("span").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_geometry(&HashMap::from([
        (
            parent.id(),
            RectF {
                x: 50.0,
                y: 40.0,
                width: 100.0,
                height: 80.0,
            },
        ),
        (
            target.id(),
            RectF {
                x: 60.0,
                y: 50.0,
                width: 0.0,
                height: 0.0,
            },
        ),
    ]));

    let outcome = execute_viewport_script(&dom, &mut runtime, "offset-coordinates");

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(body.attr("data-result").as_deref(), Some("true,8,7,2,3"));
}

#[test]
fn client_dimensions_use_padding_boxes_and_input_inline_content_clipping() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><style>input, textarea {
            width: 300px; height: 200px; padding: 2px;
            border-style: solid; border-width: 10px 20px; box-sizing: content-box
        }</style><body><input><textarea></textarea><script>
            const metrics = element => [element.clientWidth, element.clientHeight,
                element.clientLeft, element.clientTop].join(':');
            document.body.dataset.result = [metrics(document.querySelector('input')),
                metrics(document.querySelector('textarea'))].join(',');
        </script></body>"#,
        true,
    );
    let input = dom.elements_named("input").next().unwrap();
    let textarea = dom.elements_named("textarea").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_geometry(&HashMap::from([
        (
            input.id(),
            RectF {
                width: 344.0,
                height: 224.0,
                ..RectF::default()
            },
        ),
        (
            textarea.id(),
            RectF {
                width: 344.0,
                height: 224.0,
                ..RectF::default()
            },
        ),
    ]));

    let outcome = execute_viewport_script(&dom, &mut runtime, "client-boxes");

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        body.attr("data-result").as_deref(),
        Some("300:204:22:10,304:204:20:10")
    );
}

#[test]
fn table_client_dimensions_use_the_table_wrapper_box() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><body><table style="border: 8px solid"><tbody><tr><td>a</td></tr></tbody></table>
        <script>const table = document.querySelector('table'); document.body.dataset.result =
            [table.clientWidth, table.clientHeight, table.clientLeft, table.clientTop,
             table.offsetWidth, table.offsetHeight].join(',');</script></body>"#,
        true,
    );
    let table = dom.elements_named("table").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_geometry(&HashMap::from([(
        table.id(),
        RectF {
            width: 100.0,
            height: 60.0,
            ..RectF::default()
        },
    )]));

    let outcome = execute_viewport_script(&dom, &mut runtime, "table-client-box");

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        body.attr("data-result").as_deref(),
        Some("100,60,0,0,100,60")
    );
}
