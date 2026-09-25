use super::*;
use crate::engine::layout::{HitTestSnapshot, LayoutOutput};

#[test]
fn document_point_queries_follow_paint_order_and_native_clips() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><html><body><div id="back"></div><div id="front"></div>
        <output></output><script>
        const ids = (x, y) => document.elementsFromPoint(x, y).map(e => e.id || e.localName).join(',');
        document.querySelector('output').textContent = [
            document.elementFromPoint(20, 20)?.id,
            ids(20, 20),
            document.elementFromPoint(5, 5)?.id,
            ids(5, 5),
            document.elementFromPoint(-1, 20) === null,
            document.elementsFromPoint(2000, 20).length === 0
        ].join('|');
        </script></body></html>"#,
        true,
    );
    let html = dom.elements_named("html").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let back = dom.elements_named("div").next().unwrap();
    let front = dom.elements_named("div").nth(1).unwrap();
    let viewport = RectF {
        width: 1280.0,
        height: 720.0,
        ..RectF::default()
    };
    let box_rect = RectF {
        width: 100.0,
        height: 100.0,
        ..RectF::default()
    };
    let layout = LayoutOutput {
        node_bounds: HashMap::from([
            (html.id(), viewport),
            (body.id(), viewport),
            (back.id(), box_rect),
            (front.id(), box_rect),
        ]),
        node_paint_order: vec![html.id(), body.id(), back.id(), front.id()],
        clip_paths: HashMap::from([(
            front.id(),
            RectF {
                x: 10.0,
                y: 10.0,
                width: 80.0,
                height: 80.0,
            },
        )]),
        ..LayoutOutput::default()
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_geometry(&layout.node_bounds);
    runtime.set_hit_test_snapshot(&layout);
    let script = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#point-query".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "front|front,back,body,html|back|back,body,html|true|true"
    );
    let snapshot = HitTestSnapshot::from_layout(&layout, &dom.document);
    assert_eq!(snapshot.elements_at(5.0, 5.0)[0].id(), back.id());
}

#[test]
fn pointer_events_is_inherited_and_exposed_through_cssom() {
    let (dom, outcome) = execute_html(
        r#"<style>main{pointer-events:none}button{pointer-events:auto}</style>
        <main><span></span><button></button></main><output></output><script>
        const main = document.querySelector('main');
        const span = document.querySelector('span');
        const button = document.querySelector('button');
        const result = [
            CSS.supports('pointer-events', 'none'),
            !CSS.supports('pointer-events', 'painted'),
            getComputedStyle(main).pointerEvents === 'none',
            getComputedStyle(span).pointerEvents === 'none',
            getComputedStyle(button).pointerEvents === 'auto'
        ];
        document.querySelector('output').textContent = result.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,true,true,true"
    );
}
