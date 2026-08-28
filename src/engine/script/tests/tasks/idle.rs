use super::*;

#[test]
fn idle_callbacks_run_after_timer_work_with_a_bounded_deadline() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const order = [];
            requestIdleCallback(deadline => {
                order.push('idle');
                document.body.setAttribute('data-result', [
                    order.join(','),
                    deadline instanceof IdleDeadline,
                    deadline.didTimeout,
                    deadline.timeRemaining() > 0,
                    Object.prototype.toString.call(deadline)
                ].join(':'));
            });
            setTimeout(() => order.push('timer'), 0);
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-result"))
            .as_deref(),
        Some("timer,idle:true:false:true:[object IdleDeadline]")
    );
}

#[test]
fn idle_callback_timeout_and_cancellation_are_observable() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const cancelled = requestIdleCallback(() => {
                document.body.setAttribute('data-cancelled', 'ran');
            });
            cancelIdleCallback(cancelled);
            requestIdleCallback(deadline => {
                document.body.setAttribute('data-timeout', [
                    deadline.didTimeout,
                    deadline.timeRemaining()
                ].join(':'));
            }, { timeout: 10 });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-cancelled"), None);
    assert_eq!(body.attr("data-timeout").as_deref(), Some("true:0"));
}
