use super::*;
use crate::engine::dom::Node;
use crate::engine::{LayoutOutput, MediaEnvironment, RectF};
use std::collections::HashMap;

#[test]
fn child_viewport_and_cross_realm_observer_use_the_painted_frame_box() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f = document.createElement('iframe');
        f.srcdoc = '<!doctype html><div id="target" style="width:30px;height:30px"></div>';
        document.body.append(f);
        </script>"#,
    );
    drain(&mut runtime);
    runtime.set_media_environment(MediaEnvironment::new(800.0, 600.0, 1.0, false));
    let iframe = dom.elements_named("iframe").next().unwrap();
    let frame_rect = RectF {
        x: 10.0,
        y: 20.0,
        width: 150.0,
        height: 100.0,
    };
    runtime.set_layout_geometry(&HashMap::from([(iframe.id(), frame_rect)]));
    let snapshots = runtime.frame_paint_snapshots();
    assert_eq!(snapshots.len(), 1);
    let snapshot = &snapshots[0];
    let target = Node::descendants(&snapshot.dom)
        .find(|node| node.attr("id").as_deref() == Some("target"))
        .unwrap();
    let mut child_layout = LayoutOutput::default();
    child_layout.node_bounds.insert(
        target.id(),
        RectF {
            x: 8.0,
            y: 8.0,
            width: 30.0,
            height: 30.0,
        },
    );
    (snapshot.publish_geometry)(&child_layout, frame_rect);
    let outcome = runtime.dispatch_frame_viewports(&[(snapshot.document, frame_rect)]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    evaluate(
        &mut runtime,
        &dom,
        r#"
        if (f.contentWindow.innerWidth !== 150 ||
            f.contentDocument.documentElement.clientWidth !== 150)
            throw Error('child viewport was not updated: ' +
                f.contentWindow.innerWidth + '/' +
                f.contentDocument.documentElement.clientWidth);
        window.entries = [];
        window.observer = new IntersectionObserver(changes => entries.push(...changes));
        observer.observe(f.contentDocument.getElementById('target'));
        "#,
    );
    let outcome = runtime.gather_intersection_observers();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let outcome = runtime.notify_intersection_observers();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    evaluate(
        &mut runtime,
        &dom,
        r#"
        if (entries.length !== 1 || !entries[0].isIntersecting ||
            entries[0].boundingClientRect.x !== 8 ||
            entries[0].intersectionRect.x !== 18 ||
            entries[0].intersectionRect.y !== 28)
            throw Error('cross-frame entry coordinate spaces: ' + JSON.stringify(entries.map(
                e => [e.isIntersecting, e.boundingClientRect.x, e.intersectionRect.x, e.intersectionRect.y])));
        "#,
    );
    dom.document.scroll_offset.set((0.0, 100.0));
    let outcome = runtime.gather_intersection_observers();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let outcome = runtime.notify_intersection_observers();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    evaluate(
        &mut runtime,
        &dom,
        "if (entries.length !== 2 || entries[1].isIntersecting) throw Error('parent scroll did not reproject child target');",
    );
}
