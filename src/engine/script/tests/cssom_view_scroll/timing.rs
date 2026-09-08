use super::*;

#[test]
fn native_scroll_observer_task_sees_nested_promise_geometry_changes_after_input() {
    let (dom, mut runtime) = initialize(
        r#"<!doctype html><body><span id=target style='position:absolute;top:1000px'></span>
        <script>
            const target = document.getElementById('target');
            const events = [];
            new IntersectionObserver(entries => {
                for (const entry of entries) events.push(
                    'observe:' + entry.isIntersecting + ':' + entry.boundingClientRect.top);
            }).observe(target);
            document.addEventListener('scroll', event => {
                event.stopPropagation();
                event.stopImmediatePropagation();
                if (!event.isTrusted) return;
                events.push('scroll');
                Promise.resolve().then(() => {
                    events.push('microtask-one');
                    return Promise.resolve().then(() => {
                        target.style.top = '800px';
                        events.push('microtask-two');
                    });
                });
            });
            document.dispatchEvent(new Event('scroll', {bubbles:true}));
        </script></body>"#,
        |dom| {
            HashMap::from([(
                dom.elements_named("span").next().unwrap().id(),
                box_at(10.0, 1000.0),
            )])
        },
    );
    let initial = runtime.notify_layout_changed();
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let target = dom.elements_named("span").next().unwrap();
    let flushes = Rc::new(Cell::new(0));
    let observed_flushes = Rc::clone(&flushes);
    runtime.set_layout_flush_callback(Box::new(move |_, _| {
        observed_flushes.set(observed_flushes.get() + 1);
        assert!(target.attr("style").unwrap().contains("800px"));
        Some(HashMap::from([(target.id(), box_at(10.0, 800.0))]))
    }));

    let input = runtime.dispatch_user_input(UserInputEvent::Scroll { x: 0.0, y: 600.0 });
    assert!(
        input.outcome.errors.is_empty(),
        "{:?}",
        input.outcome.errors
    );
    assert_eq!(
        flushes.get(),
        0,
        "input must not deliver the observer task early"
    );
    evaluate(
        &dom,
        &mut runtime,
        "document.body.dataset.result = events.join(',');",
    );
    assert_eq!(
        result(&dom),
        "observe:false:1000,scroll,microtask-one,microtask-two"
    );

    let notified = runtime.notify_layout_changed();
    assert!(notified.errors.is_empty(), "{:?}", notified.errors);
    let unchanged = runtime.notify_layout_changed();
    assert!(unchanged.errors.is_empty(), "{:?}", unchanged.errors);
    assert_eq!(
        flushes.get(),
        1,
        "only the changed geometry should need a flush"
    );
    evaluate(
        &dom,
        &mut runtime,
        "document.body.dataset.result = events.join(',');",
    );
    assert_eq!(
        result(&dom),
        "observe:false:1000,scroll,microtask-one,microtask-two,observe:true:200"
    );
}
