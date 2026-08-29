use super::*;
use crate::limits::MAX_DOM_MUTATIONS_PER_TASK;

mod idle;

#[test]
fn match_media_uses_the_document_color_scheme_environment() {
    let dom = crate::engine::dom::parse_with_scripting(
        r#"<body><script>
            document.body.setAttribute('data-result', [
                matchMedia('(prefers-color-scheme: dark)').matches,
                matchMedia('(prefers-color-scheme: light)').matches,
                matchMedia('(min-width: 700px)').matches
            ].join(':'));
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#inline".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_media_environment(800.0, true);
    let outcome = runtime.execute_initial(&[input]);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-result"))
            .as_deref(),
        Some("true:false:true")
    );
}

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
            setTimeout(() => { throw new Error('expected timer failure'); }, 0);
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
}

#[test]
fn dom_mutations_request_one_render_checkpoint() {
    let (_, mutating) = execute_html(
        r#"<body><div id="status"></div><script>
            setTimeout(() => {
                const status = document.getElementById('status');
                status.textContent = 'ready';
                status.setAttribute('data-ready', 'true');
            }, 0);
        </script></body>"#,
    );
    assert!(mutating.errors.is_empty(), "{:?}", mutating.errors);
    assert_eq!(mutating.mutation_count, 2);
    assert!(mutating.render_requested);
    assert_eq!(mutating.invalidation.mutation_count, 2);
    assert!(mutating.invalidation.impact.affects_style());
    assert!(mutating.invalidation.impact.affects_layout());

    let (_, non_mutating) =
        execute_html(r#"<script>setTimeout(() => console.log('ready'), 0);</script>"#);
    assert!(non_mutating.errors.is_empty(), "{:?}", non_mutating.errors);
    assert_eq!(non_mutating.mutation_count, 0);
    assert!(!non_mutating.render_requested);
}

#[test]
fn dom_mutation_budget_is_enforced_per_event_loop_task() {
    let (_, runaway) = execute_html(&format!(
        r#"<body><script>
            for (let i = 0; i < {}; i++) document.body.setAttribute('data-i', String(i));
        </script></body>"#,
        MAX_DOM_MUTATIONS_PER_TASK + 1
    ));
    assert!(
        runaway
            .errors
            .iter()
            .any(|error| error.contains("DOM mutation task budget exceeded")),
        "{:?}",
        runaway.errors
    );
    assert_eq!(runaway.mutation_count, MAX_DOM_MUTATIONS_PER_TASK);

    let per_timer = MAX_DOM_MUTATIONS_PER_TASK * 3 / 5;
    let (dom, bounded) = execute_html(&format!(
        r#"<body><script>
            setTimeout(() => {{
                for (let i = 0; i < {per_timer}; i++)
                    document.body.setAttribute('data-first', String(i));
            }}, 0);
            setTimeout(() => {{
                for (let i = 0; i < {per_timer}; i++)
                    document.body.setAttribute('data-second', String(i));
                document.body.setAttribute('data-complete', 'yes');
            }}, 0);
        </script></body>"#
    ));
    assert!(bounded.errors.is_empty(), "{:?}", bounded.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-complete")
            .as_deref(),
        Some("yes")
    );
    assert!(bounded.mutation_count > MAX_DOM_MUTATIONS_PER_TASK);
}

#[test]
fn detached_nodes_and_script_cleanup_do_not_request_rendering() {
    let (_, outcome) = execute_html(
        r#"<body><script>
            setTimeout(() => {
                const detached = document.createElement('div');
                detached.setAttribute('class', 'unused');
                const loader = document.createElement('script');
                loader.setAttribute('type', 'application/json');
                document.head.appendChild(loader);
                loader.remove();
            }, 0);
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.mutation_count, 4);
    assert!(!outcome.render_requested);
}

#[test]
fn inserting_a_style_element_requests_a_full_rule_refresh() {
    let (_, outcome) = execute_html(
        r#"<body><script>
            setTimeout(() => {
                const style = document.createElement('style');
                style.textContent = 'body { color: #123456 }';
                document.head.appendChild(style);
            }, 0);
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.render_requested);
    assert!(outcome.invalidation.rebuild_style_rules);
}

#[test]
fn records_script_requested_navigation() {
    let (_, outcome) = execute_html(r#"<script>location.replace('/next?q=1')</script>"#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.navigation_url.as_deref(),
        Some("https://example.com/next?q=1")
    );
}

#[test]
fn exposes_javascript_cookie_updates_to_the_network_layer() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="value"></div><script>
            document.cookie = 'SG_SS=proof-token; Path=/; Secure; SameSite=None';
            document.cookie = 'theme=dark; Path=/';
            document.getElementById('value').textContent =
                navigator.cookieEnabled + ':' + document.cookie;
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.cookie_updates,
        [
            "SG_SS=proof-token; Path=/; Secure; SameSite=None",
            "theme=dark; Path=/"
        ]
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true:SG_SS=proof-token; theme=dark"
    );
}

#[test]
fn alternates_timers_and_promise_jobs_until_the_page_settles() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            setTimeout(() => Promise.resolve().then(() => {
                setTimeout(() => document.getElementById('status').textContent = 'ready', 0);
            }), 0);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
}

#[test]
fn exposes_browser_base64_helpers() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="value"></div><script>
            document.getElementById('value').textContent =
                atob('SGVsbG8h') + ':' + btoa('Rust');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "Hello!:UnVzdA=="
    );
}

#[test]
fn exposes_legacy_substr_for_web_compatibility() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="value"></div><script>
            document.getElementById('value').textContent = [
                'https://example.com'.substr(0, 5),
                'abcdef'.substr(-3, 2),
                'abcdef'.substr(2)
            ].join('|');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "https|de|cdef"
    );
}

#[test]
fn propagates_uncatchable_runtime_limits_from_promise_jobs() {
    let (_, outcome) = execute_html(
        r#"<script>
            Promise.resolve().then(() => { for (;;) {} });
        </script>"#,
    );
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("execution time limit")),
        "{:?}",
        outcome.errors
    );
    assert!(!outcome.runtime_stopped);
}

#[test]
fn exposes_a_same_origin_iframe_browsing_context() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="value">no</div><script>
            const frame = document.createElement('iframe');
            document.body.appendChild(frame);
            if (frame.contentWindow !== window &&
                frame.contentWindow.window === frame.contentWindow &&
                frame.contentWindow.parent === window &&
                frame.contentWindow.Array !== Array &&
                frame.contentDocument !== document &&
                frame.contentDocument.defaultView === frame.contentWindow &&
                document.defaultView === window) {
                document.getElementById('value').textContent = 'yes';
            }
            frame.remove();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn mutation_observers_and_reported_microtask_exceptions_share_the_job_queue() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const order = [];
            const text = document.createTextNode('');
            new MutationObserver(() => order.push('mutation'))
                .observe(text, { characterData: true });
            text.data = 'changed';
            queueMicrotask(() => order.push('microtask'));
            queueMicrotask(() => document.body.setAttribute('data-order', order.join(',')));

            const expected = new Error('reported');
            addEventListener('error', event => {
                if (event instanceof ErrorEvent && event.error === expected)
                    document.body.setAttribute('data-error', 'same');
            });
            queueMicrotask(() => { throw expected; });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-order").as_deref(),
        Some("mutation,microtask")
    );
    assert_eq!(body.attr("data-error").as_deref(), Some("same"));
}
