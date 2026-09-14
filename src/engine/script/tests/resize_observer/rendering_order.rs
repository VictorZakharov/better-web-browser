use super::*;

#[test]
fn loop_error_mutation_waits_for_animation_frame_before_another_observer_cycle() {
    let (dom, mut runtime) = start(
        r#"
        const target = document.getElementById('target');
        let errors = 0;
        const observer = new ResizeObserver(() => target.style.height = '111px');
        addEventListener('error', event => {
            event.preventDefault();
            errors++;
            target.style.height = '112px';
            requestAnimationFrame(() => {
                document.body.dataset.result = 'frame:' + errors;
                observer.disconnect();
            });
        });
        observer.observe(target);
        "#,
    );
    notify(&mut runtime);
    for _ in 0..3 {
        notify(&mut runtime);
    }
    assert!(
        runtime
            .advance_time(Duration::from_millis(16), 16)
            .errors
            .is_empty()
    );
    assert_eq!(
        result(&dom),
        "frame:1",
        "observer retries must follow frame callbacks"
    );
}

#[test]
fn cancelling_author_frame_does_not_cancel_skipped_observer_delivery() {
    let (dom, mut runtime) = start(
        r#"
        const target = document.getElementById('target');
        let calls = 0;
        const requestFrame = requestAnimationFrame;
        addEventListener('error', event => {
            event.preventDefault();
            const cancelled = requestFrame(() => { throw new Error('cancelled'); });
            cancelAnimationFrame(cancelled);
        });
        new ResizeObserver(() => {
            document.body.dataset.result = ++calls;
            target.style.width = 100 + calls + 'px';
        }).observe(target);
        requestAnimationFrame = setTimeout = () => { throw new Error('author override'); };
        "#,
    );
    notify(&mut runtime);
    notify(&mut runtime);
    assert_eq!(result(&dom), "1");
    assert!(
        runtime
            .advance_time(Duration::from_millis(16), 16)
            .errors
            .is_empty()
    );
    notify(&mut runtime);
    assert_eq!(
        result(&dom),
        "2",
        "the internal rendering opportunity must still run"
    );
    runtime.cancel_document();
    assert_eq!(runtime.next_timer_delay(), None);
    assert!(!runtime.has_pending_resize_observers());
}
