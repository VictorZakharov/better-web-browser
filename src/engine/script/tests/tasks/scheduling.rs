//! Timer, microtask, and animation-frame event-loop ordering.

use super::*;

#[test]
fn drains_short_timers_before_layout() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            setTimeout(() => document.getElementById('status').textContent = 'ready', 20);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
}

#[test]
fn settles_bounded_one_second_startup_timers() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            setTimeout(() => document.getElementById('status').textContent = 'ready', 500);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
}

#[test]
fn settles_nested_startup_poll_within_the_explicit_horizon() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            setTimeout(() => {
                setTimeout(() => {
                    document.getElementById('status').textContent = 'ready';
                }, 100);
            }, 1200);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
}

#[test]
fn rescheduled_short_timer_does_not_starve_later_startup_timer() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            function poll() { setTimeout(poll, 300); }
            setTimeout(poll, 300);
            setTimeout(() => document.getElementById('status').textContent = 'ready', 1000);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
}

#[test]
fn runs_a_microtask_checkpoint_between_same_deadline_timer_tasks() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const order = [];
            setTimeout(() => {
                order.push('timer-one');
                queueMicrotask(() => order.push('microtask'));
            }, 0);
            setTimeout(() => {
                order.push('timer-two');
                document.getElementById('status').textContent = order.join(',');
            }, 0);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "timer-one,microtask,timer-two"
    );
}

#[test]
fn clear_timeout_cancels_the_rust_scheduled_task() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const cancelled = setTimeout(() => {
                document.getElementById('status').textContent = 'cancelled task ran';
            }, 10);
            clearTimeout(cancelled);
            setTimeout(() => {
                document.getElementById('status').textContent = 'ready';
            }, 10);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
}

#[test]
fn animation_frame_callbacks_share_one_rendering_step_and_do_not_starve_after_mutation() {
    let (dom, outcome) = execute_html(
        r#"<body data-result="waiting"><script>
            let firstTimestamp = -1;
            requestAnimationFrame(timestamp => {
                firstTimestamp = timestamp;
                document.body.setAttribute('data-mutated', 'true');
            });
            requestAnimationFrame(timestamp => {
                document.body.setAttribute('data-result', [
                    timestamp === firstTimestamp,
                    document.body.getAttribute('data-mutated')
                ].join(':'));
            });
            const cancelled = requestAnimationFrame(() => {
                document.body.setAttribute('data-result', 'cancelled callback ran');
            });
            cancelAnimationFrame(cancelled);
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-result"))
            .as_deref(),
        Some("true:true")
    );
}

#[test]
fn animation_frames_requested_during_a_callback_run_on_a_later_frame() {
    let (dom, outcome) = execute_html(
        r#"<body data-result="waiting"><script>
            requestAnimationFrame(first => {
                requestAnimationFrame(second => {
                    document.body.setAttribute('data-result', String(second >= first));
                });
            });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-result"))
            .as_deref(),
        Some("true")
    );
}

#[test]
fn clear_interval_stops_a_rescheduled_repeating_task() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            let count = 0;
            const interval = setInterval(() => {
                count++;
                if (count === 3) {
                    clearInterval(interval);
                    document.getElementById('status').textContent = String(count);
                }
            }, 10);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "3"
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|message| !message.contains("timers after settling")),
        "{:?}",
        outcome.diagnostics
    );
}

#[test]
fn a_throwing_timer_does_not_prevent_the_next_task() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            setTimeout(() => {
                const error = new Error('expected timer failure');
                error.args = [{ error: 'nested platform failure', event: 'onStateChange' }];
                throw error;
            }, 0);
            setTimeout(() => {
                document.getElementById('status').textContent = 'ready';
            }, 0);
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("expected timer failure")),
        "{:?}",
        outcome.errors
    );
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("https://example.com/#inline")),
        "{:?}",
        outcome.errors
    );
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("error=nested platform failure; event=onStateChange")),
        "{:?}",
        outcome.errors
    );
}
