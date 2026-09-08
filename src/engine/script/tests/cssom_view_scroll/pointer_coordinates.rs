use super::*;

#[test]
fn native_pointer_and_related_boundary_coordinates_match_scrolled_client_rects() {
    let (dom, mut runtime) = initialize(
        r#"<!doctype html><body>
        <span id=a></span><span id=b></span><script>
            const a = document.getElementById('a'), b = document.getElementById('b');
            const events = [];
            const record = e => {
                const r = e.target.getBoundingClientRect();
                events.push([e.type,e.clientX,e.clientY,e.pageX,e.pageY,e.view === window,
                    e.relatedTarget?.id || null,e.clientX-r.left,e.clientY-r.top]);
            };
            for (const type of ['pointerenter','mouseenter','pointermove','mousemove','mouseout'])
                a.addEventListener(type, record);
            b.addEventListener('mouseover', record);
        </script></body>"#,
        |dom| {
            HashMap::from([
                (
                    dom.elements_named("span").next().unwrap().id(),
                    box_at(100.0, 1000.0),
                ),
                (
                    dom.elements_named("span").nth(1).unwrap().id(),
                    box_at(250.0, 1100.0),
                ),
            ])
        },
    );
    scroll(&mut runtime, 40.0, 900.0);
    evaluate(&dom, &mut runtime, "window.scrollX = window.scrollY = 999;");
    for (index, x, y) in [(0, 125.0, 1020.0), (1, 280.0, 1120.0)] {
        let input = runtime.dispatch_user_input(UserInputEvent::Pointer {
            target: dom.elements_named("span").nth(index),
            phase: "move",
            button: 0,
            buttons: 0,
            x,
            y,
            activate: false,
            modifiers: UserInputModifiers::default(),
        });
        assert!(
            input.outcome.errors.is_empty(),
            "{:?}",
            input.outcome.errors
        );
        assert!(
            input.outcome.console.is_empty(),
            "{:?}",
            input.outcome.console
        );
    }
    evaluate(
        &dom,
        &mut runtime,
        "document.body.dataset.result = JSON.stringify(events);",
    );
    assert_eq!(
        result(&dom),
        concat!(
            "[[\"pointerenter\",85,120,125,1020,true,null,25,20],",
            "[\"mouseenter\",85,120,125,1020,true,null,25,20],",
            "[\"pointermove\",85,120,125,1020,true,null,25,20],",
            "[\"mousemove\",85,120,125,1020,true,null,25,20],",
            "[\"mouseout\",240,220,280,1120,true,\"b\",180,120],",
            "[\"mouseover\",240,220,280,1120,true,\"a\",30,20]]"
        )
    );
}

#[test]
fn synthetic_mouse_page_coordinates_follow_native_scroll_and_ignore_page_initializers() {
    let (dom, mut runtime) = initialize(
        r#"<!doctype html><body><script>
        const e = new MouseEvent('mousemove', {clientX:5,clientY:6,pageX:999,pageY:999});
        const samples = [];
        const sample = () => samples.push([e.clientX,e.clientY,e.pageX,e.pageY,e.x,e.y,e.isTrusted]);
        sample();
        document.addEventListener('mousemove', sample);
    </script></body>"#,
        |_| HashMap::new(),
    );
    scroll(&mut runtime, 40.0, 900.0);
    evaluate(
        &dom,
        &mut runtime,
        "sample(); document.dispatchEvent(e); sample();",
    );
    scroll(&mut runtime, 80.0, 1000.0);
    evaluate(
        &dom,
        &mut runtime,
        "sample(); document.body.dataset.result = JSON.stringify(samples);",
    );
    assert_eq!(
        result(&dom),
        "[[5,6,5,6,5,6,false],[5,6,45,906,5,6,false],[5,6,45,906,5,6,false],[5,6,45,906,5,6,false],[5,6,85,1006,5,6,false]]"
    );
}
