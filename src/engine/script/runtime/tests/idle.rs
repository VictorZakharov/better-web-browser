use super::*;
use crate::engine::script::UserInputEvent;

fn start(code: &str) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(&format!("<body><script>{code}</script></body>"), true);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial_before_document_completion(&script_inputs(&dom), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}

fn tick(runtime: &mut ScriptRuntime, advance_ms: u64) {
    let outcome = runtime.advance_time(Duration::from_millis(advance_ms), 100);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn media_tasks_interrupt_idle_periods_and_finish_microtasks_before_idle_resumes() {
    let (dom, mut runtime) = start(
        r#"
        const video = document.createElement('video');
        document.body.append(video);
        const source = new MediaSource();
        let saved, order = [];
        const record = value => {
            order.push(value); document.body.dataset.order = order.join('|');
        };
        source.addEventListener('sourceopen', () => {
            record('media:' + saved.timeRemaining());
            queueMicrotask(() => record('microtask'));
        });
        requestIdleCallback(deadline => {
            saved = deadline;
            record('idle');
            video.src = URL.createObjectURL(source);
            requestIdleCallback(() => record('next-idle'));
        });
    "#,
    );
    let body = dom.elements_named("body").next().unwrap();
    tick(&mut runtime, 0);
    assert_eq!(body.attr("data-order").as_deref(), Some("idle"));
    tick(&mut runtime, 0);
    assert_eq!(
        body.attr("data-order").as_deref(),
        Some("idle|media:0|microtask")
    );
    tick(&mut runtime, 50);
    assert_eq!(
        body.attr("data-order").as_deref(),
        Some("idle|media:0|microtask|next-idle")
    );
}

#[test]
fn idle_timeout_starts_at_registration_inside_a_running_task() {
    let (dom, mut runtime) = start(
        r#"
        requestIdleCallback(() => {
            const until = Date.now() + 30;
            while (Date.now() < until) {}
            requestIdleCallback(deadline => document.body.dataset.result = String(deadline.didTimeout), {timeout: 100});
        });
    "#,
    );
    tick(&mut runtime, 0);
    tick(&mut runtime, 110);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("false")
    );
}

#[test]
fn idle_reposts_yield_to_input_rendering_and_timers_and_keep_fifo_order() {
    let (dom, mut runtime) = start(
        r#"
        let order = [], saved;
        const record = value => {
            order.push(value); document.body.dataset.order = order.join('|');
        };
        document.body.addEventListener('click', () => record('input:' + saved.timeRemaining()));
        setTimeout(() => record('timer'), 10);
        requestAnimationFrame(() => record('frame'));
        requestIdleCallback(deadline => {
            saved = deadline;
            if (deadline.timeRemaining() > 10) throw Error('deadline passed next timer');
            record('first');
            queueMicrotask(() => record('microtask'));
            requestIdleCallback(() => record('repost'));
        });
        requestIdleCallback(() => record('second'));
    "#,
    );
    let body = dom.elements_named("body").next().unwrap();
    tick(&mut runtime, 0);
    assert_eq!(body.attr("data-order").as_deref(), Some("first|microtask"));
    let result = runtime.dispatch_user_input(UserInputEvent::Simple {
        target: body.clone(),
        event_type: "click",
        bubbles: true,
        cancelable: true,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert_eq!(runtime.next_timer_delay(), Some(Duration::from_millis(10)));
    tick(&mut runtime, 10);
    tick(&mut runtime, 0);
    tick(&mut runtime, 0);
    tick(&mut runtime, 6);
    assert_eq!(
        body.attr("data-order").as_deref(),
        Some("first|microtask|input:0|timer|second|repost|frame")
    );
}

#[test]
fn idle_deadlines_end_when_network_or_rendering_work_is_delivered() {
    for rendering in [false, true] {
        let (dom, mut runtime) = start(
            r#"
            let saved;
            requestIdleCallback(deadline => saved = deadline);
            requestIdleCallback(() => document.body.dataset.result = String(saved.timeRemaining()));
        "#,
        );
        tick(&mut runtime, 0);
        let result = if rendering {
            runtime.notify_layout_changed()
        } else {
            runtime.complete_worker_event_with_loader(123, Ok("null".into()), None)
        };
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(runtime.next_timer_delay(), Some(Duration::from_millis(50)));
        tick(&mut runtime, 50);
        assert_eq!(
            dom.elements_named("body")
                .next()
                .unwrap()
                .attr("data-result")
                .as_deref(),
            Some("0")
        );
    }
}

#[test]
fn idle_cancellation_removes_runnable_and_pending_callbacks_and_document_teardown_clears_all() {
    let (dom, mut runtime) = start(
        r#"
        requestIdleCallback(() => {
            cancelIdleCallback(second);
            const pending = requestIdleCallback(() => { throw Error('pending ran'); });
            cancelIdleCallback(pending);
            document.body.dataset.result = 'first';
        });
        const second = requestIdleCallback(() => { throw Error('runnable ran'); });
        requestIdleCallback(() => document.body.dataset.result = 'survivor', {timeout: 1});
    "#,
    );
    tick(&mut runtime, 0);
    runtime.cancel_document();
    assert_eq!(runtime.next_timer_delay(), None);
    assert!(!runtime.is_active());
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("first")
    );
}

#[test]
fn idle_timeout_zero_is_not_an_expired_timeout_and_clear_timeout_cannot_cancel_idle() {
    let (dom, mut runtime) = start(
        r#"
        const id = requestIdleCallback(deadline => document.body.dataset.result = String(deadline.didTimeout), {timeout: 0});
        clearTimeout(id);
    "#,
    );
    runtime.elapse_time(Duration::from_secs(5));
    tick(&mut runtime, 0);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("false")
    );
}

#[test]
fn idle_timeout_is_decided_when_the_callback_runs_not_when_registered() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        requestIdleCallback(deadline => document.body.dataset.result =
            deadline.didTimeout + ':' + deadline.timeRemaining(), {timeout: 100});
    </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let result = runtime.execute_initial_before_document_completion(&script_inputs(&dom), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    // Simulate the renderer being occupied beyond the timeout before it can select work.
    let result = runtime.advance_time(Duration::from_millis(200), 1);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:0")
    );
}
