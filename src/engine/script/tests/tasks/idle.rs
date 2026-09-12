use super::*;

#[test]
fn idle_web_idl_arguments_are_converted_once_without_coercing_callback_objects() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        let rejected = 0, reads = 0;
        const throws = f => { try { f(); } catch(e) { if (e instanceof TypeError) rejected++; } };
        throws(() => requestIdleCallback());
        throws(() => requestIdleCallback({handleEvent() {}}));
        throws(() => requestIdleCallback(() => {}, 1));
        throws(() => requestIdleCallback(() => {}, {timeout: 1n}));
        throws(() => requestIdleCallback(() => {}, {timeout: Symbol()}));
        throws(() => cancelIdleCallback());
        throws(() => cancelIdleCallback(1n));
        throws(() => new IdleDeadline());
        throws(() => requestIdleCallback.call({}, () => {}));
        throws(() => cancelIdleCallback.call({}, 1));
        const id = requestIdleCallback(() => { throw Error('cancelled callback ran'); },
            {get timeout() { reads++; return {valueOf() { reads++; return 3.8; }}; }});
        const result = cancelIdleCallback({valueOf() { reads++; return id + 4294967296; }});
        document.body.dataset.result = [rejected, reads, result === undefined,
            requestIdleCallback.length, cancelIdleCallback.length].join(':');
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("10:3:true:1:1")
    );
}

#[test]
fn cancelling_an_idle_handle_does_not_cancel_an_ordinary_timer() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        const timer = setTimeout(() => document.body.dataset.timer = 'ran', 0);
        cancelIdleCallback(timer);
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-timer")
            .as_deref(),
        Some("ran")
    );
}

#[test]
fn idle_deadline_has_private_state_and_validates_receivers() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        requestIdleCallback(deadline => {
            deadline.__didTimeout = 'spoofed';
            deadline.__deadline = Infinity;
            Date.now = () => Infinity;
            performance.now = () => Infinity;
            let rejected = 0;
            try { deadline.timeRemaining.call({}); } catch (e) { rejected += e instanceof TypeError; }
            try { Object.getOwnPropertyDescriptor(IdleDeadline.prototype, 'didTimeout').get.call({}); }
            catch (e) { rejected += e instanceof TypeError; }
            document.body.dataset.idle = [typeof deadline.didTimeout, Number.isFinite(deadline.timeRemaining()), rejected].join(':');
        });
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-idle")
            .as_deref(),
        Some("boolean:true:2")
    );
}

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
fn idle_callback_can_win_before_its_timeout_and_cancellation_is_observable() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const cancelled = requestIdleCallback(() => {
                document.body.setAttribute('data-cancelled', 'ran');
            });
            cancelIdleCallback(cancelled);
            requestIdleCallback(deadline => {
                document.body.setAttribute('data-timeout', [
                    deadline.didTimeout,
                    deadline.timeRemaining() >= 0 && deadline.timeRemaining() <= 50
                ].join(':'));
            }, { timeout: 10 });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-cancelled"), None);
    assert_eq!(body.attr("data-timeout").as_deref(), Some("false:true"));
}
